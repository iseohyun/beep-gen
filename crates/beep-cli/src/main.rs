//! beep-gen 명령줄 도구.
//!
//! 여기는 "로컬 어댑터" 다: 옵션을 읽고, 파일을 읽고, 코어를 부르고, 파일로 저장한다.
//! 소리를 만드는 계산은 하나도 하지 않는다.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use beep_core::{
    DEFAULT_INTRO_TEMPLATE, MusicReport, OutputFormat, Progress, TrackInputs, TrackParams,
    build_track, default_file_name, intro_text,
};
use beep_native::{fetch, tts};
use clap::Parser;

/// 맞춤형 비프음 오디오 파일 생성기
#[derive(Parser, Debug)]
#[command(name = "beep-gen", version)]
struct Cli {
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
    match run(cli) {
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

/// 입출력 실패(파일, 네트워크).
fn io_failure(message: String) -> Failure {
    Failure { message, code: 1 }
}

fn run(cli: Cli) -> Result<(), Failure> {
    if cli.list_voices {
        return list_voices();
    }

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

    let music = cli.music.as_deref().map(load_music).transpose()?;
    let intro = make_intro(&cli);
    let output = cli
        .output
        .clone()
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

    let inputs = TrackInputs {
        music: music.as_deref(),
        intro: intro.as_deref(),
    };
    let track = build_track(&cli.params, format, inputs, on_progress).map_err(|e| {
        if !quiet {
            eprintln!();
        }
        // 잘못된 설정값·읽을 수 없는 음악 등 입력 문제.
        Failure {
            message: e.to_string(),
            code: 2,
        }
    })?;
    if !quiet {
        eprintln!();
        if let Some(report) = &track.music {
            print_music_report(report);
        }
    }

    std::fs::write(&output, &track.bytes)
        .map_err(|e| io_failure(format!("{} 에 저장하지 못했습니다: {e}", output.display())))?;
    println!("{}", output.display());
    Ok(())
}

/// 배경 음악을 파일 또는 주소에서 읽는다.
fn load_music(source: &str) -> Result<Vec<u8>, Failure> {
    if fetch::is_url(source) {
        fetch::download(source).map_err(|e| io_failure(e.to_string()))
    } else {
        std::fs::read(source)
            .map_err(|e| io_failure(format!("{source} 을(를) 읽지 못했습니다: {e}")))
    }
}

/// 안내 음성을 만든다. 만들 수 없으면 이유를 알리고 안내 없이 계속한다.
fn make_intro(cli: &Cli) -> Option<Vec<u8>> {
    let text = intro_text(&cli.tts, &cli.params);
    if text.is_empty() {
        return None;
    }
    let result = resolve_voice(cli.tts_voice.as_deref())
        .and_then(|voice| tts::synthesize(&text, voice.as_deref()));
    match result {
        Ok(wav) => Some(wav),
        Err(e) => {
            eprintln!("주의: 안내 음성 없이 만듭니다. {e}");
            None
        }
    }
}

/// 사용자가 적은 이름(일부)이나 식별자로 음성을 찾는다.
fn resolve_voice(wanted: Option<&str>) -> beep_native::Result<Option<String>> {
    let Some(wanted) = wanted else {
        return Ok(None);
    };
    let needle = wanted.to_lowercase();
    tts::list_voices()?
        .into_iter()
        .find(|v| v.id == wanted || v.name.to_lowercase().contains(&needle))
        .map(|v| Some(v.id))
        .ok_or_else(|| beep_native::Error(format!("음성을 찾지 못했습니다: {wanted}")))
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
