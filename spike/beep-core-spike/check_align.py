"""변환 결과의 킥(저역) 위치가 비프 격자에서 얼마나 벗어나는지 측정: check_align.py <out.mp3> <박 간격(초)>"""
import subprocess, sys
import numpy as np
path, step = sys.argv[1], float(sys.argv[2])
raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", path, "-af", "lowpass=f=120,lowpass=f=120",
                      "-ac", "1", "-f", "f32le", "-"], capture_output=True, check=True).stdout
sr = 44100; x = np.abs(np.frombuffer(raw, "<f4")); hop = 32
e = x[: len(x) // hop * hop].reshape(-1, hop).max(1)
total = int(len(x) / sr / step) - 1
offs = []
for k in range(1, total):
    c = int(k * step * sr / hop); w = int(min(0.2, step * 0.4) * sr / hop)
    seg = e[max(0, c - w): c + w]
    if seg.max() < e.max() * 0.2: continue
    i = np.argmax(seg >= seg.max() * 0.5)
    offs.append((k, ((max(0, c - w) + i) * hop / sr - k * step) * 1000))
o = np.array([v for _, v in offs])
q = len(o) // 4
print(f"{path}: beats={len(o)} first-quarter mean={o[:q].mean():+.1f}ms last-quarter mean={o[-q:].mean():+.1f}ms "
      f"drift={o[-q:].mean()-o[:q].mean():+.1f}ms max|off|={np.abs(o).max():.1f}ms")
