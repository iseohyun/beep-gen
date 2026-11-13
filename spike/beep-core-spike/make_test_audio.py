"""검증용 합성 음악 생성: 100 BPM, 5분, 스테레오 44.1kHz (킥·스네어·하이햇·베이스·코드)."""
import sys, wave
import numpy as np

sr, bpm, seconds = 44100, float(sys.argv[2]) if len(sys.argv) > 2 else 100.0, 300
beat = 60.0 / bpm
n = sr * seconds
t = np.arange(n) / sr
rng = np.random.default_rng(7)
L = np.zeros(n); R = np.zeros(n)

def add(buf, start, sig):
    i = int(round(start * sr))
    if i >= n: return
    e = min(i + len(sig), n)
    buf[i:e] += sig[: e - i]

def env(length, decay):
    return np.exp(-np.arange(int(length * sr)) / (decay * sr))

kick_t = np.arange(int(0.25 * sr)) / sr
kick = np.sin(2 * np.pi * (45 * kick_t + 60 * (1 - np.exp(-kick_t * 30)) / 30)) * env(0.25, 0.07)
snare = (rng.standard_normal(int(0.18 * sr)) * 0.5 + np.sin(2 * np.pi * 190 * np.arange(int(0.18 * sr)) / sr) * 0.4) * env(0.18, 0.05)
hat = rng.standard_normal(int(0.05 * sr)) * env(0.05, 0.012)
hat = np.diff(hat, prepend=0.0)

chords = [(110.0, [220.0, 261.63, 329.63]), (87.31, [174.61, 220.0, 261.63]),
          (130.81, [261.63, 329.63, 392.0]), (98.0, [196.0, 246.94, 293.66])]
total_beats = int(seconds / beat)
for b in range(total_beats):
    st = b * beat
    for buf in (L, R):
        add(buf, st, kick * 0.8)
        if b % 4 in (1, 3): add(buf, st, snare * 0.45)
    add(L, st, hat * 0.18); add(R, st + beat / 2, hat * 0.22)
    add(L, st + beat / 2, hat * 0.10); add(R, st, hat * 0.10)
    root, notes = chords[(b // 4) % 4]
    seg = np.arange(int(beat * sr)) / sr
    bass = np.sin(2 * np.pi * root * seg) * np.minimum(1, seg * 200) * np.exp(-seg * 3) * 0.35
    add(L, st, bass); add(R, st, bass)
    if b % 4 == 0:
        bar = np.arange(int(beat * 4 * sr)) / sr
        fade = np.minimum(1, bar * 8) * np.minimum(1, (beat * 4 - bar) * 8)
        for k, f in enumerate(notes):
            pad = (np.sin(2 * np.pi * f * bar) + 0.3 * np.sin(2 * np.pi * 2 * f * bar)) * fade * 0.07
            add(L, st, pad * (1.0 - 0.2 * k)); add(R, st, pad * (0.6 + 0.2 * k))

st = np.stack([L, R], axis=1)
st = st / np.max(np.abs(st)) * 0.9
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(sr)
    w.writeframes((st * 32767).astype("<i2").tobytes())
print("ok", sys.argv[1], bpm, "bpm")
