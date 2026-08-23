"""
Main Entrypoint for Beep Audio Generator (CLI + GUI)
"""

import argparse
import os
import sys

# Windows 콘솔 출력 시 유니코드/이모지 인코딩 오류 방지
if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass

import generator
import tts_engine


def run_cli(args):
    """CLI 모드 실행"""
    n = args.interval
    m = args.subdivision
    weak_vol = args.weak_volume
    length = args.length
    tts_tmpl = args.tts
    tts_eng = args.tts_engine
    tts_vc = args.tts_voice

    # 기본 출력 파일명 결정
    n_str = f"{n:g}"
    if args.output:
        out_path = args.output
    else:
        fmt = args.format.lower()
        out_path = f"beep_n{n_str}s_m{m}_l{length}s.{fmt}"

    print("=" * 60)
    print("🎵 맞춤형 비프음 생성기 (Beep Audio Generator) - CLI Mode")
    print("=" * 60)
    print(f" • 기본 비트 주기 (n)     : {n_str}초")
    print(f" • 비트 쪼개기 (m)       : {m}분할 ({n/m:.2f}초 간격)")
    print(f" • 작은 비트 볼륨        : {weak_vol}%")
    print(f" • 음원 재생 길이 (l)    : {length}초 ({length // 60}분 {length % 60:02d}초)")
    print(f" • 시작 메시지 (TTS)     : {tts_tmpl if tts_tmpl else '(생략)'}")
    print(f" • TTS 엔진 / 성우       : {tts_eng} / {tts_vc}")
    print(f" • 저장 경로             : {os.path.abspath(out_path)}")
    print("=" * 60)

    def on_progress(pct: int, msg: str):
        bar_len = 30
        filled = int(bar_len * pct / 100)
        bar = "█" * filled + "░" * (bar_len - filled)
        sys.stdout.write(f"\r[{bar}] {pct:3d}% - {msg:<35}")
        sys.stdout.flush()

    try:
        final_path = generator.build_audio_file(
            n=n,
            m=m,
            weak_volume_pct=weak_vol,
            length_sec=length,
            tts_template=tts_tmpl,
            tts_engine_name=tts_eng,
            tts_voice=tts_vc,
            output_path=out_path,
            progress_callback=on_progress,
        )
        print("\n\n🎉 음원 생성이 성공적으로 완료되었습니다!")
        print(f"📁 파일 위치: {os.path.abspath(final_path)}")
    except Exception as e:
        print(f"\n\n❌ 생성 중 오류가 발생했습니다: {e}")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description="원하는 비트와 TTS 안내를 포함하는 맞춤형 비프음 오디오 파일 생성기"
    )
    parser.add_argument(
        "-n", "--interval", type=float, default=1.0, help="기본 비트 주기(초, 소수점 1자리 지원) (default: 1.0)"
    )
    parser.add_argument(
        "-m", "--subdivision", type=int, default=2, help="기본 비트 쪼개기 수 (default: 2)"
    )
    parser.add_argument(
        "-v",
        "--weak-volume",
        type=float,
        default=35.0,
        help="쪼개진 작은 비트 볼륨 비율(%%) (default: 35.0)",
    )
    parser.add_argument(
        "-l", "--length", type=int, default=300, help="음원 재생 길이(초) (default: 300)"
    )
    parser.add_argument(
        "-t",
        "--tts",
        type=str,
        default="{n}초, {n}초 비프",
        help="음원 초기에 출력할 TTS 안내 문구 (default: '{n}초, {n}초 비프')",
    )
    parser.add_argument(
        "--tts-engine",
        type=str,
        choices=["edge_tts", "gtts", "sapi5"],
        default="edge_tts",
        help="TTS 음성 합성 엔진 (default: edge_tts)",
    )
    parser.add_argument(
        "--tts-voice",
        type=str,
        default="ko-KR-SunHiNeural",
        help="TTS 성우/목소리 ID (default: ko-KR-SunHiNeural)",
    )
    parser.add_argument(
        "-o", "--output", type=str, default=None, help="출력 파일 경로 (예: output.mp3 또는 output.wav)"
    )
    parser.add_argument(
        "--format",
        choices=["mp3", "wav"],
        default="mp3",
        help="출력 오디오 포맷 (default: mp3)",
    )
    parser.add_argument(
        "--cli", action="store_true", help="GUI 대신 CLI 모드로 실행"
    )
    parser.add_argument(
        "--gui", action="store_true", help="GUI 창 실행"
    )

    args, unknown = parser.parse_known_args()

    # 인자 유무에 따라 기본 실행 모드 결정
    has_explicit_cli_args = any(
        arg in sys.argv[1:]
        for arg in [
            "-n",
            "--interval",
            "-m",
            "--subdivision",
            "-v",
            "--weak-volume",
            "-l",
            "--length",
            "-t",
            "--tts",
            "--tts-engine",
            "--tts-voice",
            "-o",
            "--output",
            "--cli",
        ]
    )

    if args.gui or (not has_explicit_cli_args and not args.cli):
        try:
            from gui import launch_gui

            launch_gui()
        except ImportError as e:
            print(f"[Warning] GUI 라이브러리(PyQt6) 로드 실패 ({e}). CLI 모드로 전환합니다.")
            run_cli(args)
    else:
        run_cli(args)


if __name__ == "__main__":
    main()
