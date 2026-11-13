//! 비프음 합성.
//!
//! 전체 길이를 한 번에 만들지 않고 요청받은 구간만 채운다. 덕분에 긴 음원도
//! 메모리를 거의 쓰지 않고, 호출자가 구간마다 진행률을 알릴 수 있다.
//!
//! 수학 함수는 `libm` 을 쓴다. 표준 `sin` 은 환경마다 구현이 달라
//! 웹(WASM)과 로컬의 결과가 미세하게 달라질 수 있기 때문이다.

use crate::params::TrackParams;

/// 비프 하나의 최대 길이(초).
const BEEP_DURATION_SEC: f64 = 0.08;
/// 비프가 간격에서 차지할 수 있는 최대 비율. 짧은 간격에서 비프끼리 겹치지 않게 한다.
const BEEP_MAX_STEP_RATIO: f64 = 0.6;
/// 딸깍 소리를 막는 시작·끝 페이드 길이(초).
const FADE_SEC: f64 = 0.005;
/// 클리핑 방지를 위한 최대 진폭 여유.
const HEADROOM: f64 = 0.95;

/// 사인파 한 토막을 만든다. 시작과 끝은 선형 페이드로 0에서 출발해 0으로 끝난다.
///
/// `volume` 은 0.0~1.0. 결과는 -1.0~1.0 범위의 모노 샘플이다.
pub fn create_tone(frequency: f64, duration_sec: f64, volume: f64, sample_rate: u32) -> Vec<f32> {
    let sr = f64::from(sample_rate);
    let len = (duration_sec * sr) as usize;
    if len == 0 {
        return Vec::new();
    }
    let fade = ((FADE_SEC * sr) as usize).min(len / 2);
    let amplitude = volume.clamp(0.0, 1.0) * HEADROOM;
    let omega = std::f64::consts::TAU * frequency;

    (0..len)
        .map(|i| {
            let envelope = if fade == 0 {
                1.0
            } else if i < fade {
                i as f64 / fade as f64
            } else if i >= len - fade {
                (len - 1 - i) as f64 / fade as f64
            } else {
                1.0
            };
            (libm::sin(omega * i as f64 / sr) * amplitude * envelope) as f32
        })
        .collect()
}

/// 비프 시퀀스를 구간 단위로 만들어 내는 생성기.
pub struct BeepSequence {
    primary: Vec<f32>,
    sub: Vec<f32>,
    subdivision: u64,
    /// 비프 사이의 간격(샘플, 소수 포함). 비프 위치는 매번 여기서 다시 계산해 오차가 쌓이지 않는다.
    step_samples: f64,
    total_frames: u64,
    position: u64,
}

impl BeepSequence {
    /// `params` 는 미리 `validate()` 를 통과한 값이어야 한다.
    pub fn new(params: &TrackParams) -> Self {
        let sr = f64::from(params.sample_rate);
        let step_sec = params.step_sec();
        let duration = BEEP_DURATION_SEC.min(step_sec * BEEP_MAX_STEP_RATIO);
        let primary = create_tone(params.primary_freq, duration, 1.0, params.sample_rate);
        let sub = if params.subdivision > 1 {
            create_tone(
                params.sub_freq,
                duration,
                params.weak_volume_pct / 100.0,
                params.sample_rate,
            )
        } else {
            Vec::new()
        };
        Self {
            primary,
            sub,
            subdivision: u64::from(params.subdivision.max(1)),
            step_samples: step_sec * sr,
            total_frames: u64::from(params.length_sec) * u64::from(params.sample_rate),
            position: 0,
        }
    }

    /// 전체 길이(샘플).
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// 지금까지 만든 길이(샘플).
    pub fn position(&self) -> u64 {
        self.position
    }

    /// `index` 번째 비프가 시작하는 위치(샘플).
    fn beat_start(&self, index: u64) -> u64 {
        libm::round(index as f64 * self.step_samples) as u64
    }

    /// 다음 구간을 `out` 에 채우고 채운 샘플 수를 돌려준다. 끝에 도달하면 0.
    pub fn render(&mut self, out: &mut [f32]) -> usize {
        let remaining = self.total_frames - self.position;
        let count = (out.len() as u64).min(remaining) as usize;
        let out = &mut out[..count];
        out.fill(0.0);
        if count == 0 {
            return 0;
        }

        let chunk_start = self.position;
        let chunk_end = chunk_start + count as u64;
        let tone_len = self.primary.len() as u64;

        // 이 구간에 걸칠 수 있는 첫 비프: 구간 시작보다 비프 한 개 길이만큼 앞선 것부터.
        let earliest = chunk_start.saturating_sub(tone_len);
        let mut index = libm::floor(earliest as f64 / self.step_samples) as u64;
        loop {
            let start = self.beat_start(index);
            if start >= chunk_end {
                break;
            }
            let tone = if index % self.subdivision == 0 {
                &self.primary
            } else {
                &self.sub
            };
            // 비프와 구간이 겹치는 부분만 복사한다.
            let from = chunk_start.max(start);
            let to = chunk_end.min(start + tone.len() as u64);
            if from < to {
                let dst = (from - chunk_start) as usize..(to - chunk_start) as usize;
                let src = (from - start) as usize..(to - start) as usize;
                out[dst].copy_from_slice(&tone[src]);
            }
            index += 1;
        }

        self.position = chunk_end;
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_all(params: &TrackParams, chunk: usize) -> Vec<f32> {
        let mut seq = BeepSequence::new(params);
        let mut all = Vec::new();
        let mut buf = vec![0.0f32; chunk];
        loop {
            let n = seq.render(&mut buf);
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        all
    }

    #[test]
    fn tone_starts_and_ends_at_zero() {
        let tone = create_tone(880.0, 0.08, 1.0, 44_100);
        assert_eq!(tone.len(), 3528);
        assert_eq!(tone[0], 0.0);
        assert_eq!(*tone.last().unwrap(), 0.0);
        let peak = tone.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= 0.95 && peak > 0.9, "peak = {peak}");
    }

    #[test]
    fn length_matches_request() {
        let params = TrackParams {
            length_sec: 3,
            ..TrackParams::default()
        };
        assert_eq!(render_all(&params, 4096).len(), 3 * 44_100);
    }

    #[test]
    fn chunk_size_does_not_change_output() {
        // 간격이 정수 샘플로 떨어지지 않는 경우(1.4초 / 3분할)까지 확인한다.
        let params = TrackParams {
            interval_sec: 1.4,
            subdivision: 3,
            length_sec: 5,
            ..TrackParams::default()
        };
        let whole = render_all(&params, 5 * 44_100);
        for chunk in [1, 7, 1000, 4096, 44_100] {
            assert_eq!(render_all(&params, chunk), whole, "chunk = {chunk}");
        }
    }

    #[test]
    fn beeps_land_on_the_grid_without_drift() {
        // 1.4초 / 3분할: 간격이 20580 샘플. 600초 뒤에도 비프가 정확한 위치에 있어야 한다.
        let params = TrackParams {
            interval_sec: 1.0,
            subdivision: 3,
            length_sec: 600,
            ..TrackParams::default()
        };
        let audio = render_all(&params, 65_536);
        let step = 44_100.0 / 3.0;
        for index in [0u64, 1, 2, 3, 1000, 1799] {
            let start = (index as f64 * step).round() as usize;
            // 시작 샘플은 페이드 때문에 0이고, 바로 다음부터 소리가 난다.
            assert_eq!(audio[start], 0.0, "beat {index}");
            assert!(
                audio[start + 20].abs() > 0.0,
                "beat {index} 가 비어 있습니다"
            );
            if start > 0 {
                assert_eq!(audio[start - 1], 0.0, "beat {index} 앞이 조용하지 않습니다");
            }
        }
    }

    #[test]
    fn strong_and_weak_beeps_alternate() {
        let params = TrackParams {
            length_sec: 2,
            ..TrackParams::default()
        };
        let audio = render_all(&params, 4096);
        let peak = |from: usize| {
            audio[from..from + 3528]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
        };
        let strong = peak(0);
        let weak = peak(22_050);
        assert!((strong - 0.95).abs() < 0.01, "strong = {strong}");
        assert!((weak - 0.95 * 0.35).abs() < 0.01, "weak = {weak}");
    }

    #[test]
    fn no_weak_beeps_when_not_subdivided() {
        let params = TrackParams {
            subdivision: 1,
            length_sec: 2,
            ..TrackParams::default()
        };
        let audio = render_all(&params, 4096);
        assert!(audio[22_050..26_000].iter().all(|v| *v == 0.0));
        assert!(audio[44_100 + 20].abs() > 0.0);
    }
}
