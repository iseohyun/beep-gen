"""
Beep Audio Generator Module
- 순수 Python DSP 기반 고품질 비프음 합성 (Anti-click Envelope)
- Edge-TTS / gTTS / Windows SAPI5 다중 TTS 엔진 연동
- FFmpeg 기반 MP3 인코딩 및 WAV 내보내기 지원
"""

import math
import os
import shutil
import struct
import subprocess
import tempfile
import wave
from typing import Callable, Optional

import tts_engine


def is_ffmpeg_available() -> bool:
    """FFmpeg 실행 가능 여부 확인"""
    return shutil.which("ffmpeg") is not None


def resample_pcm16_mono(data: bytes, src_rate: int, dst_rate: int) -> bytes:
    """16-bit Mono PCM 단순 선형 보간 리샘플링"""
    if src_rate == dst_rate or not data:
        return data

    num_samples = len(data) // 2
    if num_samples == 0:
        return b""

    samples = struct.unpack(f"<{num_samples}h", data)
    dst_num_samples = int(num_samples * dst_rate / src_rate)

    resampled = []
    ratio = src_rate / dst_rate
    for i in range(dst_num_samples):
        src_pos = i * ratio
        idx = int(src_pos)
        frac = src_pos - idx
        if idx >= num_samples - 1:
            val = samples[-1]
        else:
            val = int(samples[idx] * (1.0 - frac) + samples[idx + 1] * frac)
        val = max(-32768, min(32767, val))
        resampled.append(val)

    return struct.pack(f"<{len(resampled)}h", *resampled)


def read_tts_audio_data(wav_path: str, target_rate: int = 44100) -> bytes:
    """TTS WAV 파일을 읽고 모노 44100Hz 16-bit PCM 바이트로 변환"""
    if not os.path.exists(wav_path):
        return b""

    try:
        with wave.open(wav_path, "rb") as wf:
            nchannels = wf.getnchannels()
            sampwidth = wf.getsampwidth()
            framerate = wf.getframerate()
            nframes = wf.getnframes()
            raw_frames = wf.readframes(nframes)

        # 16-bit PCM 변환
        if sampwidth == 1:
            samples_8 = struct.unpack(f"{len(raw_frames)}B", raw_frames)
            samples_16 = [(s - 128) * 256 for s in samples_8]
        elif sampwidth == 2:
            num_samples = len(raw_frames) // 2
            samples_16 = list(struct.unpack(f"<{num_samples}h", raw_frames))
        else:
            return b""

        # 모노 변환
        if nchannels > 1:
            mono_samples = []
            for i in range(0, len(samples_16), nchannels):
                avg = sum(samples_16[i : i + nchannels]) // nchannels
                mono_samples.append(avg)
            samples_16 = mono_samples

        pcm_data = struct.pack(f"<{len(samples_16)}h", *samples_16)
        return resample_pcm16_mono(pcm_data, framerate, target_rate)
    except Exception as e:
        print(f"[WAV Read Error] {e}")
        return b""


def create_tone(
    frequency: float,
    duration: float,
    volume: float,
    sample_rate: int = 44100,
    fade_sec: float = 0.005,
) -> list[int]:
    """
    주파수, 지속시간, 볼륨(0.0~1.0)에 따른 사인파 생성 (Attack/Release Fade 엔벨로프 포함)
    """
    num_samples = int(duration * sample_rate)
    if num_samples <= 0:
        return []

    fade_samples = min(int(fade_sec * sample_rate), num_samples // 2)
    max_amp = 32767.0 * volume * 0.95  # 클리핑 방지 여유 5%

    samples = []
    two_pi_f = 2.0 * math.pi * frequency

    for i in range(num_samples):
        t = i / sample_rate
        val = math.sin(two_pi_f * t)

        # Attack / Release Envelope
        if i < fade_samples and fade_samples > 0:
            env = i / fade_samples
        elif i >= num_samples - fade_samples and fade_samples > 0:
            env = (num_samples - 1 - i) / fade_samples
        else:
            env = 1.0

        sample_val = int(val * max_amp * env)
        samples.append(max(-32768, min(32767, sample_val)))

    return samples


def generate_beep_sequence(
    n: float,
    m: int,
    weak_volume_pct: float,
    length_sec: int,
    sample_rate: int = 44100,
    primary_freq: float = 880.0,
    sub_freq: float = 587.33,
    beep_duration: float = 0.08,
    progress_callback: Optional[Callable[[int, str], None]] = None,
) -> bytes:
    """
    n초 주기, m개 쪼개기 비프음 시퀀스 합성 (16-bit Mono PCM bytes 반환)
    """
    total_samples = int(length_sec * sample_rate)
    if total_samples <= 0:
        return b""

    step_sec = n / float(m)
    step_samples = int(step_sec * sample_rate)

    # 비프음 길이 안전 보정
    actual_beep_dur = min(beep_duration, step_sec * 0.6)

    # 사전 계산된 메인 비프 및 서브 비프 톤
    primary_volume = 1.0
    sub_volume = max(0.0, min(100.0, weak_volume_pct)) / 100.0

    primary_tone = create_tone(primary_freq, actual_beep_dur, primary_volume, sample_rate)
    sub_tone = create_tone(sub_freq, actual_beep_dur, sub_volume, sample_rate) if m > 1 else []

    buffer = [0] * total_samples
    total_beats = int(length_sec / step_sec) + 1

    for beat_idx in range(total_beats):
        start_sample = int(beat_idx * step_samples)
        if start_sample >= total_samples:
            break

        is_primary = (beat_idx % m) == 0
        tone = primary_tone if is_primary else sub_tone

        tone_len = len(tone)
        end_sample = min(start_sample + tone_len, total_samples)
        for i in range(start_sample, end_sample):
            buffer[i] = tone[i - start_sample]

        if progress_callback and beat_idx % 50 == 0:
            pct = int((start_sample / total_samples) * 70)
            progress_callback(pct, f"비프음 합성 중... ({beat_idx}/{total_beats} 비트)")

    return struct.pack(f"<{len(buffer)}h", *buffer)


def build_audio_file(
    n: float = 1.0,
    m: int = 2,
    weak_volume_pct: float = 35.0,
    length_sec: int = 300,
    tts_template: str = "{n}초, {n}초 비프",
    tts_engine_name: str = "edge_tts",
    tts_voice: str = "ko-KR-SunHiNeural",
    output_path: str = "output.mp3",
    primary_freq: float = 880.0,
    sub_freq: float = 587.33,
    sample_rate: int = 44100,
    progress_callback: Optional[Callable[[int, str], None]] = None,
) -> str:
    """
    TTS 인트로 + 비프음 시퀀스를 결합하여 MP3 또는 WAV 파일 생성
    """
    if progress_callback:
        progress_callback(5, "TTS 인트로 음성 생성 중...")

    # n 표기 문자열 (예: 1.0 -> 1, 1.5 -> 1.5)
    n_display = f"{n:g}" if isinstance(n, (int, float)) else str(n)

    # 1. TTS 생성
    tts_text = tts_template.format(n=n_display, m=m, l=length_sec).strip() if tts_template else ""
    tts_pcm = b""

    if tts_text:
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tf:
            temp_tts_path = tf.name

        try:
            if tts_engine.synthesize_tts_audio(tts_text, tts_engine_name, tts_voice, temp_tts_path):
                tts_pcm = read_tts_audio_data(temp_tts_path, sample_rate)
        finally:
            if os.path.exists(temp_tts_path):
                try:
                    os.remove(temp_tts_path)
                except OSError:
                    pass

    if progress_callback:
        progress_callback(20, "비프음 시퀀스 합성 시작...")

    # 2. 비프 시퀀스 생성
    def sub_progress(p, msg):
        if progress_callback:
            mapped_p = 20 + int(p * 0.6)
            progress_callback(mapped_p, msg)

    beeps_pcm = generate_beep_sequence(
        n=n,
        m=m,
        weak_volume_pct=weak_volume_pct,
        length_sec=length_sec,
        sample_rate=sample_rate,
        primary_freq=primary_freq,
        sub_freq=sub_freq,
        progress_callback=sub_progress,
    )

    # 3. 결합 (TTS + 0.5초 무음 + Beeps)
    silence_samples = int(sample_rate * 0.5) if tts_pcm else 0
    silence_pcm = struct.pack(f"<{silence_samples}h", *([0] * silence_samples))

    combined_pcm = tts_pcm + silence_pcm + beeps_pcm

    if progress_callback:
        progress_callback(85, "오디오 파일 인코딩 중...")

    # 임시 WAV 저장
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tf:
        temp_wav_path = tf.name

    try:
        with wave.open(temp_wav_path, "wb") as wf:
            wf.setnchannels(1)  # Mono
            wf.setsampwidth(2)  # 16-bit
            wf.setframerate(sample_rate)
            wf.writeframes(combined_pcm)

        out_ext = os.path.splitext(output_path)[1].lower()

        # MP3 인코딩
        if out_ext == ".mp3":
            if is_ffmpeg_available():
                cmd = [
                    "ffmpeg",
                    "-y",
                    "-i",
                    temp_wav_path,
                    "-codec:a",
                    "libmp3lame",
                    "-b:a",
                    "192k",
                    output_path,
                ]
                res = subprocess.run(cmd, capture_output=True, text=True)
                if res.returncode != 0:
                    fallback_path = os.path.splitext(output_path)[0] + ".wav"
                    shutil.copyfile(temp_wav_path, fallback_path)
                    output_path = fallback_path
            else:
                fallback_path = os.path.splitext(output_path)[0] + ".wav"
                shutil.copyfile(temp_wav_path, fallback_path)
                output_path = fallback_path
        else:
            shutil.copyfile(temp_wav_path, output_path)

    finally:
        if os.path.exists(temp_wav_path):
            try:
                os.remove(temp_wav_path)
            except OSError:
                pass

    if progress_callback:
        progress_callback(100, f"완료: {os.path.basename(output_path)}")

    return output_path


def generate_preview_wav(
    n: float = 1.0,
    m: int = 2,
    weak_volume_pct: float = 35.0,
    preview_len: float = 5.0,
    primary_freq: float = 880.0,
    sub_freq: float = 587.33,
    sample_rate: int = 44100,
) -> str:
    """
    설정된 파라미터로 5초간의 비프음 미리듣기용 임시 WAV 파일 생성
    """
    beeps_pcm = generate_beep_sequence(
        n=n,
        m=m,
        weak_volume_pct=weak_volume_pct,
        length_sec=int(preview_len),
        sample_rate=sample_rate,
        primary_freq=primary_freq,
        sub_freq=sub_freq,
    )

    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tf:
        preview_path = tf.name

    with wave.open(preview_path, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(beeps_pcm)

    return preview_path


def generate_tts_preview_wav(
    text: str,
    engine_key: str,
    voice_id: str,
) -> Optional[str]:
    """
    TTS 문구 음성 미리듣기용 임시 WAV 파일 생성
    """
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tf:
        preview_path = tf.name

    if tts_engine.synthesize_tts_audio(text, engine_key, voice_id, preview_path):
        return preview_path
    else:
        if os.path.exists(preview_path):
            try:
                os.remove(preview_path)
            except OSError:
                pass
        return None
