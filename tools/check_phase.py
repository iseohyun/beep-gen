"""
실제 음악에서 박이 비프 격자와 맞는지 구간별로 잰다(킥이 뚜렷하지 않은 곡에도 쓸 수 있다).

같은 설정으로 두 번 만든 WAV 를 비교한다.
  - 음악만:  beep-gen ... --music 곡 --beep-volume 0   -o music_only.wav
  - 비프만:  beep-gen ... --music 곡 --music-volume 0  -o beeps_only.wav

사용법: python tools/check_phase.py <music_only.wav> <beeps_only.wav> [구간 길이(초)=20]   (ffmpeg, numpy 필요)

음악에서 소리가 갑자기 커지는 정도(온셋 세기)를 구해, 비프 간격으로 접어 겹친다.
박이 격자에 맞으면 겹친 모양의 봉우리가 구간마다 같은 자리(0ms 근처)에 선다.
봉우리 위치가 구간마다 옮겨 가면 뒤로 갈수록 어긋난다는 뜻이다. 양수는 음악이 늦다는 뜻이다.
"또렷함" 이 낮은 구간(드럼이 없는 부분 등)은 값이 흔들릴 수 있다.
"""

import subprocess
import sys

import numpy as np

SR = 22050
HOP = 64  # 약 2.9ms


def load(path):
    cmd = ["ffmpeg", "-loglevel", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"]
    return np.frombuffer(subprocess.run(cmd, capture_output=True, check=True).stdout, "<f4")


def onset_strength(x):
    """대역별 에너지가 늘어난 양의 합(스펙트럼 플럭스)."""
    size = 1024
    frames = 1 + (len(x) - size) // HOP
    window = np.hanning(size)
    previous = None
    out = np.zeros(frames)
    for i in range(frames):
        spectrum = np.log1p(50 * np.abs(np.fft.rfft(x[i * HOP : i * HOP + size] * window)))
        if previous is not None:
            out[i] = np.maximum(spectrum - previous, 0).sum()
        previous = spectrum
    return out


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    music, beeps = load(sys.argv[1]), load(sys.argv[2])
    section = float(sys.argv[3]) if len(sys.argv) > 3 else 20.0

    # 비프의 시작 시각으로 격자(간격과 기준)를 구한다.
    level = np.abs(beeps)
    loud = level > level.max() * 0.05
    starts = np.flatnonzero(loud & ~np.roll(loud, 1))
    starts = starts[np.insert(np.diff(starts) > SR * 0.05, 0, True)] / SR
    if len(starts) < 4:
        print("비프를 충분히 찾지 못했습니다")
        return 1
    step, origin = np.polyfit(np.arange(len(starts)), starts, 1)

    strength = onset_strength(music)
    times = (np.arange(len(strength)) * HOP + 512) / SR
    bins = 120
    print(f"비프 간격 {step * 1000:.2f}ms, 구간 {section:.0f}초씩")
    print("  구간        봉우리 위치   또렷함")
    peaks = []
    for start in np.arange(0, times[-1], section):
        pick = (times >= start) & (times < start + section)
        if pick.sum() < 100:
            continue
        phase = ((times[pick] - origin) / step) % 1.0
        folded = np.bincount((phase * bins).astype(int) % bins, strength[pick], bins)
        folded = np.convolve(np.tile(folded, 3), np.ones(3) / 3, "same")[bins : 2 * bins]
        peak = int(np.argmax(folded))
        offset = ((peak + 0.5) / bins + 0.5) % 1.0 - 0.5
        clarity = folded.max() / (folded.mean() + 1e-12)
        peaks.append((offset * step * 1000, clarity))
        print(f"  {start:5.0f}~{start + section:<5.0f} {offset * step * 1000:+8.1f}ms   {clarity:5.2f}")
    clear = [p for p, c in peaks if c >= 1.5]
    if clear:
        print(f"또렷한 구간 {len(clear)}개: 가운데값 {np.median(clear):+.1f}ms, 범위 {min(clear):+.1f} ~ {max(clear):+.1f}ms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
