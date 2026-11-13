"""
Rust 코어(beep-gen CLI)와 기존 Python 구현(generator.py)의 비프 합성 결과를 비교한다.

사용법: python tools/parity_check.py <beep-gen 실행 파일 경로>

두 구현은 샘플을 정수로 바꾸는 방식이 달라(Python: 버림, Rust: 반올림) 값이 1만큼
다를 수 있다. 그래서 "길이가 같고, 모든 샘플의 차이가 1 이하" 를 일치로 본다.

Python 구현이 간격을 정확히 계산하지 못하는 설정은 비교 대상에서 뺀다. Python 구현은
간격(샘플)을 버림한 뒤 박 번호를 곱하므로, 간격이 정수가 아니거나 부동소수점 오차로
정수 바로 아래가 되면(예: 1.4초/2분할 = 30869.999… → 30869) 뒤로 갈수록 박이 밀린다.
Rust 구현은 박 위치를 매번 반올림해 계산하므로 밀리지 않는다(의도된 차이).
"""

import array
import os
import subprocess
import sys
import tempfile
import wave

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)

import generator  # noqa: E402

SAMPLE_RATE = 44100

# (n, m, 약한 비프 볼륨 %, 길이 초)
CASES = [
    (1.0, 2, 35.0, 10),
    (1.0, 1, 35.0, 5),
    (2.0, 4, 25.0, 12),
    (1.4, 2, 35.0, 30),
    (1.0, 3, 100.0, 7),
    (0.5, 2, 0.0, 5),
    (0.1, 2, 50.0, 3),
    (1.0, 2, 35.0, 300),
]


def read_wav(path):
    with wave.open(path, "rb") as wf:
        assert wf.getnchannels() == 1, "모노가 아닙니다"
        assert wf.getsampwidth() == 2, "16비트가 아닙니다"
        assert wf.getframerate() == SAMPLE_RATE, "샘플 레이트가 다릅니다"
        samples = array.array("h")
        samples.frombytes(wf.readframes(wf.getnframes()))
    if sys.byteorder == "big":
        samples.byteswap()
    return samples


def python_reference(n, m, weak, length):
    samples = array.array("h")
    samples.frombytes(generator.generate_beep_sequence(n, m, weak, length, SAMPLE_RATE))
    if sys.byteorder == "big":
        samples.byteswap()
    return samples


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    exe = os.path.abspath(sys.argv[1])
    failures = 0

    with tempfile.TemporaryDirectory() as tmp:
        for n, m, weak, length in CASES:
            label = f"n={n:g} m={m} v={weak:g} l={length}"
            # generator.py 와 같은 식으로 계산한 간격이 실제 간격과 다르면 Python 쪽이 밀린다.
            python_step = int(n / float(m) * SAMPLE_RATE)
            exact_step = round(n * SAMPLE_RATE) / m
            if python_step != exact_step:
                print(f"SKIP  {label}  (Python 간격 {python_step} ≠ 실제 {exact_step:g} 샘플: 의도된 차이)")
                continue

            out = os.path.join(tmp, "out.wav")
            subprocess.run(
                [exe, "-n", str(n), "-m", str(m), "-v", str(weak), "-l", str(length), "-o", out, "-q"],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            rust = read_wav(out)
            ref = python_reference(n, m, weak, length)

            if len(rust) != len(ref):
                print(f"FAIL  {label}  길이 다름: rust={len(rust)} python={len(ref)}")
                failures += 1
                continue
            worst = max(abs(a - b) for a, b in zip(rust, ref))
            different = sum(1 for a, b in zip(rust, ref) if a != b)
            status = "OK  " if worst <= 1 else "FAIL"
            if worst > 1:
                failures += 1
            print(f"{status}  {label}  샘플 {len(rust)}개, 최대 차이 {worst}, 다른 샘플 {different}개")

    if failures:
        print(f"\n{failures}건 불일치")
        return 1
    print("\n모두 일치")
    return 0


if __name__ == "__main__":
    sys.exit(main())
