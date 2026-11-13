//! beep-gen 명령줄 도구.
//!
//! 여기는 "로컬 어댑터" 다: 옵션을 읽고, 공용 절차(`beep_native::job`)를 부르고, 파일로 저장한다.
//! 소리를 만드는 계산은 하나도 하지 않는다.
//!
//!   beep-gen [옵션]      음원을 만든다
//!   beep-gen mcp         MCP 서버로 실행한다 (AI 도구 연동)

mod mcp;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use beep_core::{DEFAULT_INTRO_TEMPLATE, MusicReport, OutputFormat, TrackParams};
use beep_native::job::{self, IntroRequest, JobError, JobRequest, MusicSource};
use beep_native::tts;
use clap::{Args, Parser, Subcommand};

/// 맞춤형 비프음 오디오 파일 생성기
#[derive(Parser, Debug)]
#[command(name = "beep-gen", version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    generate: GenerateArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// MCP 서버로 실행한다. AI 도구가 표준 입출력으로 이 프로그램을 부를 수 있다.
    Mcp,
}

#[derive(Args, Debug)]
struct GenerateArgs {
    #[command(flatten)]
    params: TrackParams,

    /// 배경 음악: 파일 경로 또는 오디오 파일의 직접 주소(http/https).
    /// 비프 템포에 맞춰 변환해 함께 넣는다 (MP3, WAV, FLAC, OGG, M4A).
    #[arg(long = "music")]
    music: Option<String>,

    /// 시작 안내 음성(TTS) 문구. {n} 주기, {m} 분할 수, {l} 길이로 바뀐다. 빈 문자열이면 넣지 않는다.
    #[arg(short = 't', long = "tts", default_value = DEFAULT_INTRO_TEMPLATE)]
    tts: String,

    /// 안내 음성에 쓸 음성의 이름 또는 식별자 (일부만 적어도 된다). --list-voices 로 확인.
    #[arg(long = "tts-voice")]
    tts_voice: Option<String>,

    /// 설치된 음성 목록을 보여주고 끝낸다.
    #[arg(long = "list-voices")]
    list_voices: bool,

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
    let result = match cli.command {
        Some(Command::Mcp) => {
            mcp::serve_stdio().map_err(|e| io_failure(format!("MCP 서버가 중단되었습니다: {e}")))
        }
        None => generate(cli.generate),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure { message, code }) => {
            eprintln!("오류: {message}");
            ExitCode::from(code)
        }
    }
}

struct Failure {
    message: String,
    code: u8,
}

/// 입출력 실패(파일, 네트워크). 종료 코드 1.
fn io_failure(message: String) -> Failure {
    Failure { message, code: 1 }
}

fn generate(args: GenerateArgs) -> Result<(), Failure> {
    if args.list_voices {
        return list_voices();
    }

    let format = args
        .format
        .or_else(|| {
            args.output
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .and_then(OutputFormat::from_extension)
        })
        .unwrap_or_default();
    let request = JobRequest {
        params: args.params,
        format,
        music: args.music.as_deref().map(MusicSource::parse),
        intro: Some(IntroRequest {
            template: args.tts,
            voice: args.tts_voice,
        }),
    };

    let quiet = args.quiet;
    let mut last = None;
    let result = job::run(&request, |percent, stage| {
        if !quiet && last != Some((percent, stage.to_owned())) {
            last = Some((percent, stage.to_owned()));
            eprint!("\r{stage}... {percent:3}%   ");
            let _ = std::io::stderr().flush();
        }
    });
    if !quiet {
        eprintln!();
    }
    let track = result.map_err(|e| match e {
        // 잘못된 설정값·읽을 수 없는 음악 등 입력 문제는 종료 코드 2.
        JobError::Input(message) => Failure { message, code: 2 },
        JobError::Fetch(message) => io_failure(message),
    })?;

    // 음원은 만들었지만 알아야 할 일(예: 안내 음성을 넣지 못함)은 조용한 모드에서도 알린다.
    for notice in &track.notices {
        eprintln!("주의: {notice}");
    }
    if !quiet {
        if let Some(report) = &track.music_report {
            print_music_report(report);
        }
    }

    let output = args
        .output
        .unwrap_or_else(|| PathBuf::from(&track.file_name));
    std::fs::write(&output, &track.bytes)
        .map_err(|e| io_failure(format!("{} 에 저장하지 못했습니다: {e}", output.display())))?;
    println!("{}", output.display());
    Ok(())
}

fn list_voices() -> Result<(), Failure> {
    if !tts::is_supported() {
        return Err(io_failure(
            "이 운영체제에서는 음성 합성(TTS)을 아직 지원하지 않습니다.".into(),
        ));
    }
    let voices = tts::list_voices().map_err(|e| io_failure(e.to_string()))?;
    for voice in voices {
        println!("{}\t{}", voice.name, voice.language);
    }
    Ok(())
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
