//! 소리 자체로 박 격자 찾기(템포가 거의 일정한 곡).
//!
//! 박자 검출기는 박을 하나씩 따라가다가, 드럼이 약한 구간에서 반 박쯤 미끄러지기도 한다.
//! 검출한 박을 그대로 믿고 따라가면(박 지도) 그 실수까지 따라가서, 그 뒤로는 비프가
//! 박 사이에 떨어진다.
//!
//! 대부분의 대중음악은 템포가 처음부터 끝까지 (거의) 똑같다. 그런 곡은 박 위치가
//! "첫 박 + k × 간격" 인 격자에 놓이므로, 검출기가 중간에 미끄러졌어도 격자로 바로잡을 수 있다.
//!
//! 방법: 소리가 갑자기 커지는 지점들을 박 간격으로 접어 겹친다. 박이 격자에 있으면 한 자리에
//! 몰린다. 이것을 20초 구간마다 해서, 몰리는 자리가 구간에서 구간으로 조금씩만 움직이면
//! 그 움직임까지 따라가는 격자를 만든다(템포가 0.5% 안에서 느리게 흔들리는 곡까지 맞는다).
//! 자리가 크게 움직이거나 흐릿한 곡은 다루지 않는다(`None`). 그런 곡은 검출한 박을 따라간다.
//!
//! 3박 계열(3/4, 6/8 등)도 여기서 가려낸다. 검출기는 이런 곡의 맥박 두 개를 한 박으로 읽는데,
//! 강세는 맥박 세 개마다 오므로 그대로 쓰면 비프의 강약이 곡의 강세와 엇갈린다. 맥박 세 개
//! 간격의 강세가 뚜렷하면 3박 계열로 보고, 격자를 맥박 단위로 내주며 강세의 자리를 함께 알려준다.

/// 소리 크기를 재는 간격(초).
const HOP_SEC: f64 = 0.004;
/// 박 간격을 다듬을 때 검출한 값에서 훑는 범위(±).
const SEARCH_RANGE: f64 = 0.01;
/// 훑는 간격(거친 단계, 고운 단계).
const COARSE_STEP: f64 = 1e-4;
const FINE_STEP: f64 = 5e-6;
/// 접어 겹칠 때 함께 보는 배음 수(박, 반 박, 1/3 박, 1/4 박).
const HARMONICS: usize = 4;
/// 구간의 길이와 이동 간격(초).
const WINDOW_SEC: f64 = 20.0;
const WINDOW_HOP_SEC: f64 = 10.0;
/// 구간 안에서 박이 이만큼은 또렷해야 그 구간을 믿는다.
const MIN_WINDOW_CLARITY: f64 = 0.03;
/// 소리가 있는 구간 중 이만큼은 또렷해야 격자를 믿는다.
const MIN_CLEAR_FRACTION: f64 = 0.7;
/// 이웃한 구간 사이에서 박의 자리가 움직여도 되는 한계(박 간격에 대한 비율).
/// 이동 간격 10초에 박의 1/12 이면 템포가 약 0.5% 벗어난 것이다.
const MAX_STEP_RATIO: f64 = 1.0 / 12.0;
/// 판단하려면 필요한 최소 구간 수.
const MIN_WINDOWS: usize = 3;
/// 구간별 흔들림까지 따라가려면, 그렇게 했을 때 킥이 이만큼은 더 또렷하게 겹쳐야 한다.
/// 접어 겹친 자리는 편곡이 바뀔 때도(하이햇이 들어오는 등) 조금 움직이는데, 그것은 템포가
/// 변한 것이 아니므로 따라가면 안 된다.
const FOLLOW_GAIN: f64 = 1.03;
/// 3박 계열로 보려면, 맥박 세 개 간격의 강세가 두 개·네 개 간격의 강세보다 이만큼은 커야 한다.
/// (실제 곡에서 3박 계열은 1.9배, 2박 계열은 0.6배 아래였다.)
const TRIPLE_GAIN: f64 = 1.3;

/// 소리에서 찾은 박 격자.
#[derive(Debug, Clone, PartialEq)]
pub struct SteadyGrid {
    /// 곡 전체의 평균 박 간격(원곡 프레임).
    pub period: f64,
    /// 격자의 기준 박 간격(원곡 프레임). 박의 위치는 `phase + k × base_period + 옮길 양` 이다.
    base_period: f64,
    /// 기준 격자 위의 한 박의 위치(원곡 프레임, `0 ≤ phase < base_period`).
    pub phase: f64,
    /// 기준 격자를 옮기는 양: (원곡 위치, 옮길 프레임 수), 위치 오름차순, 두 점 이상.
    /// 템포가 완전히 일정한 곡은 한 직선 위에 있다.
    drift: Vec<(f64, f64)>,
    /// 구간별 흔들림까지 따라가는지(아니면 한 가지 템포의 고정 격자).
    pub follows_wavering: bool,
    /// 3박 계열(3/4, 6/8 등)인지. 그렇다면 격자의 한 칸은 맥박 하나(검출기가 본 박의 절반)이고,
    /// 강세는 세 칸마다 온다.
    pub triple: bool,
    /// 3박 계열일 때, `beats()` 의 몇 번째 칸에 첫 강세가 오는지(0~2). 2박 계열은 0.
    pub accent_offset: usize,
    /// 소리가 있는 구간 중 박이 또렷했던 비율(0~1).
    pub clear_fraction: f64,
}

impl SteadyGrid {
    /// `position` 근처에서 격자를 옮길 양. 점 사이는 직선으로 잇고, 양 끝 밖은 끝 조각을 늘인다.
    fn drift_at(&self, position: f64) -> f64 {
        let last = self.drift.len() - 2;
        let i = self
            .drift
            .partition_point(|point| point.0 <= position)
            .saturating_sub(1)
            .min(last);
        let (from, to) = (self.drift[i], self.drift[i + 1]);
        from.1 + (position - from.0) / (to.0 - from.0) * (to.1 - from.1)
    }

    /// 곡 길이(`frames`) 안에 드는 모든 박의 위치.
    pub fn beats(&self, frames: usize) -> Vec<f64> {
        let mut beats = Vec::new();
        let mut nominal = self.phase;
        while nominal < frames as f64 {
            beats.push(nominal + self.drift_at(nominal));
            nominal += self.base_period;
        }
        beats
    }

    /// `position` 에 가장 가까운 박의 순번(`beats()` 의 인덱스).
    pub fn nearest(&self, position: f64) -> usize {
        let nominal = position - self.drift_at(position) - self.phase;
        libm::round(nominal / self.base_period).max(0.0) as usize
    }
}

/// 복소수 한 개(실수부, 허수부).
type Complex = (f64, f64);

fn magnitude((re, im): Complex) -> f64 {
    libm::sqrt(re * re + im * im)
}

/// `envelope[from..to]` 를 주기 `period`(칸 수) 의 `harmonic` 배음으로 접은 합.
/// 값이 한 자리에 몰려 있을수록 크기가 크고, 각도가 그 자리를 가리킨다.
fn fold(envelope: &[f32], from: usize, to: usize, period: f64, harmonic: usize) -> Complex {
    let step = std::f64::consts::TAU * harmonic as f64 / period;
    let (sin, cos) = (libm::sin(step), libm::cos(step));
    let start = step * from as f64;
    let (mut re, mut im) = (libm::cos(start), libm::sin(start));
    let (mut sum_re, mut sum_im) = (0.0, 0.0);
    for (i, value) in envelope[from..to].iter().enumerate() {
        let value = f64::from(*value);
        sum_re += value * re;
        sum_im += value * im;
        // 회전을 곱셈으로 이어 간다(매번 sin/cos 를 부르지 않는다).
        (re, im) = (re * cos - im * sin, re * sin + im * cos);
        // 곱셈이 쌓여 크기가 1에서 벗어나지 않게 가끔 바로잡는다.
        if i % 4096 == 4095 {
            let norm = libm::sqrt(re * re + im * im);
            re /= norm;
            im /= norm;
        }
    }
    (sum_re, sum_im)
}

/// 소리가 갑자기 커지는 정도를 `hop` 프레임마다 잰다.
/// 반환: (전체 = 저역 + 고역, 저역만). 저역은 킥, 고역은 하이햇·스네어를 주로 잡는다.
fn onset_envelopes(mono: &[f32], sample_rate: u32, hop: usize) -> (Vec<f32>, Vec<f32>) {
    let blocks = mono.len() / hop;
    // 150Hz 아래만 남기는 필터(두 번 거른다).
    let alpha = 1.0 - libm::exp(-std::f64::consts::TAU * 150.0 / f64::from(sample_rate));
    let alpha = alpha as f32;
    let (mut low1, mut low2, mut previous) = (0.0f32, 0.0f32, 0.0f32);
    let mut low_level = vec![0.0f32; blocks];
    let mut high_level = vec![0.0f32; blocks];
    for (block, chunk) in mono.chunks_exact(hop).enumerate() {
        let (mut low_peak, mut high_peak) = (0.0f32, 0.0f32);
        for sample in chunk {
            low1 += alpha * (sample - low1);
            low2 += alpha * (low1 - low2);
            low_peak = low_peak.max(low2.abs());
            // 이웃 샘플의 차이는 높은 소리일수록 크다.
            high_peak = high_peak.max((sample - previous).abs());
            previous = *sample;
        }
        low_level[block] = low_peak;
        high_level[block] = high_peak;
    }

    let rises = |level: &[f32]| -> Vec<f32> {
        let mut out = vec![0.0f32; level.len()];
        for i in 1..level.len() {
            out[i] = (level[i] - level[i - 1]).max(0.0);
        }
        let mean = out.iter().sum::<f32>() / out.len().max(1) as f32;
        if mean > 0.0 {
            for value in &mut out {
                *value /= mean;
            }
        }
        out
    };
    let low = rises(&low_level);
    let high = rises(&high_level);
    let all = low.iter().zip(&high).map(|(l, h)| l + 0.5 * h).collect();
    (all, low)
}

/// 접었을 때 가장 또렷해지는 박 간격(칸 수)을 `guess` 근처에서 찾는다.
fn refine_period(envelope: &[f32], guess: f64) -> f64 {
    let score = |period: f64| -> f64 {
        (1..=HARMONICS)
            .map(|harmonic| {
                let (re, im) = fold(envelope, 0, envelope.len(), period, harmonic);
                re * re + im * im
            })
            .sum()
    };
    let search = |center: f64, range: f64, step: f64| -> f64 {
        let steps = libm::round(range / step) as i64;
        (-steps..=steps)
            .map(|k| center * (1.0 + k as f64 * step))
            .map(|period| (score(period), period))
            .fold((f64::MIN, center), |best, candidate| {
                if candidate.0 > best.0 {
                    candidate
                } else {
                    best
                }
            })
            .1
    };
    let coarse = search(guess, SEARCH_RANGE, COARSE_STEP);
    search(coarse, COARSE_STEP * 1.5, FINE_STEP)
}

/// 소리에서 박 격자를 찾는다.
///
/// - `mono`: 원곡(모노)
/// - `beats`: 검출기가 찾은 박 위치(원곡 프레임). 박의 자리(정박/엇박)를 고르는 데 쓴다.
/// - `period_guess`: 검출기가 본 평균 박 간격(원곡 프레임)
///
/// 템포가 크게 변하는 곡, 박이 흐릿한 곡, 너무 짧은 곡은 `None`.
pub fn find_steady_grid(
    mono: &[f32],
    sample_rate: u32,
    beats: &[f64],
    period_guess: f64,
) -> Option<SteadyGrid> {
    find_steady_grid_traced(mono, sample_rate, beats, period_guess, Meter::Auto, None)
}

/// `find_steady_grid` 와 같되, 박자 계열을 직접 정한다(분석이 틀렸을 때 사용자가 고친 값).
pub fn find_steady_grid_for(
    mono: &[f32],
    sample_rate: u32,
    beats: &[f64],
    period_guess: f64,
    meter: Meter,
) -> Option<SteadyGrid> {
    find_steady_grid_traced(mono, sample_rate, beats, period_guess, meter, None)
}

/// 곡의 박자 계열을 어떻게 정할지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meter {
    /// 소리를 보고 정한다.
    Auto,
    /// 2박 계열(2/4, 4/4)로 본다.
    Duple,
    /// 3박 계열(3/4, 6/8)로 본다.
    Triple,
}

/// 판단에 쓴 값들(진단용).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SteadyTrace {
    /// 다듬은 박 간격(원곡 프레임).
    pub period: f64,
    /// 소리가 있는 구간 수와 그중 박이 또렷한 구간 수.
    pub windows: usize,
    pub clear_windows: usize,
    /// 또렷한 구간들의 또렷함 가운데값.
    pub clarity: f64,
    /// 이웃 구간 사이에서 박의 자리가 가장 크게 움직인 양(원곡 프레임, 이동 간격 하나당).
    /// 평균 템포의 차이(한 방향으로 꾸준히 움직이는 몫)는 뺀 값이다.
    pub max_step: f64,
    /// 박의 자리가 움직인 전체 폭(원곡 프레임, 위와 같이 꾸준한 몫은 뺀 값).
    pub drift_range: f64,
    /// 킥이 겹치는 정도: 한 가지 템포로 볼 때와 구간별 흔들림을 따라갈 때.
    pub kick_rigid: f64,
    pub kick_following: f64,
}

/// -0.5 ~ 0.5 범위로 접는다.
fn wrap(value: f64) -> f64 {
    value - libm::round(value)
}

/// `find_steady_grid` 와 같되, 판단에 쓴 값을 `trace` 에 남긴다.
pub fn find_steady_grid_traced(
    mono: &[f32],
    sample_rate: u32,
    beats: &[f64],
    period_guess: f64,
    meter: Meter,
    trace: Option<&mut SteadyTrace>,
) -> Option<SteadyGrid> {
    let hop = (libm::round(f64::from(sample_rate) * HOP_SEC) as usize).max(1);
    let hop_frames = hop as f64;
    let (all, low) = onset_envelopes(mono, sample_rate, hop);
    let window = libm::round(WINDOW_SEC * f64::from(sample_rate) / hop_frames) as usize;
    let window_hop = libm::round(WINDOW_HOP_SEC * f64::from(sample_rate) / hop_frames) as usize;
    if beats.is_empty() || window == 0 || all.len() < window + (MIN_WINDOWS - 1) * window_hop {
        return None;
    }

    let period = refine_period(&all, period_guess / hop_frames);
    let period_frames = period * hop_frames;
    let half = period_frames / 2.0;

    // 구간마다 반 박 격자로 접는다. 편곡에 따라 정박이 셀 때도, 엇박이 셀 때도 있지만
    // 반 박 격자에서는 둘 다 같은 자리에 겹친다. 접은 합의 각도가 그 자리를 가리킨다.
    let place = |folded: Complex| libm::atan2(folded.1, folded.0) / std::f64::consts::TAU;
    let base_turn = place(fold(&all, 0, all.len(), period, 2));

    // (구간 번호, 구간 가운데의 원곡 위치, 전체 기준에서 벗어난 양[반 박 = 1])
    let mut clear: Vec<(usize, f64, f64)> = Vec::new();
    let mut clarities: Vec<f64> = Vec::new();
    let mut windows = 0usize;
    let mut number = 0usize;
    let mut from = 0;
    while from + window <= all.len() {
        let sum: f64 = all[from..from + window].iter().map(|v| f64::from(*v)).sum();
        // 소리가 거의 없는 구간(도입부의 침묵 등)은 세지 않는다.
        if sum > 0.05 * window as f64 {
            windows += 1;
            let folded = fold(&all, from, from + window, period, 2);
            let clarity = magnitude(folded) / sum;
            if clarity >= MIN_WINDOW_CLARITY {
                let center = (from as f64 + window as f64 / 2.0) * hop_frames;
                clear.push((number, center, wrap(place(folded) - base_turn)));
                clarities.push(clarity);
            }
        }
        from += window_hop;
        number += 1;
    }

    if clear.len() < MIN_WINDOWS {
        return None;
    }
    // 이웃 구간과 이어지도록 편다(반 박을 넘어간 것처럼 보이는 값을 바로잡는다).
    for i in 1..clear.len() {
        let previous = clear[i - 1].2;
        clear[i].2 = previous + wrap(clear[i].2 - previous);
    }
    // 옮길 양을 프레임으로 바꾸고, 한 방향으로 꾸준히 움직이는 몫(직선)을 가려낸다.
    // 그 몫은 박 간격을 조금 잘못 잡은 것일 뿐이라 템포가 흔들린 것이 아니다.
    let points: Vec<(f64, f64)> = clear.iter().map(|w| (w.1, w.2 * half)).collect();
    let count = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / count;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / count;
    let spread: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.0 - mean_x)).sum();
    let slope = points
        .iter()
        .map(|p| (p.0 - mean_x) * (p.1 - mean_y))
        .sum::<f64>()
        / spread;
    let line = |x: f64| mean_y + slope * (x - mean_x);
    let residuals: Vec<f64> = points.iter().map(|p| p.1 - line(p.0)).collect();

    let mut max_step = 0.0f64;
    for i in 1..clear.len() {
        let gap = (clear[i].0 - clear[i - 1].0) as f64;
        max_step = max_step.max((residuals[i] - residuals[i - 1]).abs() / gap);
    }
    let (lowest, highest) = residuals
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), r| (lo.min(*r), hi.max(*r)));

    // 한 가지 템포의 격자(직선만)와, 구간별 흔들림까지 따라가는 격자.
    let (first_x, last_x) = (points[0].0, points[points.len() - 1].0);
    let rigid_drift = vec![(first_x, line(first_x)), (last_x, line(last_x))];
    let following_drift: Vec<(f64, f64)> = (0..points.len())
        .map(|i| {
            // 구간별 값의 잔떨림을 줄이려고 이웃과 평균한다.
            let from = i.saturating_sub(1);
            let to = (i + 2).min(points.len());
            let mean = residuals[from..to].iter().sum::<f64>() / (to - from) as f64;
            (points[i].0, line(points[i].0) + mean)
        })
        .collect();
    let mut grid = SteadyGrid {
        period: period_frames * (1.0 + slope),
        base_period: period_frames,
        phase: 0.0,
        drift: rigid_drift,
        follows_wavering: false,
        triple: false,
        accent_offset: 0,
        clear_fraction: clear.len() as f64 / windows.max(1) as f64,
    };

    // 킥(저역)이 격자에 얼마나 또렷하게 겹치는지로 둘 중 하나를 고른다.
    let kick_focus = |grid: &SteadyGrid| -> f64 {
        let (mut re, mut im, mut total) = (0.0, 0.0, 0.0);
        for (i, value) in low.iter().enumerate() {
            let value = f64::from(*value);
            if value <= 0.0 {
                continue;
            }
            let position = (i as f64 + 0.5) * hop_frames;
            let turn = (position - grid.drift_at(position)) / period_frames;
            let angle = std::f64::consts::TAU * (turn - libm::floor(turn));
            re += value * libm::cos(angle);
            im += value * libm::sin(angle);
            total += value;
        }
        if total > 0.0 {
            magnitude((re, im)) / total
        } else {
            0.0
        }
    };
    let kick_rigid = kick_focus(&grid);
    grid.drift = following_drift;
    let kick_following = kick_focus(&grid);
    if kick_following > kick_rigid * FOLLOW_GAIN {
        grid.follows_wavering = true;
    } else {
        grid.drift = vec![(first_x, line(first_x)), (last_x, line(last_x))];
    }

    if let Some(trace) = trace {
        clarities.sort_by(f64::total_cmp);
        *trace = SteadyTrace {
            period: grid.period,
            windows,
            clear_windows: clear.len(),
            clarity: clarities[clarities.len() / 2],
            max_step,
            drift_range: highest - lowest,
            kick_rigid,
            kick_following,
        };
    }
    if grid.clear_fraction < MIN_CLEAR_FRACTION || max_step > period_frames * MAX_STEP_RATIO {
        return None;
    }
    // 3박 계열(6/8, 12/8, 빠른 3/4)인지 본다. 검출기는 이런 곡의 맥박 두 개를 한 박으로 읽는데,
    // 실제 강세는 맥박 세 개마다 온다. 그대로 쓰면 비프가 강박과 세 번에 한 번만 만난다.
    // 맥박 세 개 간격으로 접었을 때가 두 개·네 개 간격으로 접었을 때보다 뚜렷하게 또렷하면 3박 계열이다.
    let sounds_triple = {
        let (mut three, mut two, mut four, mut total) = (0.0, 0.0, 0.0, 0.0);
        let mut from = 0;
        while from + window <= all.len() {
            total += all[from..from + window]
                .iter()
                .map(|v| f64::from(*v))
                .sum::<f64>();
            three += magnitude(fold(&all, from, from + window, period * 1.5, 1));
            two += magnitude(fold(&all, from, from + window, period, 1));
            four += magnitude(fold(&all, from, from + window, period * 2.0, 1));
            from += window_hop;
        }
        total > 0.0 && three / total >= MIN_WINDOW_CLARITY && three > two.max(four) * TRIPLE_GAIN
    };
    let triple = match meter {
        Meter::Auto => sounds_triple,
        Meter::Duple => false,
        Meter::Triple => true,
    };
    // 강세가 되풀이되는 간격과, 강세의 자리 후보 수(맥박 = 반 박 간격으로 늘어선다).
    let (beat_frames, slots) = if triple {
        (period_frames * 1.5, 3)
    } else {
        (period_frames, 2)
    };

    // 박의 자리는 맥박 격자 위의 후보 중 하나다. 검출한 박이 더 많이 모인 쪽과
    // 소리가 더 많이 떨어지는 쪽을 함께 보고 고른다.
    let base = (base_turn - libm::floor(base_turn)) * half;
    let tolerance = period_frames / 8.0;
    // 격자(후보)에서 떨어진 거리(-반 주기 ~ +반 주기).
    let near = |position: f64, candidate: f64| -> f64 {
        wrap((position - grid.drift_at(position) - candidate) / beat_frames) * beat_frames
    };
    // 그 자리에 떨어지는 소리의 비율.
    let landing = |envelope: &[f32], candidate: f64| -> f64 {
        let (mut on, mut total) = (0.0, 0.0);
        for (i, value) in envelope.iter().enumerate() {
            let value = f64::from(*value);
            total += value;
            if near((i as f64 + 0.5) * hop_frames, candidate).abs() < tolerance {
                on += value;
            }
        }
        if total > 0.0 { on / total } else { 0.0 }
    };
    let votes = |candidate: f64| -> f64 {
        if triple {
            // 검출한 박은 세 자리를 고르게 돌기 때문에 쓸모가 없다. 소리의 강세만 본다.
            return landing(&all, candidate) + landing(&low, candidate);
        }
        let close = beats
            .iter()
            .filter(|beat| near(**beat, candidate).abs() < tolerance)
            .count() as f64
            / beats.len() as f64;
        close + landing(&low, candidate)
    };
    let candidate = (0..slots)
        .map(|slot| base + slot as f64 * half)
        .map(|candidate| (votes(candidate), candidate))
        .fold(
            (f64::MIN, base),
            |best, next| {
                if next.0 > best.0 { next } else { best }
            },
        )
        .1;

    // 정확한 자리는 그쪽에 모인 검출 박들의 가운데값으로 정한다(검출기와 같은 기준을 쓴다).
    let mut offsets: Vec<f64> = beats
        .iter()
        .map(|beat| near(*beat, candidate))
        .filter(|offset| offset.abs() < tolerance)
        .collect();
    let shift = if offsets.is_empty() {
        0.0
    } else {
        offsets.sort_by(f64::total_cmp);
        offsets[offsets.len() / 2]
    };
    let phase = candidate + shift;
    let phase = phase - libm::floor(phase / beat_frames) * beat_frames;
    if triple {
        // 3박 계열의 격자는 맥박마다 한 칸이다. 비프는 맥박(또는 맥박 두 개)마다 울리고,
        // 강세는 세 칸마다 오므로 첫 강세가 몇 번째 칸인지 함께 알려준다.
        let offset = libm::floor(phase / half);
        grid.triple = true;
        grid.accent_offset = offset as usize;
        grid.phase = phase - offset * half;
        grid.period /= 2.0;
        grid.base_period = half;
    } else {
        grid.phase = phase;
    }
    Some(grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// 박마다 낮은 소리, 박 사이마다 약한 잡음이 있는 곡. `beat_at(초)` 가 그 시점의 박 길이(초).
    fn music(seconds: f64, first: f64, beat_at: impl Fn(f64) -> f64) -> (Vec<f32>, Vec<f64>) {
        let frames = (seconds * f64::from(RATE)) as usize;
        let mut mono = vec![0.0f32; frames];
        let mut beats = Vec::new();
        let mut seed = 7u32;
        let mut time = first;
        while time < seconds - 0.5 {
            let start = (time * f64::from(RATE)) as usize;
            beats.push(time * f64::from(RATE));
            for i in 0..6000.min(frames - start) {
                let t = i as f64 / f64::from(RATE);
                mono[start + i] += (libm::sin(std::f64::consts::TAU * 60.0 * t)
                    * libm::exp(-t * 30.0)
                    * 0.8) as f32;
            }
            let beat = beat_at(time);
            let half = start + (beat / 2.0 * f64::from(RATE)) as usize;
            for i in 0..1200.min(frames.saturating_sub(half)) {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = f64::from(seed >> 8) / f64::from(1u32 << 24) * 2.0 - 1.0;
                mono[half + i] += (noise * libm::exp(-(i as f64) / 300.0) * 0.15) as f32;
            }
            time += beat;
        }
        (mono, beats)
    }

    /// 3박 계열 곡: 맥박(`pulse` 초)마다 약한 소리, 세 번째마다 강한 소리.
    /// 검출기는 이런 곡을 맥박 두 개가 한 박인 것으로 읽는다(반환하는 박이 그렇다).
    fn triple_music(seconds: f64, first: f64, pulse: f64) -> (Vec<f32>, Vec<f64>) {
        let frames = (seconds * f64::from(RATE)) as usize;
        let mut mono = vec![0.0f32; frames];
        let mut tracked = Vec::new();
        let mut index = 0usize;
        loop {
            let time = first + index as f64 * pulse;
            if time >= seconds - 0.5 {
                break;
            }
            let start = (time * f64::from(RATE)) as usize;
            // 검출기의 박은 강박과 한 맥박 어긋난 자리에서 시작한다(일부러 틀린 자리).
            if index % 2 == 1 {
                tracked.push(time * f64::from(RATE));
            }
            let (level, pitch) = if index.is_multiple_of(3) {
                (0.8, 60.0)
            } else {
                (0.2, 400.0)
            };
            for i in 0..6000.min(frames - start) {
                let t = i as f64 / f64::from(RATE);
                mono[start + i] += (libm::sin(std::f64::consts::TAU * pitch * t)
                    * libm::exp(-t * 30.0)
                    * level) as f32;
            }
            index += 1;
        }
        (mono, tracked)
    }

    #[test]
    fn a_triple_meter_song_gets_a_beat_of_three_pulses_on_the_accent() {
        let pulse = 0.306;
        let (mono, tracked) = triple_music(90.0, 0.4, pulse);
        let rate = f64::from(RATE);
        let grid = find_steady_grid(&mono, RATE, &tracked, 2.0 * pulse * rate).expect("격자");
        assert!(grid.triple);
        assert!(
            (grid.period / rate - pulse).abs() < 0.001,
            "{}",
            grid.period / rate
        );
        // 격자는 맥박마다 한 칸이고, 세 칸마다 강한 소리(0.4초 + 3맥박마다) 위에 있다.
        let beats = grid.beats(mono.len());
        assert!(beats.len() > 250, "{}", beats.len());
        for beat in beats.iter().skip(grid.accent_offset).step_by(3) {
            let offset = wrap((beat / rate - 0.4) / (3.0 * pulse)) * 3.0 * pulse;
            assert!(
                offset.abs() < 0.03,
                "박 {:.3}s 가 강박에서 {offset:.3}s 벗어남",
                beat / rate
            );
        }
    }

    #[test]
    fn the_meter_can_be_set_by_hand() {
        let rate = f64::from(RATE);
        // 3박 계열로 들리는 곡을 2박 계열로.
        let (mono, tracked) = triple_music(90.0, 0.4, 0.306);
        let grid =
            find_steady_grid_for(&mono, RATE, &tracked, 0.612 * rate, Meter::Duple).expect("격자");
        assert!(!grid.triple);
        assert!((grid.period / rate - 0.612).abs() < 0.001);
        // 2박 계열로 들리는 곡을 3박 계열로.
        let (mono, beats) = music(90.0, 0.3, |_| 0.5);
        let grid =
            find_steady_grid_for(&mono, RATE, &beats, 0.5 * rate, Meter::Triple).expect("격자");
        assert!(grid.triple);
        assert!((grid.period / rate - 0.25).abs() < 0.001);
    }

    #[test]
    fn a_duple_meter_song_is_not_taken_for_triple() {
        let (mono, beats) = music(90.0, 0.3, |_| 0.5);
        let grid = find_steady_grid(&mono, RATE, &beats, 0.5 * f64::from(RATE)).expect("격자");
        assert!(!grid.triple);
    }

    #[test]
    fn a_steady_song_gets_its_grid_even_if_the_tracker_slipped() {
        let beat = 0.6125;
        let (mono, true_beats) = music(120.0, 0.4, |_| beat);
        // 검출기가 곡의 60% 지점부터 반 박 미끄러졌다고 하자. 평균 간격도 조금 틀리게 준다.
        let slip_from = true_beats.len() * 6 / 10;
        let tracked: Vec<f64> = true_beats
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if i < slip_from {
                    *b
                } else {
                    b + beat / 2.0 * f64::from(RATE)
                }
            })
            .collect();
        let guess = beat * f64::from(RATE) * 1.0015;

        let grid = find_steady_grid(&mono, RATE, &tracked, guess).expect("일정한 템포");
        assert!(
            (grid.period / f64::from(RATE) - beat).abs() < 2e-5,
            "{grid:?}"
        );
        // 격자의 박이 곡의 끝에서도 실제 박 위에 있다.
        let rigid = grid.beats(mono.len());
        let last_true = true_beats[true_beats.len() - 1];
        let nearest = rigid[grid.nearest(last_true)];
        assert!(
            (nearest - last_true).abs() < 0.01 * f64::from(RATE),
            "끝 박이 {:.1}ms 어긋남",
            (nearest - last_true) / f64::from(RATE) * 1000.0
        );
        assert!((rigid[grid.nearest(true_beats[3])] - true_beats[3]).abs() < 441.0);
    }

    #[test]
    fn a_slowly_wavering_tempo_is_followed() {
        // 템포가 ±0.3% 느리게 흔들리는 곡. 고정 격자로는 ±57ms 까지 벗어난다.
        // 20초 구간으로 보므로 조금 늦게 따라가지만, 그 절반 아래로는 줄어야 한다.
        let waver = |t: f64| 0.6125 * (1.0 + 0.003 * libm::sin(std::f64::consts::TAU * t / 120.0));
        let (mono, true_beats) = music(180.0, 0.4, waver);
        let guess =
            (true_beats[true_beats.len() - 1] - true_beats[0]) / (true_beats.len() - 1) as f64;
        let mut trace = SteadyTrace::default();
        let grid = find_steady_grid_traced(
            &mono,
            RATE,
            &true_beats,
            guess,
            Meter::Auto,
            Some(&mut trace),
        )
        .unwrap_or_else(|| panic!("격자를 찾아야 한다: {trace:?}"));
        let rigid = grid.beats(mono.len());
        let worst = true_beats
            .iter()
            .skip(20)
            .take(true_beats.len() - 40)
            .map(|beat| (rigid[grid.nearest(*beat)] - beat).abs())
            .fold(0.0f64, f64::max);
        assert!(
            worst < 0.03 * f64::from(RATE),
            "가장 크게 어긋난 박 {:.1}ms ({trace:?})",
            worst / f64::from(RATE) * 1000.0
        );
    }

    #[test]
    fn a_drifting_song_is_not_called_steady() {
        // 템포가 3% 서서히 빨라지는 곡.
        let (mono, beats) = music(120.0, 0.4, |t| 0.6125 * (1.0 - 0.03 * t / 120.0));
        let guess = (beats[beats.len() - 1] - beats[0]) / (beats.len() - 1) as f64;
        assert_eq!(find_steady_grid(&mono, RATE, &beats, guess), None);
    }

    #[test]
    fn short_or_silent_audio_is_not_judged() {
        let (mono, beats) = music(20.0, 0.4, |_| 0.6);
        assert_eq!(find_steady_grid(&mono, RATE, &beats, 0.6 * 44_100.0), None);
        let silence = vec![0.0f32; RATE as usize * 60];
        assert_eq!(find_steady_grid(&silence, RATE, &[1000.0], 26_460.0), None);
    }
}
