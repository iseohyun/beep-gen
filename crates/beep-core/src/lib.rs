//! beep-gen 코어: 비프음 합성과 인코딩.
//!
//! 이 크레이트는 웹(WASM)과 로컬(네이티브)이 그대로 함께 쓴다. 그래서 파일·네트워크·
//! 시계·OS 호출을 직접 쓰지 않는다. 입력은 값으로 받고, 결과는 바이트로 돌려준다.
//! (규칙: `.agents/rules/project_architecture.md` 2.2절)

pub mod encode;
pub mod error;
pub mod params;
pub mod synth;

pub use encode::OutputFormat;
pub use error::{Error, Result};
pub use params::{ParamKind, ParamSpec, TrackParams, param_specs};

use encode::Encoder;
use synth::BeepSequence;

/// 한 번에 합성해 인코더로 넘기는 구간 길이(샘플). 약 0.37초(44.1kHz 기준).
const CHUNK_FRAMES: usize = 16_384;

/// 진행 상황. `done` / `total` 은 샘플 수다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

impl Progress {
    /// 0~100 의 정수 백분율.
    pub fn percent(self) -> u8 {
        (self.done * 100).checked_div(self.total).unwrap_or(100) as u8
    }
}

/// 비프 음원 파일을 만들어 바이트로 돌려준다.
///
/// `on_progress` 는 구간 하나를 처리할 때마다 호출된다.
pub fn build_track(
    params: &TrackParams,
    format: OutputFormat,
    mut on_progress: impl FnMut(Progress),
) -> Result<Vec<u8>> {
    params.validate()?;

    let mut sequence = BeepSequence::new(params);
    let mut encoder = Encoder::new(format, params.sample_rate, 1);
    let total = sequence.total_frames();
    let mut chunk = vec![0.0f32; CHUNK_FRAMES];

    loop {
        let count = sequence.render(&mut chunk);
        if count == 0 {
            break;
        }
        encoder.push(&chunk[..count])?;
        on_progress(Progress {
            done: sequence.position(),
            total,
        });
    }

    Ok(encoder.finish())
}

/// 기본 출력 파일 이름. 예: `beep_n1.4s_m2_l300s.mp3`
pub fn default_file_name(params: &TrackParams, format: OutputFormat) -> String {
    format!(
        "beep_n{}s_m{}_l{}s.{}",
        params.interval_label(),
        params.subdivision,
        params.length_sec,
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
            default_file_name(&params, OutputFormat::Mp3),
            "beep_n1s_m2_l300s.mp3"
        );
        let params = TrackParams {
            interval_sec: 1.4,
            length_sec: 60,
            ..params
        };
        assert_eq!(
            default_file_name(&params, OutputFormat::Wav),
            "beep_n1.4s_m2_l60s.wav"
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
        let wav = build_track(&params, OutputFormat::Wav, |p| {
            calls += 1;
            last = Some(p);
        })
        .unwrap();
        assert_eq!(wav.len(), 44 + 2 * 44_100 * 2);
        assert!(calls >= 2);
        assert_eq!(last.unwrap().percent(), 100);
    }

    #[test]
    fn build_mp3_produces_audio() {
        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let mp3 = build_track(&params, OutputFormat::Mp3, |_| {}).unwrap();
        assert!((40_000..60_000).contains(&mp3.len()), "len = {}", mp3.len());
    }

    #[test]
    fn invalid_params_are_rejected_before_any_work() {
        let params = TrackParams {
            subdivision: 0,
            ..TrackParams::default()
        };
        let mut called = false;
        let result = build_track(&params, OutputFormat::Wav, |_| called = true);
        assert!(matches!(
            result,
            Err(Error::InvalidParam {
                name: "subdivision",
                ..
            })
        ));
        assert!(!called);
    }
}
