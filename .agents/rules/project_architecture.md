# Project Architecture & Policy (beep-gen)

## 1. 프로젝트 개요
- **목적**: 사용자가 지정한 주기(n초), 비트 분할(m개), 약한 비프 볼륨 비율, 전체 길이(l초), TTS 인트로 안내를 포함하는 맞춤형 비프음 오디오 생성기 (GUI + Stand-alone 스크립트).
- **기술 스택**: Python 3.12, PyQt6 (GUI), Windows SAPI5 / pyttsx3 (오프라인 TTS), FFmpeg / Python `wave` & `math` (WAV/MP3 생성).

## 2. 웹 UI 관련 정책 (사용자 확인 완료)
- **CSS 스타일링 아키텍처**: 추후 Web UI 개발 착수 시 재확인 및 결정 (현재 보류).
- **캐시 버스팅 정책**: 추후 Web UI 개발 착수 시 재확인 및 결정 (현재 보류).

## 3. 오디오 생성 및 GUI 정책
- **기본 포맷**: MP3 (Default, FFmpeg 인코딩 연동), WAV 선택 지원.
- **오프라인 동작 보장**: 외부 네트워크 연결 없이 로컬 Windows TTS 및 DSP 오디오 합성 사용.
- **파라미터 체계**:
  - `n` (기본 주기, 초 단위 정수, default: 1)
  - `m` (비트 분할 수, 정수, default: 2)
  - Weak Beep Volume (`m` 분할음 볼륨 비율, default: 35%, 0~100% 범위)
  - TTS Intro Message (default: "{n}초, {n}초 비프")
  - Length `l` (비프음 지속 시간, TTS 제외, default: 300초)
