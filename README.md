# 🎵 맞춤형 비프음 오디오 생성기 (Beep Gen)

원하는 비트 주기($n$초)와 쪼개기 박자($m$개), 약한 비프 볼륨, 재생 길이($l$초), 그리고 음원 시작 시 세팅을 알려주는 오프라인 TTS 인트로 음성을 포함하는 맞춤형 오디오 파일(.mp3 / .wav) 생성 도구입니다.

---

## 🌟 주요 기능

1. **기본 비트 ($n$)**: $n$초마다 명확하고 강한 기준 비프음(880Hz, 100% 볼륨)을 출력합니다.
2. **쪼개기 ($m$)**: 기본 비트를 $m$개로 등분하여 작은 비프음(587Hz)을 출력합니다.
3. **약한 비프 볼륨 조절**: 쪼개진 작은 비트의 볼륨 비율(Default: `35%`, 0~100%)을 자유롭게 조정할 수 있습니다.
4. **시작 메시지 (TTS)**: Windows 내장 SAPI5 오프라인 음성 합성기를 사용하여 시작 시 설정된 주기를 음성으로 안내합니다. (예: `"1초, 1초 비프"`, 문구 커스텀 가능)
5. **음원 길이 ($l$)**: TTS 인트로를 제외한 순수 비프음 지속 시간(Default: `300`초 = 5분)을 지정할 수 있습니다.
6. **MP3 / WAV 포맷 지원**: 초소형 고음질 MP3(Default) 및 무손실 WAV 출력을 지원합니다.
7. **GUI & CLI 지원**: PyQt6 기반의 직관적인 데스크톱 GUI 및 자동화가 가능한 커맨드라인 인터페이스를 모두 제공합니다.

---

## 🚀 실행 방법

### 1. 단독 실행 파일 (.exe) 바로 실행
`dist\BeepGen.exe` 파일을 더블 클릭하여 파이썬 설치 없이도 바로 실행할 수 있습니다.

### 2. EXE 빌드 스크립트 실행
언제든 새로운 실행 파일을 빌드하려면 아래 명령(또는 `build.bat` 더블 클릭)을 실행하세요:
```bash
build.bat
```

### 3. Python 소스 코드로 직접 실행
```bash
# GUI 모드 실행
python main.py

# CLI 모드 실행 (예: 1초 주기, 2분할, 볼륨 35%, 300초 길이, MP3)
python main.py --cli

# 사용자 지정 파라미터 (2초 주기, 4분할, 볼륨 25%, 60초 길이)
python main.py -n 2 -m 4 -v 25 -l 60 -o my_beat.mp3
```

---

## ⚙️ CLI 옵션 명세

| 옵션 | 단축키 | 기본값 | 설명 |
| :--- | :--- | :--- | :--- |
| `--interval` | `-n` | `1` | 기본 비트 주기 (초 단위 정수) |
| `--subdivision` | `-m` | `2` | 비트 쪼개기 수 (정수) |
| `--weak-volume` | `-v` | `35.0` | 쪼개진 작은 비트 볼륨 비율 (%) |
| `--length` | `-l` | `300` | 음원 재생 길이 (초 단위 정수, TTS 제외) |
| `--tts` | `-t` | `"{n}초, {n}초 비프"` | 시작 안내 TTS 템플릿 (빈 문자열 입력 시 생략) |
| `--output` | `-o` | `beep_n{n}s_m{m}_l{l}s.mp3` | 저장할 파일명/경로 |
| `--format` | | `mp3` | 출력 포맷 (`mp3` 또는 `wav`) |
| `--gui` | | | 강제 GUI 실행 |
| `--cli` | | | 강제 CLI 모드 실행 |

---

## 🦀 Rust 버전 (개발 중)

웹·PC·CLI가 하나의 코어를 함께 쓰도록 Rust로 옮기는 중입니다. 비프 합성, 배경 음악 템포 맞추기, 시작 안내 음성(TTS), WAV/MP3 저장이 구현되어 있고 웹 앱·데스크톱 앱·CLI로 쓸 수 있습니다. FFmpeg 없이 동작합니다.

```bash
# 빌드
cargo build --release -p beep-cli

# 실행 (예: 1.4초 주기, 2분할, 60초)
target/release/beep-gen -n 1.4 -m 2 -l 60 -o my_beat.mp3

# 배경 음악 넣기: 음악을 비프 템포에 맞춰 변환해 함께 넣는다 (길이는 음악을 따름)
target/release/beep-gen -n 1.4 -m 2 --music song.mp3 --music-volume 70 -o with_music.mp3

# 음악을 주소로 가져오기 (오디오 파일의 직접 주소만)
target/release/beep-gen -n 1.4 -m 2 --music https://example.com/song.mp3

# 시작 안내 음성: 기본 문구는 "{n}초, {n}초 비프". 바꾸거나(-t), 빼려면 -t ""
target/release/beep-gen -n 1.4 -m 2 -t "{n}초 템포" --tts-voice Heami
target/release/beep-gen --list-voices
```

배경 음악은 MP3, WAV, FLAC, OGG, M4A를 받습니다. 음악의 한 박을 비프 간격의 0.5·1·2배 중 가장 가까운 쪽에 맞추며, 박자가 일정한 곡에서 잘 동작합니다. `--beep-volume 0` 으로 템포만 맞춘 음악을 얻을 수 있습니다.

| 폴더 | 내용 |
| :--- | :--- |
| `crates/beep-core` | 합성·인코딩 코어 (웹 WASM · 네이티브 공용) |
| `crates/beep-cli` | 명령줄 도구 `beep-gen` |
| `crates/beep-wasm` | 코어의 웹(WASM) 연결부 |
| `apps/web` | 웹 앱 (TypeScript + Vite, PWA) |
| `apps/desktop` | 데스크톱 앱 (Tauri, 화면은 `apps/web` 을 그대로 사용) |
| `crates/beep-native` | 로컬 어댑터: Windows 내장 TTS, URL 다운로드 |
| `xtask` | 빌드 작업 (WASM 바인딩 생성) |
| `tools/parity_check.py` | Python 구현과 결과를 비교하는 검사 |
| `tools/check_align.py` | 배경 음악과 비프의 정렬을 측정하는 도구 |

### 웹 앱

브라우저 안에서 같은 코어(WASM)로 음원을 만듭니다. 서버로 아무것도 보내지 않으며, 한 번 연 뒤에는 오프라인에서도 동작합니다.

```bash
cd apps/web
npm install
npm run build      # 결과물: 저장소 루트의 dist/
npm run preview    # http://localhost:4173 에서 확인
```

기본 옵션은 Python 버전과 같고(`-n`, `-m`, `-v`, `-l`, `-t`, `-o`, `--format`), `--music`, `--beep-volume`, `--music-volume`, `--tts-voice`, `--list-voices` 가 추가되었습니다.

- **안내 음성(TTS)** 은 Windows 내장 음성으로 만듭니다(인터넷 불필요). 다른 운영체제에서는 안내 음성 없이 만들어집니다.
- **음악 주소** 는 오디오 파일을 바로 내려받을 수 있는 주소만 받습니다. YouTube 등 스트리밍 서비스 주소는 저작권 문제로 지원하지 않습니다. 필요하다면 이 프로젝트를 포크(fork)해 직접 기능을 추가해 사용하시기 바랍니다. `-n` 은 소수(예: 1.4)를 받습니다.

### 데스크톱 앱 (Windows)

웹 앱과 같은 화면에 안내 음성(TTS)과 음악 주소 입력이 더해지고, 음원을 네이티브 속도로 만듭니다.

```bash
cd apps/web && npm install && cd ../desktop && npm install
npm run build:exe   # 결과물: target/release/beep-desktop.exe
npm run build       # 설치 파일까지 만들기
```

실행에는 WebView2 런타임이 필요합니다(Windows 11에는 기본 포함).
