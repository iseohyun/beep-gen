//! 시작 안내 음성(인트로).
//!
//! 음성을 만드는 일(TTS)은 환경마다 방법이 달라 코어 밖의 어댑터가 한다.
//! 코어는 만들어진 음성 파일을 받아 음원의 샘플 레이트·채널에 맞추고,
//! 뒤에 짧은 무음을 붙여 맨 앞에 넣을 수 있는 형태로 바꾼다.

use crate::decode::{Audio, decode_audio};
use crate::error::Result;
use crate::params::TrackParams;
use crate::resample::StreamResampler;

/// 기본 안내 문구. `{n}` 주기(초), `{m}` 분할 수, `{l}` 길이(초)가 값으로 바뀐다.
pub const DEFAULT_INTRO_TEMPLATE: &str = "{n}초, {n}초 비프";

/// 안내 음성과 비프 사이의 무음 길이(초).
const PAUSE_AFTER_INTRO_SEC: f64 = 0.5;

/// 안내 문구의 자리표시자를 설정값으로 바꾼다. 앞뒤 공백은 지운다.
///
/// 예: `"{n}초, {n}초 비프"` + 주기 1.4 → `"1.4초, 1.4초 비프"`
pub fn intro_text(template: &str, params: &TrackParams) -> String {
    template
        .replace("{n}", &params.interval_label())
        .replace("{m}", &params.subdivision.to_string())
        .replace("{l}", &params.length_sec.to_string())
        .trim()
        .to_owned()
}

/// 음원의 샘플 레이트에 맞춰진 안내 구간(음성 + 무음).
pub struct Intro {
    audio: Audio,
}

/// 안내 음성 파일을 읽어 `sample_rate` 에 맞춘다.
pub fn decode_intro(bytes: &[u8], sample_rate: u32) -> Result<Intro> {
    let source = decode_audio(bytes)?;
    let channels = source.channels;

    let mut samples = Vec::new();
    if source.sample_rate == sample_rate {
        samples = source.samples;
    } else {
        // 음성 합성기는 보통 16~24kHz 로 내보내므로 음원의 샘플 레이트로 올린다.
        let ratio = f64::from(sample_rate) / f64::from(source.sample_rate);
        let mut resampler = StreamResampler::new(channels, ratio);
        resampler.push(&source.samples, &mut samples);
        resampler.finish(&mut samples);
    }

    let pause_frames = (PAUSE_AFTER_INTRO_SEC * f64::from(sample_rate)) as usize;
    samples.resize(samples.len() + pause_frames * channels, 0.0);
    Ok(Intro {
        audio: Audio {
            sample_rate,
            channels,
            samples,
        },
    })
}

impl Intro {
    /// 요청한 채널 수(1 또는 2)의 샘플로 바꾼다. 모노는 양쪽에 복사하고, 스테레오는 평균한다.
    pub fn into_interleaved(self, channels: usize) -> Vec<f32> {
        match (self.audio.channels, channels) {
            (from, to) if from == to => self.audio.samples,
            (1, 2) => self.audio.samples.iter().flat_map(|s| [*s, *s]).collect(),
            _ => self.audio.to_mono(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_placeholders_are_filled() {
        let params = TrackParams {
            interval_sec: 1.4,
            subdivision: 3,
            length_sec: 60,
            ..TrackParams::default()
        };
        assert_eq!(
            intro_text(DEFAULT_INTRO_TEMPLATE, &params),
            "1.4초, 1.4초 비프"
        );
        assert_eq!(intro_text(" {m}분할 {l}초 ", &params), "3분할 60초");
        assert_eq!(intro_text("", &params), "");
    }

    #[test]
    fn mono_intro_is_copied_to_both_channels() {
        let intro = Intro {
            audio: Audio {
                sample_rate: 44_100,
                channels: 1,
                samples: vec![0.1, 0.2],
            },
        };
        assert_eq!(intro.into_interleaved(2), [0.1, 0.1, 0.2, 0.2]);
    }

    #[test]
    fn stereo_intro_is_averaged_for_mono() {
        let intro = Intro {
            audio: Audio {
                sample_rate: 44_100,
                channels: 2,
                samples: vec![0.2, 0.4, -0.2, 0.0],
            },
        };
        let mono = intro.into_interleaved(1);
        assert!((mono[0] - 0.3).abs() < 1e-6 && (mono[1] + 0.1).abs() < 1e-6);
    }
}
