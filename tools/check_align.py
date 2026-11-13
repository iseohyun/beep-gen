"""
배경 음악의 박이 비프와 얼마나 잘 맞는지 측정한다.

같은 설정으로 두 번 만든 WAV 를 비교한다.
  - 음악만:  beep-gen ... --music 곡 --beep-volume 0   -o music_only.wav
  - 비프만:  beep-gen ... --music 곡 --music-volume 0  -o beeps_only.wav

사용법: python tools/check_align.py <music_only.wav> <beeps_only.wav>   (ffmpeg, numpy 필요)

음악의 킥(저역) 시작 시각이 비프 격자에서 얼마나 벗어나는지 잰다(중앙값 기준).
양수는 음악이 비프보다 늦다는 뜻이다. 킥이 뚜렷한 곡에서만 의미가 있다.
"""

import subprocess
import sys

import numpy as np

SR = 44100
HOP = 16


def envelope(path, audio_filter):
    cmd = ["ffmpeg", "-loglevel", "error", "-i", path]
    if audio_filter:
        cmd += ["-af", audio_filter]
    cmd += ["-ac", "1", "-ar", str(SR), "-f", "f32le", "-"]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    x = np.abs(np.frombuffer(raw, "<f4"))
    return x[: len(x) // HOP * HOP].reshape(-1, HOP).max(1)


def onsets(env, threshold_ratio, min_gap_sec):
    """조용한 구간 뒤에 threshold 를 넘는 지점들의 시각(초)."""
    threshold = env.max() * threshold_ratio
    above = env >= threshold
    gap = int(min_gap_sec * SR / HOP)
    times = []
    last = -gap
    for i in np.flatnonzero(above & ~np.roll(above, 1)):
        if i - last >= gap:
            times.append(i * HOP / SR)
        last = i
    return np.array(times)


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    music_path, beeps_path = sys.argv[1], sys.argv[2]

    beeps = onsets(envelope(beeps_path, None), 0.02, 0.05)
    kicks = onsets(envelope(music_path, "lowpass=f=120,lowpass=f=120"), 0.25, 0.15)
    if len(beeps) < 4 or len(kicks) < 4:
        print(f"박을 충분히 찾지 못했습니다 (비프 {len(beeps)}개, 킥 {len(kicks)}개)")
        return 1

    # 비프 시작 시각에 직선을 맞춰 정확한 간격과 기준 시각을 구한다.
    step, origin = np.polyfit(np.arange(len(beeps)), beeps, 1)
    # 음악의 박이 비프보다 촘촘하면(예: 비프 사이에 박이 2개) 그만큼 나눈 격자와 비교한다.
    kick_interval = float(np.median(np.diff(kicks)))
    grid = step / max(1, round(step / kick_interval))
    offsets = ((kicks - origin + grid / 2) % grid - grid / 2) * 1000

    quarter = max(1, len(offsets) // 4)
    first, last = np.median(offsets[:quarter]), np.median(offsets[-quarter:])
    print(
        f"비프 간격 {step * 1000:.2f}ms, 킥 {len(kicks)}개(격자 {grid * 1000:.1f}ms) | "
        f"앞 1/4 {first:+.1f}ms, 뒤 1/4 {last:+.1f}ms, "
        f"누적 어긋남 {last - first:+.1f}ms, 95% 범위 ±{np.percentile(np.abs(offsets), 95):.1f}ms"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
