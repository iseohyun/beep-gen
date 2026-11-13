//! beep-gen 코어: 비프음 합성, 배경 음악 템포 맞추기, 인코딩.
//!
//! 이 크레이트는 웹(WASM)과 로컬(네이티브)이 그대로 함께 쓴다. 그래서 파일·네트워크·
//! 시계·OS 호출을 직접 쓰지 않는다. 입력은 값으로 받고, 결과는 바이트로 돌려준다.
//! (규칙: `.agents/rules/project_architecture.md` 2.2절)

pub mod decode;
pub mod encode;
pub mod error;
pub mod music;
pub mod params;
pub mod resample;
pub mod synth;

pub use encode::OutputFormat;
pub use error::{Error, Result};
pub use music::MusicReport;
pub use params::{ParamKind, ParamSpec, ParamWhen, TrackParams, param_specs};

use encode::Encoder;
use music::MusicStream;
use synth::BeepSequence;

/// 한 번에 합성해 인코더로 넘기는 구간 길이(샘플). 약 0.37초(44.1kHz 기준).
const CHUNK_FRAMES: usize = 16_384;

/// 진행 단계.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// 음악 파일을 읽는 중.
    Decode,
    /// 음악의 박자를 분석하는 중.
    Analyze,
    /// 음원을 만드는 중(템포 변환·비프 합성·인코딩).
    Render,
}

impl Stage {
    /// 단계 식별자. 화면과 주고받을 때 쓴다.
    pub fn key(self) -> &'static str {
        match self {
            Stage::Decode => "decode",
            Stage::Analyze => "analyze",
            Stage::Render => "render",
        }
    }

    /// 사람이 읽는 설명.
    pub fn label(self) -> &'static str {
        match self {
            Stage::Decode => "음악 파일을 읽는 중",
            Stage::Analyze => "박자를 분석하는 중",
            Stage::Render => "음원을 만드는 중",
        }
    }
}

/// 진행 상황.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    /// 전체 작업 기준 0~100.
    pub percent: u8,
}

/// 음악이 있을 때 `Render` 단계가 시작되는 백분율. 그 앞은 읽기와 분석이 차지한다.
const RENDER_START_WITH_MUSIC: u64 = 20;
const ANALYZE_START: u8 = 8;

/// 만들어진 음원.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackOutput {
    /// 완성된 파일 바이트.
    pub bytes: Vec<u8>,
    /// 배경 음악을 넣었을 때, 음악을 어떻게 맞췄는지에 대한 보고.
    pub music: Option<MusicReport>,
}

/// 음원 파일을 만든다.
///
/// `music` 에 음악 파일 바이트를 주면 그 음악을 비프 템포에 맞춰 함께 넣는다.
/// 이때 음원 길이는 (템포를 맞춘) 음악 길이를 따른다.
/// `on_progress` 는 진행 상황이 바뀔 때 호출된다.
pub fn build_track(
    params: &TrackParams,
    format: OutputFormat,
    music: Option<&[u8]>,
    mut on_progress: impl FnMut(Progress),
) -> Result<TrackOutput> {
    params.validate()?;
    match music {
        None => build_beeps_only(params, format, &mut on_progress),
        Some(bytes) => build_with_music(params, format, bytes, &mut on_progress),
    }
}

fn build_beeps_only(
    params: &TrackParams,
    format: OutputFormat,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<TrackOutput> {
    let mut sequence = BeepSequence::new(params);
    let mut encoder = Encoder::new(format, params.sample_rate, 1);
    let total = sequence.total_frames();
    let gain = (params.beep_volume_pct / 100.0) as f32;
    let mut chunk = vec![0.0f32; CHUNK_FRAMES];

    loop {
        let count = sequence.render(&mut chunk);
        if count == 0 {
            break;
        }
        // 기본값(100%)에서는 곱해도 값이 그대로다.
        for sample in &mut chunk[..count] {
            *sample *= gain;
        }
        encoder.push(&chunk[..count])?;
        on_progress(Progress {
            stage: Stage::Render,
            percent: percent_of(sequence.position(), total, 0),
        });
    }

    Ok(TrackOutput {
        bytes: encoder.finish(),
        music: None,
    })
}

fn build_with_music(
    params: &TrackParams,
    format: OutputFormat,
    music_bytes: &[u8],
    on_progress: &mut dyn FnMut(Progress),
) -> Result<TrackOutput> {
    on_progress(Progress {
        stage: Stage::Decode,
        percent: 0,
    });
    let audio = decode::decode_audio(music_bytes)?;

    on_progress(Progress {
        stage: Stage::Analyze,
        percent: ANALYZE_START,
    });
    let mut stream = MusicStream::new(audio, params)?;

    let channels = stream.channels();
    let total = stream.total_frames();
    let mut beeps = BeepSequence::with_layout(params, total, stream.anchor_frames());
    let mut encoder = Encoder::new(format, params.sample_rate, channels as u16);

    // 음악과 비프의 최대 진폭 합이 1을 넘으면 소리가 찌그러지므로, 넘는 만큼 둘 다 줄인다.
    let music_volume = params.music_volume_pct / 100.0;
    let beep_volume = params.beep_volume_pct / 100.0;
    let worst_peak = music_volume * f64::from(stream.source_peak()) + beep_volume * synth::HEADROOM;
    let normalize = if worst_peak > 1.0 {
        1.0 / worst_peak
    } else {
        1.0
    };
    let music_gain = (music_volume * normalize) as f32;
    let beep_gain = (beep_volume * normalize) as f32;

    let mut block = Vec::new();
    let mut beep_block = Vec::new();
    while stream.next_block(&mut block)? {
        let frames = block.len() / channels;
        beep_block.resize(frames, 0.0);
        // 음악이 예상 길이를 넘는 꼬리에서는 비프가 끝나 있으므로 무음으로 채운다.
        let beep_count = beeps.render(&mut beep_block);
        beep_block[beep_count..].fill(0.0);

        for (frame, beep) in block.chunks_exact_mut(channels).zip(&beep_block) {
            let beep = beep * beep_gain;
            for sample in frame {
                *sample = (*sample * music_gain + beep).clamp(-1.0, 1.0);
            }
        }
        encoder.push(&block)?;
        on_progress(Progress {
            stage: Stage::Render,
            percent: percent_of(stream.produced_frames(), total, RENDER_START_WITH_MUSIC),
        });
    }

    Ok(TrackOutput {
        bytes: encoder.finish(),
        music: Some(stream.report().clone()),
    })
}

/// `start`%에서 시작해 100%까지, `done / total` 만큼 진행한 백분율.
fn percent_of(done: u64, total: u64, start: u64) -> u8 {
    let span = 100 - start;
    let part = (done.min(total) * span).checked_div(total).unwrap_or(span);
    (start + part) as u8
}

/// 기본 출력 파일 이름. 예: `beep_n1.4s_m2_l300s.mp3`, 음악이 있으면 `beep_n1.4s_m2_music.mp3`
pub fn default_file_name(params: &TrackParams, format: OutputFormat, with_music: bool) -> String {
    let tail = if with_music {
        "music".to_owned()
    } else {
        format!("l{}s", params.length_sec)
    };
    format!(
        "beep_n{}s_m{}_{}.{}",
        params.interval_label(),
        params.subdivision,
        tail,
        format.extension()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_name_matches_legacy_pattern() {
        let params = TrackParams::default();
        assert_eq!(
            default_file_name(&params, OutputFormat::Mp3, false),
            "beep_n1s_m2_l300s.mp3"
        );
        let params = TrackParams {
            interval_sec: 1.4,
            length_sec: 60,
            ..params
        };
        assert_eq!(
            default_file_name(&params, OutputFormat::Wav, false),
            "beep_n1.4s_m2_l60s.wav"
        );
        assert_eq!(
            default_file_name(&params, OutputFormat::Mp3, true),
            "beep_n1.4s_m2_music.mp3"
        );
    }

    #[test]
    fn build_wav_has_expected_size_and_reports_progress() {
        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let mut last = None;
        let mut calls = 0;
        let output = build_track(&params, OutputFormat::Wav, None, |p| {
            calls += 1;
            last = Some(p);
        })
        .unwrap();
        assert_eq!(output.bytes.len(), 44 + 2 * 44_100 * 2);
        assert!(output.music.is_none());
        assert!(calls >= 2);
        assert_eq!(
            last.unwrap(),
            Progress {
                stage: Stage::Render,
                percent: 100
            }
        );
    }

    #[test]
    fn build_mp3_produces_audio() {
        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let mp3 = build_track(&params, OutputFormat::Mp3, None, |_| {})
            .unwrap()
            .bytes;
        assert!((40_000..60_000).contains(&mp3.len()), "len = {}", mp3.len());
    }

    #[test]
    fn beep_volume_scales_output() {
        let full = TrackParams {
            length_sec: 1,
            ..TrackParams::default()
        };
        let half = TrackParams {
            beep_volume_pct: 50.0,
            ..full.clone()
        };
        let peak = |params: &TrackParams| {
            let wav = build_track(params, OutputFormat::Wav, None, |_| {})
                .unwrap()
                .bytes;
            wav[44..]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
                .max()
                .unwrap()
        };
        let (full_peak, half_peak) = (peak(&full), peak(&half));
        assert!((i32::from(full_peak) - 2 * i32::from(half_peak)).abs() <= 2);
    }

    #[test]
    fn invalid_params_are_rejected_before_any_work() {
        let params = TrackParams {
            subdivision: 0,
            ..TrackParams::default()
        };
        let mut called = false;
        let result = build_track(&params, OutputFormat::Wav, None, |_| called = true);
        assert!(matches!(
            result,
            Err(Error::InvalidParam {
                name: "subdivision",
                ..
            })
        ));
        assert!(!called);
    }

    #[test]
    fn garbage_music_is_a_decode_error() {
        let params = TrackParams::default();
        let result = build_track(&params, OutputFormat::Wav, Some(&[1, 2, 3, 4, 5]), |_| {});
        assert!(matches!(result, Err(Error::Decode(_))), "{result:?}");
    }
}
