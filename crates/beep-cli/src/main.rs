//! beep-gen 명령줄 도구.
//!
//! 여기는 "로컬 어댑터" 다: 옵션을 읽고, 코어를 부르고, 파일로 저장한다.
//! 소리를 만드는 계산은 하나도 하지 않는다.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use beep_core::{OutputFormat, Progress, TrackParams, build_track, default_file_name};
use clap::Parser;

/// 맞춤형 비프음 오디오 파일 생성기
#[derive(Parser, Debug)]
#[command(name = "beep-gen", version)]
struct Cli {
    #[command(flatten)]
    params: TrackParams,

    /// 저장할 파일 경로. 생략하면 설정값으로 이름을 만든다.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// 출력 형식. 생략하면 저장 경로의 확장자를 따르고, 그것도 없으면 mp3.
    #[arg(long = "format", value_enum)]
    format: Option<OutputFormat>,

    /// 진행 표시를 끈다.
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
    let output = cli
        .output
        .unwrap_or_else(|| PathBuf::from(default_file_name(&cli.params, format)));

    let quiet = cli.quiet;
    let mut last_percent = None;
    let on_progress = |p: Progress| {
        let percent = p.percent();
        if !quiet && last_percent != Some(percent) {
            last_percent = Some(percent);
            eprint!("\r생성 중... {percent:3}%");
            let _ = std::io::stderr().flush();
        }
    };

    let bytes = match build_track(&cli.params, format, on_progress) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("\n오류: {e}");
            return ExitCode::from(2);
        }
    };
    if !quiet {
        eprintln!();
    }

    if let Err(e) = std::fs::write(&output, &bytes) {
        eprintln!("오류: {} 에 저장하지 못했습니다: {e}", output.display());
        return ExitCode::FAILURE;
    }
    println!("{}", output.display());
    ExitCode::SUCCESS
}
