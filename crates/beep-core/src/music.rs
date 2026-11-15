//! 배경 음악을 비프 템포에 맞추기.
//!
//! 1. 박자 분석: 음악의 박 간격과 기준 박(첫 마디 시작) 위치를 찾는다.
//! 2. 목표 결정: 음악의 한 박을 비프 간격의 0.5·1·2배 중 변환 폭이 가장 작은 쪽에 맞춘다.
//! 3. 템포 변환: 음정을 유지한 채 길이를 바꾼다.
//! 4. 박 맞추기: 검출한 박 하나하나가 비프 격자의 제자리에 오도록, 구간마다 길이를
//!    조금씩 다르게 다시 맞춘다(재샘플링). 템포가 흔들리거나 바뀌는 곡도 끝까지 맞는다.
//!
//! 3·4는 구간 단위로 진행해, 변환된 음악 전체를 메모리에 쌓지 않는다.

use serde::Serialize;
use timestretch::{
    Backend, Parameters, PreparedSession, ProcessStatus, SessionConfig, StreamSession, TempoRate,
    detect_beat_grid,
};

use crate::Excerpt;
use crate::beatmap::BeatMap;
use crate::decode::Audio;
use crate::error::{Error, Result};
use crate::params::TrackParams;
use crate::resample::StreamResampler;
use crate::steady::{Meter, SteadyGrid, find_steady_grid_for};

/// 음악의 한 박을 맞출 후보: 비프 간격의 몇 배인지.
const BEAT_MULTIPLES: [f64; 3] = [0.5, 1.0, 2.0];
/// 3박 계열 곡에서 맥박 하나를 맞출 후보: 비프 간격의 몇 배인지. 비프 세 개가 한 마디가
/// 되려면 비프 하나가 맥박의 정수 배여야 한다(맥박 1·2·4개마다 비프 하나).
const PULSE_MULTIPLES: [f64; 3] = [0.25, 0.5, 1.0];
/// 3박 계열에서 강세 한 번에 드는 맥박 수.
const PULSES_PER_ACCENT: usize = 3;
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
/// 이보다 곡 안에서 템포가 많이 변하면 알린다(그만큼 구간마다 음높이가 달라진다).
const TEMPO_DRIFT_WARN_RATIO: f64 = 0.015;
/// 박을 맞추려고 한 구간의 길이를 바꿀 수 있는 한계(기준 배율 대비).
/// 박 검출이 크게 틀린 구간에서 소리가 심하게 일그러지는 것을 막는다.
const MAX_LOCAL_ADJUST: f64 = 1.12;
/// 발췌할 때 원하는 지점보다 앞에서부터 변환을 시작하는 길이(초).
/// 변환기가 자리를 잡는 동안의 소리는 버린다.
const EXCERPT_PREROLL_SEC: f64 = 0.5;
/// 한 번에 꺼내는 변환 결과의 길이(프레임).
const BLOCK_FRAMES: usize = 1024;
/// 한 번에 진행시키는 변환 작업량.
const WORK_QUOTA: usize = 32;

/// 음악을 어떻게 맞췄는지에 대한 보고. CLI·화면·MCP 가 그대로 보여준다.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MusicReport {
    /// 검출한 원곡 템포(BPM). 3박 계열은 강세(맥박 세 개)의 빠르기다.
    pub detected_bpm: f64,
    /// 박자 검출 신뢰도(0~1).
    pub beat_confidence: f32,
    /// 음악의 한 박(3박 계열은 맥박 하나)을 맞춘 간격(초).
    pub target_beat_sec: f64,
    /// 속도 배율. 1보다 크면 빨라지고 작으면 느려진다.
    pub tempo_rate: f64,
    /// 템포 변화율(%). 양수면 빨라짐.
    pub tempo_change_pct: f64,
    /// 박을 맞추려고 추가로 적용한 길이 배율의 평균.
    pub correction: f64,
    /// 곡 안에서 템포가 평균에서 벗어난 최대 폭(%). 템포가 일정한 곡은 0에 가깝다.
    pub tempo_drift_pct: f64,
    /// 템포가 끝까지 일정한 곡으로 판단해 고정 격자에 맞췄는지.
    /// 아니면 검출한 박을 하나씩 따라가며 맞췄다.
    pub steady_tempo: bool,
    /// 3박 계열(6/8 등)로 보고 맥박 세 개를 한 박으로 잡았는지.
    pub triple_meter: bool,
    pub source_seconds: f64,
    pub output_seconds: f64,
    /// 강한 비프를 마디 시작에 맞췄는지(아니면 검출된 첫 박에 맞춤).
    pub anchored_on_downbeat: bool,
    /// 사용자가 알아야 할 주의 사항.
    pub warnings: Vec<String>,
}

/// 곡의 박자 계열과 템포(곡을 불러왔을 때 보여주고, 음악 파일에 적어 둔다).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TempoInfo {
    /// 박자 계열: 2 = 2박 계열(2/4, 4/4), 3 = 3박 계열(3/4, 6/8).
    pub meter: u32,
    /// 템포(BPM). 3박 계열은 강세(맥박 세 개)의 빠르기다.
    pub bpm: f64,
    /// 맥박의 빠르기(BPM). 2박 계열은 `bpm` 의 두 배, 3박 계열은 세 배다.
    /// 박자 계열을 직접 바꿨을 때의 템포는 이 값을 계열(2 또는 3)로 나눈 것이다.
    pub pulse_bpm: f64,
    /// 템포가 끝까지 일정한 곡인지.
    pub steady: bool,
    /// 박자 검출 신뢰도(0~1).
    pub confidence: f32,
}

fn meter_of(params: &TrackParams) -> Meter {
    match params.meter {
        2 => Meter::Duple,
        3 => Meter::Triple,
        _ => Meter::Auto,
    }
}

/// 곡의 박자 계열과 템포를 알아낸다. 박자를 찾지 못한 곡은 `None`.
///
/// 몇 분의 몇 박자인지까지는 소리만으로 가릴 수 없다(3/4 와 6/8, 2/4 와 4/4 는 강세가 오는
/// 간격이 같다). 강세가 맥박 두 개마다 오는지 세 개마다 오는지만 가린다.
pub fn analyze_tempo(audio: &Audio) -> Option<TempoInfo> {
    let mono = audio.to_mono();
    let grid = detect_beat_grid(&mono, audio.sample_rate);
    let (tracked_beat, _) = fit_beat_line(&grid.beats).filter(|_| grid.bpm > 0.0)?;
    let steady: Option<SteadyGrid> = find_steady_grid_for(
        &mono,
        audio.sample_rate,
        &grid.beats,
        tracked_beat,
        Meter::Auto,
    );
    let rate = f64::from(audio.sample_rate);
    let per_minute = |frames: f64| 60.0 * rate / frames;
    Some(match &steady {
        Some(rigid) if rigid.triple => TempoInfo {
            meter: 3,
            bpm: per_minute(rigid.period * PULSES_PER_ACCENT as f64),
            pulse_bpm: per_minute(rigid.period),
            steady: true,
            confidence: grid.confidence,
        },
        other => {
            let beat = other.as_ref().map_or(tracked_beat, |rigid| rigid.period);
            TempoInfo {
                meter: 2,
                bpm: per_minute(beat),
                pulse_bpm: per_minute(beat / 2.0),
                steady: other.is_some(),
                confidence: grid.confidence,
            }
        }
    })
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
    /// 원곡의 어느 위치가 결과의 어느 위치에 와야 하는지.
    map: BeatMap,
    /// 변환 라이브러리가 낸 한 프레임이 결과에서 차지해야 하는 평균 길이.
    nominal_ratio: f64,
    /// 지금까지 재샘플러에 "이만큼 내보내라" 고 한 결과 길이의 합(프레임).
    planned_frames: f64,
    /// `audio` 의 0 프레임이 원곡에서 몇 번째 프레임인지(발췌할 때만 0 이 아니다).
    source_offset: f64,
    /// 재샘플러가 다음에 내놓을 프레임의, 전체 결과 기준 위치.
    stream_position: f64,
    /// 내보낼 구간의 시작(전체 결과 기준 프레임). 발췌가 아니면 0.
    window_start: f64,
    /// 템포 배율(변환기를 다시 만들 때 쓴다).
    tempo_rate: f64,
    /// 일부 구간만 내보내는 중인지.
    excerpt: bool,
    /// 재샘플러의 출력을 잠시 담는 곳.
    scratch: Vec<f32>,
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
        let mono = audio.to_mono();
        let grid = detect_beat_grid(&mono, audio.sample_rate);
        let (tracked_beat, _) = fit_beat_line(&grid.beats)
            .filter(|_| grid.bpm > 0.0)
            .ok_or(Error::TempoNotFound)?;

        // 템포가 끝까지 일정한 곡이면 고정 격자를 쓴다. 검출기가 드럼이 약한 구간에서
        // 반 박쯤 미끄러졌더라도 격자가 바로잡는다. 그렇지 않은 곡은 검출한 박을 따라간다.
        let steady = find_steady_grid_for(
            &mono,
            audio.sample_rate,
            &grid.beats,
            tracked_beat,
            meter_of(params),
        );
        drop(mono);
        let triple = steady.as_ref().is_some_and(|rigid| rigid.triple);
        let (beats, segment_starts, beat_samples) = match &steady {
            Some(rigid) => (rigid.beats(audio.frames()), Vec::new(), rigid.period),
            None => (
                grid.beats.clone(),
                grid.segments.iter().map(|s| s.start_beat).collect(),
                tracked_beat,
            ),
        };
        let source_beat_sec = beat_samples / source_rate;

        // 2. 목표 결정: 변환 폭(배율의 로그 크기)이 가장 작은 후보를 고른다.
        let step_sec = params.step_sec();
        let multiples = if triple {
            PULSE_MULTIPLES
        } else {
            BEAT_MULTIPLES
        };
        let (target_beat_sec, tempo_rate) = multiples
            .iter()
            .map(|multiple| {
                let target = step_sec * multiple;
                (target, source_beat_sec / target)
            })
            .min_by(|a, b| libm::log(a.1).abs().total_cmp(&libm::log(b.1).abs()))
            .expect("후보는 비어 있지 않다");

        // 3. 템포 변환 준비
        let session = open_session(&audio, params.sample_rate, tempo_rate)?;

        // 4. 박 지도: 검출한 박 하나하나를 비프 격자의 제자리로 보낸다.
        let mut map = BeatMap::new(&beats, &segment_starts, target_beat_sec * output_rate)
            .ok_or(Error::TempoNotFound)?;
        let achieved = session.applied_parameters().achieved_stretch;
        // 라이브러리가 낸 길이를 평균적으로 얼마나 더 바꿔야 하는지(배율 반올림 오차 포함).
        let correction = target_beat_sec / (source_beat_sec * achieved);
        let source_seconds = audio.duration_sec();
        let total_frames = libm::round(map.at(audio.frames() as f64)).max(1.0) as u64;
        let output_seconds = total_frames as f64 / output_rate;

        // 곡 안에서 템포가 평균에서 얼마나 벗어나는지.
        let mean_slope = target_beat_sec * output_rate / beat_samples;
        let tempo_drift = beats
            .iter()
            .map(|beat| (map.slope_at(*beat) / mean_slope - 1.0).abs())
            .fold(0.0f64, f64::max);

        // 강한 비프를 맞출 기준 박: 마디 시작을 믿을 수 있으면 그곳, 아니면 0번 박.
        let downbeat = grid
            .downbeats
            .first()
            .copied()
            .filter(|_| grid.downbeat_confidence >= DOWNBEAT_CONFIDENCE);
        let anchor_index = match &steady {
            // 검출한 마디 시작에 가장 가까운 격자 박.
            // 3박 계열: 강세가 오는 칸 중에서 고른다(마디의 첫 비프가 강세에 온다).
            Some(rigid) if rigid.triple => {
                let near = downbeat.map_or(0, |index| rigid.nearest(grid.beats[index]));
                let bars = libm::round(
                    (near as f64 - rigid.accent_offset as f64) / PULSES_PER_ACCENT as f64,
                )
                .max(0.0) as usize;
                rigid.accent_offset + bars * PULSES_PER_ACCENT
            }
            Some(rigid) => downbeat.map_or(0, |index| rigid.nearest(grid.beats[index])),
            None => downbeat.unwrap_or(0),
        };
        let anchor_frames = map.beat_position(anchor_index);

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
        if tempo_drift > TEMPO_DRIFT_WARN_RATIO {
            warnings.push(format!(
                "곡 안에서 템포가 최대 {:.1}% 변합니다. 박마다 비프에 맞췄지만, 그만큼 구간에 따라 음높이가 조금 달라질 수 있습니다.",
                tempo_drift * 100.0
            ));
        }
        if params.meter == 3 && !triple {
            warnings.push(
                "템포가 일정하지 않은 곡이라 3박 계열로 맞추지 못했습니다. 2박 계열로 맞췄습니다."
                    .into(),
            );
        }
        if grid.phase_untrusted {
            warnings
                .push("박의 위치를 확신하기 어렵습니다. 비프가 박과 엇갈릴 수 있습니다.".into());
        }

        let report = MusicReport {
            detected_bpm: 60.0
                / if triple {
                    source_beat_sec * PULSES_PER_ACCENT as f64
                } else {
                    source_beat_sec
                },
            beat_confidence: grid.confidence,
            target_beat_sec,
            tempo_rate,
            tempo_change_pct: tempo_change * 100.0,
            correction,
            tempo_drift_pct: tempo_drift * 100.0,
            steady_tempo: steady.is_some(),
            triple_meter: triple,
            source_seconds,
            output_seconds,
            anchored_on_downbeat: downbeat.is_some(),
            warnings,
        };

        Ok(Self {
            resampler: StreamResampler::new(audio.channels, correction),
            map,
            nominal_ratio: correction,
            planned_frames: 0.0,
            source_offset: 0.0,
            stream_position: 0.0,
            window_start: 0.0,
            tempo_rate,
            excerpt: false,
            scratch: Vec::new(),
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

    /// 전체 대신 일부 구간만 내보내도록 바꾼다(미리듣기).
    ///
    /// 구간은 `excerpt.from_percent` 에 가장 가까운 강한 비프에서 시작한다.
    /// 그 뒤로 `total_frames`·`anchor_frames` 는 발췌한 구간 기준의 값이 된다.
    pub fn select_excerpt(&mut self, params: &TrackParams, excerpt: Excerpt) -> Result<()> {
        let output_rate = f64::from(params.sample_rate);
        let full = self.total_frames as f64;
        let length = libm::round(excerpt.seconds * output_rate).min(full);
        // 강한 비프 사이의 간격. 3박 계열은 마디(비프 세 개)의 길이.
        let bar = if self.report.triple_meter {
            params.step_sec() * PULSES_PER_ACCENT as f64 * output_rate
        } else {
            params.interval_sec * output_rate
        };
        let wanted = full * excerpt.from_percent / 100.0;
        let last_start = (full - length).max(0.0);
        // 원하는 위치에서 가장 가까운 강한 비프. 끝을 넘으면 앞쪽 비프로 당긴다.
        let mut bars = libm::round((wanted - self.anchor_frames) / bar);
        while self.anchor_frames + bars * bar > last_start {
            bars -= 1.0;
        }
        let start = libm::round((self.anchor_frames + bars * bar).max(0.0));

        // 변환은 조금 앞에서부터 시작하고, 원곡도 필요한 만큼만 남긴다.
        let source_rate = f64::from(self.audio.sample_rate);
        let channels = self.audio.channels;
        let frames = self.audio.frames();
        let first = (libm::floor(self.map.source_at(start) - EXCERPT_PREROLL_SEC * source_rate)
            .max(0.0) as usize)
            .min(frames);
        let tail = EXCERPT_PREROLL_SEC * source_rate;
        let last =
            (libm::ceil(self.map.source_at(start + length) + tail) as usize).clamp(first, frames);
        self.audio.samples = self.audio.samples[first * channels..last * channels].to_vec();

        self.session = open_session(&self.audio, params.sample_rate, self.tempo_rate)?;
        self.resampler = StreamResampler::new(channels, self.nominal_ratio);
        self.source_offset = first as f64;
        self.planned_frames = self.map.at(first as f64);
        self.stream_position = self.planned_frames;
        self.window_start = start;
        self.excerpt = true;
        // 발췌한 구간 안에서 첫 강한 비프의 위치.
        let offset = self.anchor_frames - start;
        self.anchor_frames = offset - libm::floor(offset / bar) * bar;
        self.total_frames = length as u64;
        self.produced_frames = 0;
        self.finished = self.total_frames == 0;
        self.report.output_seconds = length / output_rate;
        Ok(())
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
        let mut raw = std::mem::take(&mut self.scratch);
        let fail = |e: timestretch::SessionError| Error::Stretch(e.to_string());

        // 출력이 나올 때까지 변환을 진행시킨다.
        while out.is_empty() && !self.session.finished() && !self.finished {
            raw.clear();
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
            self.session
                .acknowledge(stamp.consumed(stamp.frames).map_err(fail)?)
                .map_err(fail)?;
            if stamp.frames > 0 {
                // 이 구간이 원곡의 어디까지인지 라이브러리가 알려준다. 그 위치가 결과에서
                // 놓여야 할 자리까지, 이 구간으로 정확히 채우도록 길이 배율을 정한다.
                // 앞에서 모자랐거나 넘친 만큼은 여기서 함께 갚으므로 오차가 쌓이지 않는다.
                let target = self.map.at(self.source_offset + self.session.position());
                let wanted = (target - self.planned_frames) / stamp.frames as f64;
                let ratio = wanted.clamp(
                    self.nominal_ratio / MAX_LOCAL_ADJUST,
                    self.nominal_ratio * MAX_LOCAL_ADJUST,
                );
                self.planned_frames += ratio * stamp.frames as f64;
                self.resampler.set_ratio(ratio);
                self.resampler
                    .push(&self.block[..stamp.frames * channels], &mut raw);
            }
            self.emit(&raw, out);
        }

        if self.session.finished() && !self.finished {
            raw.clear();
            self.resampler.finish(&mut raw);
            self.emit(&raw, out);
            self.finished = true;
        }
        self.scratch = raw;
        Ok(!out.is_empty() || !self.finished)
    }

    /// 재샘플러가 낸 소리 중 내보낼 구간에 드는 부분만 `out` 에 붙인다.
    fn emit(&mut self, raw: &[f32], out: &mut Vec<f32>) {
        let channels = self.audio.channels;
        let frames = raw.len() / channels;
        // 이 묶음의 첫 프레임이 내보낼 구간에서 몇 번째인지(음수면 구간 앞).
        let first = libm::round(self.stream_position - self.window_start) as i64;
        self.stream_position += frames as f64;
        // 발췌가 아니면 길이를 자르지 않는다(예상 길이와 한두 프레임 다를 수 있다).
        let limit = if self.excerpt {
            self.total_frames as i64
        } else {
            i64::MAX
        };
        let from = (-first).clamp(0, frames as i64);
        let to = (limit - first).clamp(from, frames as i64);
        out.extend_from_slice(&raw[from as usize * channels..to as usize * channels]);
        self.produced_frames += (to - from) as u64;
        if first + frames as i64 >= limit {
            self.finished = true;
        }
    }
}

/// 템포 변환기를 연다.
fn open_session(audio: &Audio, output_sample_rate: u32, tempo_rate: f64) -> Result<StreamSession> {
    let rate = TempoRate::new(tempo_rate)
        .map_err(|e| Error::Stretch(format!("템포 배율 {tempo_rate:.3}: {e}")))?;
    Ok(PreparedSession::new(SessionConfig {
        source_sample_rate: audio.sample_rate,
        output_sample_rate,
        channels: audio.channels,
        backend: Backend::Portable,
        source_frames: Some(audio.frames() as u64),
        source_access: timestretch::SourceAccess::Seekable,
        parameters: Parameters::keylock(rate),
        ..SessionConfig::default()
    })
    .map_err(|e| Error::Stretch(e.to_string()))?
    .activate())
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
