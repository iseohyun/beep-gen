# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-08-23

### Added
- **맞춤형 비프음 오디오 생성 코어 (`generator.py`)**
  - 안티 클리핑 페이드 엔벨로프(Attack/Release 5ms)를 적용한 클릭 노이즈 없는 부드러운 사인파 비프음 합성
  - 기본 비트 주기($n$, 소수점 1자리 지원) 및 균등 쪼개기($m$), 약한 비프 볼륨 비율 조절 기능
  - FFmpeg 연동 초소형 MP3 (Default) 및 무손실 WAV 내보내기 지원
- **다중 TTS 엔진 및 성우 선택 시스템 (`tts_engine.py`)**
  - Edge-TTS (Microsoft Neural AI - 선희, 인준, 현수, Ava, Andrew 등)
  - Google TTS (gTTS - 한국어, 영어, 일본어)
  - Windows System SAPI5 (오프라인 내장 음성 - 혜미, 지라 등 시스템 음성 자동 탐색)
  - 음원 초기에 설정된 주기를 음성으로 안내하는 인트로 TTS 생성
- **PyQt6 데스크톱 그래픽 인터페이스 (`gui.py`)**
  - $n, m, v, l,$ TTS 문구, 엔진/성우, 파일 포맷/경로를 직관적으로 제어할 수 있는 모던 UI
  - 5초 비트 미리듣기 및 TTS 인트로 음성 즉시 듣기 지원
  - 백그라운드 비동기 생성(QThread) 및 실시간 프로그레스 바 제공
  - 생성 완료 후 저장 폴더 열기 / 파일 재생 버튼
- **통합 진입점 및 CLI 인터페이스 (`main.py`)**
  - 무인 자동화 및 스크립트 연동을 위한 커맨드라인 파라미터 모드 지원
- **단독 실행 파일 빌드 스크립트 (`build.bat`)**
  - PyInstaller 기반의 원클릭 `dist\BeepGen.exe` 배포 빌드 지원
