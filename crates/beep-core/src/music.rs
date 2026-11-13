//! 배경 음악을 비프 템포에 맞추기.
//!
//! 1. 박자 분석: 음악의 박 간격과 기준 박(첫 마디 시작) 위치를 찾는다.
//! 2. 목표 결정: 음악의 한 박을 비프 간격의 0.5·1·2배 중 변환 폭이 가장 작은 쪽에 맞춘다.
//! 3. 템포 변환: 음정을 유지한 채 길이를 바꾼다.
//! 4. 미세 보정: 변환 라이브러리의 배율 반올림 오차를 재샘플링으로 없앤다.
//!
//! 3·4는 구간 단위로 진행해, 변환된 음악 전체를 메모리에 쌓지 않는다.

use serde::Serialize;
use timestretch::{
    Backend, Parameters, PreparedSession, ProcessStatus, SessionConfig, StreamSession, TempoRate,
    detect_beat_grid,
};

use crate::decode::Audio;
use crate::error::{Error, Result};
use crate::params::TrackParams;
use crate::resample::StreamResampler;

/// 음악의 한 박을 맞출 후보: 비프 간격의 몇 배인지.
const BEAT_MULTIPLES: [f64; 3] = [0.5, 1.0, 2.0];
/// 박 간격을 믿고 쓸 수 있는 최소 박 수.
const MIN_BEATS: usize = 8;
/// 이보다 템포를 많이 바꾸면 음질 저하를 경고한다.
const TEMPO_CHANGE_WARN_RATIO: f64 = 0.15;
/// 이보다 박자 검출 신뢰도가 낮으면 경고한다.
const LOW_CONFIDENCE: f32 = 0.5;
/// 마디 시작 위치를 믿고 쓸 최소 신뢰도.
const DOWNBEAT_CONFIDENCE: f32 = 0.5;
/// 템포 변환 라이브러리가 받는 입력 샘플 레이트 범위.
const SOURCE_RATE_RANGE: (u32, u32) = (32_000, 192_000);
/// 한 번에 꺼내는 변환 결과의 길이(프레임).
const BLOCK_FRAMES: usize = 1024;
/// 한 번에 진행시키는 변환 작업량.
const WORK_QUOTA: usize = 32;

/// 음악을 어떻게 맞췄는지에 대한 보고. CLI·화면·MCP 가 그대로 보여준다.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MusicReport {
    /// 검출한 원곡 템포(BPM).
    pub detected_bpm: f64,
    /// 박자 검출 신뢰도(0~1).
    pub beat_confidence: f32,
    /// 음악의 한 박을 맞춘 간격(초).
    pub target_beat_sec: f64,
    /// 속도 배율. 1보다 크면 빨라지고 작으면 느려진다.
    pub tempo_rate: f64,
    /// 템포 변화율(%). 양수면 빨라짐.
    pub tempo_change_pct: f64,
    /// 배율 반올림 오차를 없애려고 추가로 적용한 길이 배율.
    pub correction: f64,
    pub source_seconds: f64,
    pub output_seconds: f64,
    /// 강한 비프를 마디 시작에 맞췄는지(아니면 검출된 첫 박에 맞춤).
    pub anchored_on_downbeat: bool,
    /// 사용자가 알아야 할 주의 사항.
    pub warnings: Vec<String>,
}

/// 박 번호 i 와 위치 `beats[i]` 에 최소제곱 직선을 맞춘다.
/// 반환: (박 간격, 0번 박의 위치) — 둘 다 샘플 단위.
///
/// 대표 BPM(중앙값) 대신 이 값을 쓴다. 박 하나의 오차는 작아도 수백 박이 쌓이면
/// 곡 끝에서 크게 어긋나기 때문이다.
fn fit_beat_line(beats: &[f64]) -> Option<(f64, f64)> {
    if beats.len() < MIN_BEATS {
        return None;
    }
    let count = beats.len() as f64;
    let mean_index = (count - 1.0) / 2.0;
    let mean_position = beats.iter().sum::<f64>() / count;
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (i, position) in beats.iter().enumerate() {
        let d = i as f64 - mean_index;
        numerator += d * (position - mean_position);
        denominator += d * d;
    }
    let slope = numerator / denominator;
    (slope.is_finite() && slope > 0.0).then_some((slope, mean_position - slope * mean_index))
}

/// 템포에 맞춰진 음악을 구간 단위로 내보내는 생성기.
pub struct MusicStream {
    audio: Audio,
    session: StreamSession,
    resampler: StreamResampler,
    block: Vec<f32>,
    total_frames: u64,
    produced_frames: u64,
    anchor_frames: f64,
    finished: bool,
    report: MusicReport,
}

impl MusicStream {
    /// 음악을 분석해 `params` 의 비프 간격에 맞출 준비를 한다.
    pub fn new(audio: Audio, params: &TrackParams) -> Result<Self> {
        let (min_rate, max_rate) = SOURCE_RATE_RANGE;
        if !(min_rate..=max_rate).contains(&audio.sample_rate) {
            return Err(Error::Unsupported(format!(
                "음악의 샘플 레이트 {} Hz 는 지원하지 않습니다 ({min_rate}~{max_rate} Hz)",
                audio.sample_rate
            )));
        }
        let source_rate = f64::from(audio.sample_rate);
        let output_rate = f64::from(params.sample_rate);

        // 1. 박자 분석
        let grid = detect_beat_grid(&audio.to_mono(), audio.sample_rate);
        let (beat_samples, first_beat) = fit_beat_line(&grid.beats)
            .filter(|_| grid.bpm > 0.0)
            .ok_or(Error::TempoNotFound)?;
        let source_beat_sec = beat_samples / source_rate;

        // 2. 목표 결정: 변환 폭(배율의 로그 크기)이 가장 작은 후보를 고른다.
        let step_sec = params.step_sec();
        let (target_beat_sec, tempo_rate) = BEAT_MULTIPLES
            .iter()
            .map(|multiple| {
                let target = step_sec * multiple;
                (target, source_beat_sec / target)
            })
            .min_by(|a, b| libm::log(a.1).abs().total_cmp(&libm::log(b.1).abs()))
            .expect("후보는 비어 있지 않다");

        // 3. 템포 변환 준비
        let rate = TempoRate::new(tempo_rate)
            .map_err(|e| Error::Stretch(format!("템포 배율 {tempo_rate:.3}: {e}")))?;
        let session = PreparedSession::new(SessionConfig {
            source_sample_rate: audio.sample_rate,
            output_sample_rate: params.sample_rate,
            channels: audio.channels,
            backend: Backend::Portable,
            source_frames: Some(audio.frames() as u64),
            source_access: timestretch::SourceAccess::Seekable,
            parameters: Parameters::keylock(rate),
            ..SessionConfig::default()
        })
        .map_err(|e| Error::Stretch(e.to_string()))?
        .activate();

        // 4. 미세 보정: 라이브러리가 실제로 적용한 배율과 목표의 차이.
        let achieved = session.applied_parameters().achieved_stretch;
        let correction = target_beat_sec / (source_beat_sec * achieved);
        let total_scale = achieved * correction;
        let source_seconds = audio.duration_sec();
        let output_seconds = source_seconds * total_scale;
        let total_frames = libm::round(output_seconds * output_rate) as u64;

        // 강한 비프를 맞출 기준 박: 마디 시작을 믿을 수 있으면 그곳, 아니면 0번 박.
        let downbeat = grid
            .downbeats
            .first()
            .copied()
            .filter(|_| grid.downbeat_confidence >= DOWNBEAT_CONFIDENCE);
        let anchor_index = downbeat.unwrap_or(0);
        let anchor_source = first_beat + beat_samples * anchor_index as f64;
        let anchor_frames = anchor_source / source_rate * total_scale * output_rate;

        let tempo_change = tempo_rate - 1.0;
        let mut warnings = Vec::new();
        if tempo_change.abs() > TEMPO_CHANGE_WARN_RATIO {
            warnings.push(format!(
                "템포를 {:.0}% {} 바꿨습니다. 변환 폭이 커서 음질이 떨어질 수 있습니다.",
                tempo_change.abs() * 100.0,
                if tempo_change > 0.0 {
                    "빠르게"
                } else {
                    "느리게"
                }
            ));
        }
        if grid.confidence < LOW_CONFIDENCE {
            warnings.push(format!(
                "박자 검출 신뢰도가 낮습니다({:.0}%). 비프와 음악이 어긋날 수 있습니다.",
                grid.confidence * 100.0
            ));
        }
        if grid.segments.len() > 1 {
            warnings.push(
                "곡 중간에 템포가 변하는 것으로 보입니다. 뒤로 갈수록 비프와 어긋날 수 있습니다."
                    .into(),
            );
        }
        if grid.phase_untrusted {
            warnings
                .push("박의 위치를 확신하기 어렵습니다. 비프가 박과 엇갈릴 수 있습니다.".into());
        }

        let report = MusicReport {
            detected_bpm: 60.0 / source_beat_sec,
            beat_confidence: grid.confidence,
            target_beat_sec,
            tempo_rate,
            tempo_change_pct: tempo_change * 100.0,
            correction,
            source_seconds,
            output_seconds,
            anchored_on_downbeat: downbeat.is_some(),
            warnings,
        };

        Ok(Self {
            resampler: StreamResampler::new(audio.channels, correction),
            block: vec![0.0; BLOCK_FRAMES * audio.channels],
            audio,
            session,
            total_frames,
            produced_frames: 0,
            anchor_frames,
            finished: false,
            report,
        })
    }

    pub fn channels(&self) -> usize {
        self.audio.channels
    }

    /// 변환된 음악의 예상 전체 길이(출력 샘플 레이트 기준 프레임 수).
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// 지금까지 내보낸 길이(프레임).
    pub fn produced_frames(&self) -> u64 {
        self.produced_frames
    }

    /// 강한 비프를 맞출 기준 박의 위치(출력 프레임, 소수 포함).
    pub fn anchor_frames(&self) -> f64 {
        self.anchor_frames
    }

    /// 원곡의 최대 진폭.
    pub fn source_peak(&self) -> f32 {
        self.audio.peak()
    }

    pub fn report(&self) -> &MusicReport {
        &self.report
    }

    /// 다음 구간을 `out` 에 채운다(기존 내용은 지운다). 더 내보낼 것이 없으면 `false`.
    pub fn next_block(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        out.clear();
        if self.finished {
            return Ok(false);
        }
        let channels = self.audio.channels;
        let fail = |e: timestretch::SessionError| Error::Stretch(e.to_string());

        // 출력이 나올 때까지 변환을 진행시킨다.
        while out.is_empty() && !self.session.finished() {
            let progress = self.session.advance(WORK_QUOTA);
            match progress.status {
                ProcessStatus::Fault => {
                    return Err(self.session.fault().map_or_else(
                        || Error::Stretch("알 수 없는 변환 오류".into()),
                        |e| Error::Stretch(e.to_string()),
                    ));
                }
                ProcessStatus::NeedSource => {
                    let request = self
                        .session
                        .source_request()
                        .ok_or_else(|| Error::Stretch("입력 요청을 읽지 못했습니다".into()))?;
                    let samples = &self.audio.samples;
                    let start = (request.start_frame as usize * channels).min(samples.len());
                    let end = (start + request.frames * channels).min(samples.len());
                    if start == end {
                        self.session.finish_input().map_err(fail)?;
                    } else {
                        self.session
                            .supply(request, &samples[start..end])
                            .map_err(fail)?;
                    }
                }
                _ => {}
            }
            let stamp = self.session.read_output(&mut self.block).map_err(fail)?;
            self.resampler
                .push(&self.block[..stamp.frames * channels], out);
            self.session
                .acknowledge(stamp.consumed(stamp.frames).map_err(fail)?)
                .map_err(fail)?;
        }

        if self.session.finished() {
            self.resampler.finish(out);
            self.finished = true;
        }
        self.produced_frames += (out.len() / channels) as u64;
        Ok(!out.is_empty() || !self.finished)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_line_recovers_interval_and_offset() {
        // 간격 26460, 첫 박 1234 인 박에 ±3 샘플의 흔들림을 준다.
        let beats: Vec<f64> = (0..400)
            .map(|i| 1234.0 + 26_460.0 * f64::from(i) + f64::from((i * 5) % 7 - 3))
            .collect();
        let (interval, first) = fit_beat_line(&beats).unwrap();
        assert!((interval - 26_460.0).abs() < 0.01, "interval = {interval}");
        assert!((first - 1234.0).abs() < 2.0, "first = {first}");
    }

    #[test]
    fn beat_line_needs_enough_beats() {
        assert!(fit_beat_line(&[0.0, 100.0, 200.0]).is_none());
    }
}
