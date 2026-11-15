//! beep-gen 코어: 비프음 합성, 배경 음악 템포 맞추기, 인코딩.
//!
//! 이 크레이트는 웹(WASM)과 로컬(네이티브)이 그대로 함께 쓴다. 그래서 파일·네트워크·
//! 시계·OS 호출을 직접 쓰지 않는다. 입력은 값으로 받고, 결과는 바이트로 돌려준다.
//! (규칙: `.agents/rules/project_architecture.md` 2.2절)

pub mod beatmap;
pub mod decode;
pub mod describe;
pub mod encode;
pub mod error;
pub mod intro;
pub mod levels;
pub mod music;
pub mod naming;
pub mod params;
pub mod resample;
pub mod steady;
pub mod synth;

pub use describe::{AppInfo, describe};
pub use encode::OutputFormat;
pub use error::{Error, Result};
pub use intro::{DEFAULT_INTRO_TEMPLATE, intro_text};
pub use levels::{LevelGuide, level_guide};
pub use music::MusicReport;
pub use naming::{
    DEFAULT_NAME_FORMAT, MusicName, NAME_FORMATS, SongGuess, SongInfo, SongSource,
    format_file_name, identify_song, output_file_name,
};
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

/// 미리듣기용 발췌: 전체를 만들지 않고 일부 구간만 만든다.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Excerpt {
    /// 발췌할 길이(초).
    pub seconds: f64,
    /// 어디서부터 들을지: 전체 길이에 대한 위치(0~100%). 가장 가까운 강한 비프에서 시작한다.
    pub from_percent: f64,
}

impl Excerpt {
    /// 미리듣기의 기본 길이(초).
    pub const DEFAULT_SECONDS: f64 = 10.0;
    /// 미리듣기의 기본 시작 위치(%). 곡의 도입부를 지나 소리가 다 나오는 곳을 듣는다.
    pub const DEFAULT_FROM_PERCENT: f64 = 30.0;
    /// 발췌할 수 있는 길이의 범위(초).
    pub const SECONDS_RANGE: (f64, f64) = (1.0, 120.0);

    fn validate(&self) -> Result<()> {
        let (min, max) = Self::SECONDS_RANGE;
        if !(self.seconds.is_finite() && (min..=max).contains(&self.seconds)) {
            return Err(Error::InvalidParam {
                name: "preview",
                reason: format!("{} 은(는) {min}~{max} 초 범위를 벗어났습니다", self.seconds),
            });
        }
        if !(self.from_percent.is_finite() && (0.0..=100.0).contains(&self.from_percent)) {
            return Err(Error::InvalidParam {
                name: "preview-from",
                reason: format!("{} 은(는) 0~100 범위를 벗어났습니다", self.from_percent),
            });
        }
        Ok(())
    }
}

impl Default for Excerpt {
    fn default() -> Self {
        Self {
            seconds: Self::DEFAULT_SECONDS,
            from_percent: Self::DEFAULT_FROM_PERCENT,
        }
    }
}

/// 음원에 넣을 외부 소리. 모두 오디오 파일의 내용(바이트)으로 받는다.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackInputs<'a> {
    /// 배경 음악. 비프 템포에 맞춰 변환해 함께 넣는다.
    /// 이때 음원 길이는 (템포를 맞춘) 음악 길이를 따른다.
    pub music: Option<&'a [u8]>,
    /// 시작 안내 음성(TTS 등). 맨 앞에 넣고, 짧은 무음 뒤에 비프가 시작된다.
    pub intro: Option<&'a [u8]>,
    /// 미리듣기: 이 구간만 만든다. `None` 이면 전체를 만든다.
    pub excerpt: Option<Excerpt>,
}

/// 만들어진 음원.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackOutput {
    /// 완성된 파일 바이트.
    pub bytes: Vec<u8>,
    /// 맨 앞 안내 구간(음성 + 무음)의 길이(초). 안내 음성이 없으면 0.
    pub intro_seconds: f64,
    /// 배경 음악을 넣었을 때, 음악을 어떻게 맞췄는지에 대한 보고.
    pub music: Option<MusicReport>,
}

/// 음원 파일을 만든다. `on_progress` 는 진행 상황이 바뀔 때 호출된다.
pub fn build_track(
    params: &TrackParams,
    format: OutputFormat,
    inputs: TrackInputs,
    mut on_progress: impl FnMut(Progress),
) -> Result<TrackOutput> {
    params.validate()?;
    if let Some(excerpt) = &inputs.excerpt {
        excerpt.validate()?;
    }
    let intro = inputs
        .intro
        .map(|bytes| intro::decode_intro(bytes, params.sample_rate))
        .transpose()?;
    match inputs.music {
        None => build_beeps_only(params, format, intro, inputs.excerpt, &mut on_progress),
        Some(bytes) => build_with_music(
            params,
            format,
            bytes,
            intro,
            inputs.excerpt,
            &mut on_progress,
        ),
    }
}

/// 안내 구간을 인코더에 먼저 넣고 그 길이(초)를 돌려준다.
fn push_intro(
    encoder: &mut Encoder,
    intro: Option<intro::Intro>,
    sample_rate: u32,
    channels: usize,
) -> Result<f64> {
    let Some(intro) = intro else {
        return Ok(0.0);
    };
    let samples = intro.into_interleaved(channels);
    encoder.push(&samples)?;
    Ok((samples.len() / channels) as f64 / f64::from(sample_rate))
}

fn build_beeps_only(
    params: &TrackParams,
    format: OutputFormat,
    intro: Option<intro::Intro>,
    excerpt: Option<Excerpt>,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<TrackOutput> {
    // 비프만 있는 음원은 처음부터 끝까지 같으므로, 미리듣기는 앞부분을 들려준다.
    let mut sequence = match excerpt {
        Some(excerpt) => {
            let full = u64::from(params.length_sec) * u64::from(params.sample_rate);
            let wanted = libm::round(excerpt.seconds * f64::from(params.sample_rate)) as u64;
            BeepSequence::with_layout(params, wanted.min(full), 0.0)
        }
        None => BeepSequence::new(params),
    };
    let mut encoder = Encoder::new(format, params.sample_rate, 1);
    let intro_seconds = push_intro(&mut encoder, intro, params.sample_rate, 1)?;
    let total = sequence.total_frames();
    let mut chunk = vec![0.0f32; CHUNK_FRAMES];

    // 비프 음량(`beep-volume`)은 음악과의 균형을 잡는 값이라 여기서는 쓰지 않는다.
    loop {
        let count = sequence.render(&mut chunk);
        if count == 0 {
            break;
        }
        encoder.push(&chunk[..count])?;
        on_progress(Progress {
            stage: Stage::Render,
            percent: percent_of(sequence.position(), total, 0),
        });
    }

    Ok(TrackOutput {
        bytes: encoder.finish(),
        intro_seconds,
        music: None,
    })
}

fn build_with_music(
    params: &TrackParams,
    format: OutputFormat,
    music_bytes: &[u8],
    intro: Option<intro::Intro>,
    excerpt: Option<Excerpt>,
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
    if let Some(excerpt) = excerpt {
        stream.select_excerpt(params, excerpt)?;
    }

    let channels = stream.channels();
    let total = stream.total_frames();
    let mut beeps = BeepSequence::with_meter(
        params,
        total,
        stream.anchor_frames(),
        stream.report().triple_meter,
    );
    let mut encoder = Encoder::new(format, params.sample_rate, channels as u16);
    let intro_seconds = push_intro(&mut encoder, intro, params.sample_rate, channels)?;

    // 음악은 가장 큰 순간이 최대 크기가 되도록 맞춘 뒤 음량을 곱한다. 작게 녹음된 곡도
    // 음량 100% 에서 같은 크기로 나오고, 화면의 dB 눈금과 실제 소리가 어긋나지 않는다.
    // 음악과 비프의 최대 진폭 합이 1을 넘으면 소리가 찌그러지므로, 넘는 만큼 둘 다 줄인다.
    let music_volume = params.music_volume_pct / 100.0;
    let beep_volume = params.beep_volume_pct / 100.0;
    let source_peak = f64::from(stream.source_peak());
    let to_full_scale = if source_peak > 0.0 {
        1.0 / source_peak
    } else {
        1.0
    };
    let worst_peak = music_volume + beep_volume * synth::HEADROOM;
    let normalize = if worst_peak > 1.0 {
        1.0 / worst_peak
    } else {
        1.0
    };
    let music_gain = (music_volume * to_full_scale * normalize) as f32;
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
        intro_seconds,
        music: Some(stream.report().clone()),
    })
}

/// `start`%에서 시작해 100%까지, `done / total` 만큼 진행한 백분율.
fn percent_of(done: u64, total: u64, start: u64) -> u8 {
    let span = 100 - start;
    let part = (done.min(total) * span).checked_div(total).unwrap_or(span);
    (start + part) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_wav_has_expected_size_and_reports_progress() {
        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let mut last = None;
        let mut calls = 0;
        let output = build_track(&params, OutputFormat::Wav, TrackInputs::default(), |p| {
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
        let mp3 = build_track(&params, OutputFormat::Mp3, TrackInputs::default(), |_| {})
            .unwrap()
            .bytes;
        assert!((40_000..60_000).contains(&mp3.len()), "len = {}", mp3.len());
    }

    #[test]
    fn beeps_only_excerpt_is_the_first_seconds() {
        let params = TrackParams {
            length_sec: 60,
            ..TrackParams::default()
        };
        let inputs = TrackInputs {
            excerpt: Some(Excerpt {
                seconds: 3.0,
                from_percent: 50.0,
            }),
            ..TrackInputs::default()
        };
        let preview = build_track(&params, OutputFormat::Wav, inputs, |_| {}).unwrap();
        let full = build_track(&params, OutputFormat::Wav, TrackInputs::default(), |_| {}).unwrap();
        assert_eq!(preview.bytes.len(), 44 + 3 * 44_100 * 2);
        assert_eq!(preview.bytes[44..], full.bytes[44..preview.bytes.len()]);

        // 음원보다 긴 미리듣기는 음원 길이까지만.
        let short = TrackParams {
            length_sec: 2,
            ..params
        };
        let preview = build_track(&short, OutputFormat::Wav, inputs, |_| {}).unwrap();
        assert_eq!(preview.bytes.len(), 44 + 2 * 44_100 * 2);

        let bad = TrackInputs {
            excerpt: Some(Excerpt {
                seconds: 0.0,
                from_percent: 0.0,
            }),
            ..TrackInputs::default()
        };
        assert!(build_track(&short, OutputFormat::Wav, bad, |_| {}).is_err());
    }

    #[test]
    fn beep_volume_only_matters_with_music() {
        let full = TrackParams {
            length_sec: 1,
            beep_volume_pct: 100.0,
            ..TrackParams::default()
        };
        let half = TrackParams {
            beep_volume_pct: 50.0,
            ..full.clone()
        };
        let peak = |params: &TrackParams| {
            let wav = build_track(params, OutputFormat::Wav, TrackInputs::default(), |_| {})
                .unwrap()
                .bytes;
            wav[44..]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
                .max()
                .unwrap()
        };
        let (full_peak, half_peak) = (peak(&full), peak(&half));
        assert_eq!(full_peak, half_peak);
        assert!(full_peak > 30_000);
    }

    #[test]
    fn intro_comes_first_followed_by_a_pause() {
        // 16kHz 모노 1초짜리 안내 음성 → 44.1kHz 로 바뀌어 맨 앞에 들어가고 0.5초 쉰다.
        let voice: Vec<f32> = (0..16_000)
            .map(|i| {
                (libm::sin(std::f64::consts::TAU * 300.0 * f64::from(i) / 16_000.0) * 0.5) as f32
            })
            .collect();
        let mut wav = encode::WavEncoder::new(16_000, 1);
        wav.push(&voice);
        let intro = wav.finish();

        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let inputs = TrackInputs {
            intro: Some(&intro),
            ..TrackInputs::default()
        };
        let output = build_track(&params, OutputFormat::Wav, inputs, |_| {}).unwrap();
        assert!(
            (output.intro_seconds - 1.5).abs() < 0.001,
            "{}",
            output.intro_seconds
        );

        let samples: Vec<i16> = output.bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let rate = 44_100usize;
        assert!(
            (samples.len() as i64 - (rate * 7 / 2) as i64).abs() <= 2,
            "{}",
            samples.len()
        );
        let peak = |from: usize, to: usize| {
            samples[from..to]
                .iter()
                .map(|s| s.unsigned_abs())
                .max()
                .unwrap()
        };
        assert!(
            peak(rate / 10, rate * 9 / 10) > 15_000,
            "안내 음성이 있어야 합니다"
        );
        assert_eq!(
            peak(rate + 200, rate * 3 / 2 - 200),
            0,
            "안내 뒤에는 쉬어야 합니다"
        );
        // 비프는 안내 구간이 끝나는 1.5초에서 시작한다.
        assert!(
            peak(rate * 3 / 2, rate * 3 / 2 + 3528) > 30_000,
            "비프가 이어져야 합니다"
        );
    }

    #[test]
    fn invalid_params_are_rejected_before_any_work() {
        let params = TrackParams {
            subdivision: 0,
            ..TrackParams::default()
        };
        let mut called = false;
        let result = build_track(&params, OutputFormat::Wav, TrackInputs::default(), |_| {
            called = true
        });
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
        let inputs = TrackInputs {
            music: Some(&[1, 2, 3, 4, 5]),
            ..TrackInputs::default()
        };
        let result = build_track(&params, OutputFormat::Wav, inputs, |_| {});
        assert!(matches!(result, Err(Error::Decode(_))), "{result:?}");
    }
}
