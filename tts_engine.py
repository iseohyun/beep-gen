"""
TTS Engine Manager Module
- Edge-TTS (Microsoft Neural AI Voices)
- Google TTS (gTTS)
- Windows SAPI5 (Offline Installed Voices)
"""

import asyncio
import os
import shutil
import subprocess
import tempfile
from typing import Dict, List, Tuple


def is_ffmpeg_available() -> bool:
    return shutil.which("ffmpeg") is not None


def get_sapi5_voices() -> List[Tuple[str, str]]:
    """Windows SAPI5에 설치된 음성 목록 반환 [(VoiceName, DisplayLabel), ...]"""
    ps_code = """
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
foreach ($v in $synth.GetInstalledVoices()) {
    Write-Output "$($v.VoiceInfo.Name)|$($v.VoiceInfo.Culture.Name)|$($v.VoiceInfo.Gender)"
}
"""
    try:
        res = subprocess.run(
            ["powershell", "-NoProfile", "-Command", ps_code],
            capture_output=True,
            text=True,
            timeout=10,
        )
        voices = []
        for line in res.stdout.strip().splitlines():
            parts = line.strip().split("|")
            if len(parts) >= 3:
                vname, culture, gender = parts[0], parts[1], parts[2]
                label = f"{vname} ({culture}, {gender})"
                voices.append((vname, label))
        if not voices:
            voices = [("Microsoft Heami Desktop", "Microsoft Heami Desktop (ko-KR, Female)")]
        return voices
    except Exception:
        return [("Microsoft Heami Desktop", "Microsoft Heami Desktop (ko-KR, Female)")]


# 지원 엔진 및 성우 프리셋 정의
TTS_ENGINES = {
    "edge_tts": {
        "name": "Edge-TTS (Microsoft Neural AI - 추천 ⭐)",
        "type": "online",
        "voices": [
            ("ko-KR-SunHiNeural", "선희 (SunHi - 한국어 여성, 추천 ⭐)"),
            ("ko-KR-InJoonNeural", "인준 (InJoon - 한국어 남성)"),
            ("ko-KR-HyunsuMultilingualNeural", "현수 (Hyunsu - 한국어 남성 멀티)"),
            ("en-US-AvaNeural", "Ava (영어 여성)"),
            ("en-US-AndrewNeural", "Andrew (영어 남성)"),
        ],
    },
    "gtts": {
        "name": "Google TTS (gTTS - 구글 번역기 음성)",
        "type": "online",
        "voices": [
            ("ko", "한국어 (Korean)"),
            ("en", "영어 (English)"),
            ("ja", "일본어 (Japanese)"),
        ],
    },
    "sapi5": {
        "name": "Windows System TTS (오프라인 내장 음성)",
        "type": "offline",
        "voices": [],  # 동적으로 get_sapi5_voices()로 채워짐
    },
}


def get_available_voices(engine_key: str) -> List[Tuple[str, str]]:
    """선택된 엔진의 성우 목록 반환 [(VoiceID, DisplayName), ...]"""
    if engine_key == "sapi5":
        return get_sapi5_voices()
    return TTS_ENGINES.get(engine_key, {}).get("voices", [])


def synthesize_tts_audio(text: str, engine_key: str, voice_id: str, output_wav_path: str) -> bool:
    """
    지정된 엔진과 성우로 텍스트를 WAV 파일로 생성
    """
    if not text or not text.strip():
        return False

    text = text.strip()

    if engine_key == "edge_tts":
        return _synth_edge_tts(text, voice_id, output_wav_path)
    elif engine_key == "gtts":
        return _synth_gtts(text, voice_id, output_wav_path)
    elif engine_key == "sapi5":
        return _synth_sapi5(text, voice_id, output_wav_path)
    else:
        # Default fallback to edge-tts or sapi5
        if not _synth_edge_tts(text, "ko-KR-SunHiNeural", output_wav_path):
            return _synth_sapi5(text, voice_id, output_wav_path)
        return True


def _synth_edge_tts(text: str, voice: str, output_wav: str) -> bool:
    """Edge-TTS를 사용하여 음성 생성 후 WAV로 변환"""
    import edge_tts

    async def _run():
        with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as tf:
            temp_mp3 = tf.name

        try:
            communicate = edge_tts.Communicate(text, voice)
            await communicate.save(temp_mp3)

            # MP3를 WAV로 변환 (ffmpeg 사용)
            if is_ffmpeg_available():
                cmd = ["ffmpeg", "-y", "-i", temp_mp3, "-ac", "1", "-ar", "44100", output_wav]
                res = subprocess.run(cmd, capture_output=True, text=True)
                return res.returncode == 0 and os.path.exists(output_wav)
            else:
                # ffmpeg가 없는 경우 경고 후 실패
                return False
        finally:
            if os.path.exists(temp_mp3):
                try:
                    os.remove(temp_mp3)
                except OSError:
                    pass

    try:
        return asyncio.run(_run())
    except Exception as e:
        print(f"[Edge-TTS Error] {e}")
        return False


def _synth_gtts(text: str, lang: str, output_wav: str) -> bool:
    """gTTS를 사용하여 음성 생성 후 WAV로 변환"""
    from gtts import gTTS

    with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as tf:
        temp_mp3 = tf.name

    try:
        tts = gTTS(text=text, lang=lang)
        tts.save(temp_mp3)

        if is_ffmpeg_available():
            cmd = ["ffmpeg", "-y", "-i", temp_mp3, "-ac", "1", "-ar", "44100", output_wav]
            res = subprocess.run(cmd, capture_output=True, text=True)
            return res.returncode == 0 and os.path.exists(output_wav)
        return False
    except Exception as e:
        print(f"[gTTS Error] {e}")
        return False
    finally:
        if os.path.exists(temp_mp3):
            try:
                os.remove(temp_mp3)
            except OSError:
                pass


def _synth_sapi5(text: str, voice_name: str, output_wav: str) -> bool:
    """Windows SAPI5를 사용하여 음성 생성"""
    escaped_text = text.replace('"', '""').replace("`", "``").replace("$", "`$")
    escaped_path = os.path.abspath(output_wav).replace('"', '""').replace("`", "``").replace("$", "`$")
    escaped_voice = voice_name.replace('"', '""').replace("`", "``").replace("$", "`$")

    voice_select_cmd = f"$synth.SelectVoice('{escaped_voice}')" if voice_name else ""

    ps_code = f"""
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
try {{
    {voice_select_cmd}
}} catch {{}}
$synth.SetOutputToWaveFile("{escaped_path}")
$synth.Speak("{escaped_text}")
$synth.Dispose()
"""

    with tempfile.NamedTemporaryFile(mode="w", suffix=".ps1", delete=False, encoding="utf-8") as tf:
        tf.write(ps_code)
        ps_script_path = tf.name

    try:
        res = subprocess.run(
            [
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                ps_script_path,
            ],
            capture_output=True,
            text=True,
            timeout=15,
        )
        return res.returncode == 0 and os.path.exists(output_wav)
    except Exception as e:
        print(f"[SAPI5 Error] {e}")
        return False
    finally:
        if os.path.exists(ps_script_path):
            try:
                os.remove(ps_script_path)
            except OSError:
                pass
