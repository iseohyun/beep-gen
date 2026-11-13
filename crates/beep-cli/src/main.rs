//! beep-gen 명령줄 도구.
//!
//! 여기는 "로컬 어댑터" 다: 옵션을 읽고, 파일을 읽고, 코어를 부르고, 파일로 저장한다.
//! 소리를 만드는 계산은 하나도 하지 않는다.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use beep_core::{MusicReport, OutputFormat, Progress, TrackParams, build_track, default_file_name};
use clap::Parser;

/// 맞춤형 비프음 오디오 파일 생성기
#[derive(Parser, Debug)]
#[command(name = "beep-gen", version)]
struct Cli {
    #[command(flatten)]
    params: TrackParams,

    /// 배경 음악 파일. 비프 템포에 맞춰 변환해 함께 넣는다 (MP3, WAV, FLAC, OGG, M4A).
    #[arg(long = "music")]
    music: Option<PathBuf>,

    /// 저장할 파일 경로. 생략하면 설정값으로 이름을 만든다.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// 출력 형식. 생략하면 저장 경로의 확장자를 따르고, 그것도 없으면 mp3.
    #[arg(long = "format", value_enum)]
    format: Option<OutputFormat>,

    /// 진행 표시와 보고를 끈다.
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let format = cli
        .format
        .or_else(|| {
            cli.output
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .and_then(OutputFormat::from_extension)
        })
        .unwrap_or_default();

    let music = match &cli.music {
        None => None,
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                eprintln!("오류: {} 을(를) 읽지 못했습니다: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
    };
    let output = cli
        .output
        .unwrap_or_else(|| PathBuf::from(default_file_name(&cli.params, format, music.is_some())));

    let quiet = cli.quiet;
    let mut last = None;
    let on_progress = |p: Progress| {
        if !quiet && last != Some(p) {
            last = Some(p);
            eprint!("\r{}... {:3}%   ", p.stage.label(), p.percent);
            let _ = std::io::stderr().flush();
        }
    };

    let track = match build_track(&cli.params, format, music.as_deref(), on_progress) {
        Ok(track) => track,
        Err(e) => {
            eprintln!("\n오류: {e}");
            return ExitCode::from(2);
        }
    };
    if !quiet {
        eprintln!();
        if let Some(report) = &track.music {
            print_music_report(report);
        }
    }

    if let Err(e) = std::fs::write(&output, &track.bytes) {
        eprintln!("오류: {} 에 저장하지 못했습니다: {e}", output.display());
        return ExitCode::FAILURE;
    }
    println!("{}", output.display());
    ExitCode::SUCCESS
}

fn print_music_report(report: &MusicReport) {
    eprintln!(
        "음악: {:.1} BPM 검출 (신뢰도 {:.0}%) → 한 박을 {:.3}초에 맞춤, 템포 {:+.1}%",
        report.detected_bpm,
        report.beat_confidence * 100.0,
        report.target_beat_sec,
        report.tempo_change_pct,
    );
    eprintln!(
        "길이: {:.1}초 → {:.1}초, 강한 비프 기준: {}",
        report.source_seconds,
        report.output_seconds,
        if report.anchored_on_downbeat {
            "마디 시작"
        } else {
            "첫 박"
        },
    );
    for warning in &report.warnings {
        eprintln!("주의: {warning}");
    }
}
