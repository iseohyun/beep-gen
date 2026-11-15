//! 박 지도: 원곡의 어느 위치가 결과의 어느 위치에 와야 하는지.
//!
//! 사람이 연주한 곡은 템포가 조금씩 흔들리고, 곡 중간에 템포가 바뀌기도 한다. 곡 전체를
//! 한 가지 템포로 보고 한 배율로만 늘이면, 처음에는 맞다가 뒤로 갈수록 비프와 어긋난다
//! (템포가 2%만 변해도 4분짜리 곡에서 반 박까지 벌어진다).
//!
//! 그래서 검출한 박 하나하나를 비프 격자의 제자리로 보내는 지도를 만든다:
//! `k` 번째 박(원곡의 `source[k]` 프레임) → 결과의 `k × 박 간격` 프레임.
//! 박과 박 사이는 직선으로 잇는다.

/// 박 위치를 다듬을 때 양옆으로 함께 보는 박 수.
/// 검출한 박은 한 박마다 ±10ms 안팎으로 흔들리는데, 그대로 따라가면 음악이 떨린다.
const SMOOTH_HALF_WINDOW: usize = 6;
/// 이 비율보다 간격이 많이 다르면 박을 놓쳤거나 잘못 끼운 것으로 본다.
const GAP_TOLERANCE: f64 = 0.25;
/// 지도를 만들 수 있는 최소 박 수.
pub const MIN_BEATS: usize = 8;

/// 원곡 위치(프레임) → 결과 위치(프레임)의 지도. 조각별 직선이며 항상 증가한다.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatMap {
    /// 박의 원곡 위치(원곡 프레임, 오름차순).
    source: Vec<f64>,
    /// 각 박의 박 번호. 검출이 박을 건너뛴 곳에서는 번호도 건너뛴다.
    index: Vec<f64>,
    /// 결과에서 한 박의 길이(결과 프레임).
    beat_frames: f64,
    /// 결과에서 원곡의 0 프레임이 놓이는 위치를 0 으로 맞추는 값.
    origin: f64,
    /// 직전 조회가 속한 조각(조회는 대체로 앞에서 뒤로 진행한다).
    cursor: usize,
}

/// 구간별 템포 경계: 이 박 번호부터 템포가 달라진다(다듬을 때 경계를 넘겨 섞지 않는다).
pub type SegmentStarts<'a> = &'a [usize];

impl BeatMap {
    /// 검출한 박 위치로 지도를 만든다.
    ///
    /// - `beats`: 박의 원곡 위치(프레임, 오름차순)
    /// - `segment_starts`: 템포가 바뀌는 지점의 박 순번(`beats` 의 인덱스)
    /// - `beat_frames`: 결과에서 한 박의 길이(프레임)
    ///
    /// 박이 너무 적거나 순서가 어긋나 있으면 `None`.
    pub fn new(beats: &[f64], segment_starts: SegmentStarts, beat_frames: f64) -> Option<Self> {
        if beats.len() < MIN_BEATS || !beat_frames.is_finite() || beat_frames <= 0.0 {
            return None;
        }
        let index = number_beats(beats)?;
        let source = smooth(beats, &index, segment_starts);
        // 다듬은 뒤에도 반드시 증가해야 한다(아니면 시간을 거꾸로 돌리게 된다).
        if source.windows(2).any(|w| w[1] <= w[0]) {
            return None;
        }
        let mut map = Self {
            source,
            index,
            beat_frames,
            origin: 0.0,
            cursor: 0,
        };
        map.origin = map.raw(0.0);
        Some(map)
    }

    /// 곡 전체에서 박 하나의 평균 길이(원곡 프레임).
    pub fn mean_source_beat(&self) -> f64 {
        let last = self.source.len() - 1;
        (self.source[last] - self.source[0]) / (self.index[last] - self.index[0])
    }

    /// `n` 번째로 검출된 박이 결과에서 놓이는 위치(프레임).
    pub fn beat_position(&self, n: usize) -> f64 {
        let n = n.min(self.index.len() - 1);
        self.index[n] * self.beat_frames - self.origin
    }

    /// 원곡의 `position` 프레임이 결과에서 놓일 위치(프레임).
    pub fn at(&mut self, position: f64) -> f64 {
        self.raw(position) - self.origin
    }

    /// 결과의 `output` 프레임에 놓이는 원곡 위치(프레임). `at` 의 반대 방향이다.
    pub fn source_at(&self, output: f64) -> f64 {
        let number = (output + self.origin) / self.beat_frames;
        let last = self.index.len() - 2;
        let i = self
            .index
            .partition_point(|n| *n <= number)
            .saturating_sub(1)
            .min(last);
        let t = (number - self.index[i]) / (self.index[i + 1] - self.index[i]);
        self.source[i] + t * (self.source[i + 1] - self.source[i])
    }

    /// 원곡의 `position` 근처에서, 원곡 한 프레임이 결과에서 차지하는 길이(프레임).
    pub fn slope_at(&mut self, position: f64) -> f64 {
        let i = self.segment(position);
        (self.index[i + 1] - self.index[i]) * self.beat_frames
            / (self.source[i + 1] - self.source[i])
    }

    /// `position` 이 속한 조각의 왼쪽 박 순번. 첫 박 앞과 끝 박 뒤는 가장자리 조각을 늘여 쓴다.
    fn segment(&mut self, position: f64) -> usize {
        let last = self.source.len() - 2;
        let mut i = self.cursor.min(last);
        while i > 0 && position < self.source[i] {
            i -= 1;
        }
        while i < last && position >= self.source[i + 1] {
            i += 1;
        }
        self.cursor = i;
        i
    }

    fn raw(&mut self, position: f64) -> f64 {
        let i = self.segment(position);
        let t = (position - self.source[i]) / (self.source[i + 1] - self.source[i]);
        (self.index[i] + t * (self.index[i + 1] - self.index[i])) * self.beat_frames
    }
}

/// 박마다 번호를 붙인다. 간격이 보통의 두 배쯤이면 박 하나를 놓친 것이므로 번호를 건너뛴다.
/// 간격이 0 이하인 곳이 있으면 `None`.
fn number_beats(beats: &[f64]) -> Option<Vec<f64>> {
    let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.iter().any(|gap| !gap.is_finite() || *gap <= 0.0) {
        return None;
    }
    let ordered = {
        gaps.sort_by(f64::total_cmp);
        gaps
    };
    let typical = ordered[ordered.len() / 2];

    let mut index = Vec::with_capacity(beats.len());
    let mut number = 0.0;
    index.push(number);
    let mut local = typical;
    for pair in beats.windows(2) {
        let gap = pair[1] - pair[0];
        let steps = libm::round(gap / local).max(1.0);
        // 정상 간격이면 그 값을 따라가며 "보통 간격" 을 고친다(템포가 서서히 변하는 곡).
        if (gap / (local * steps) - 1.0).abs() < GAP_TOLERANCE {
            local += (gap / steps - local) * 0.2;
        }
        number += steps;
        index.push(number);
    }
    Some(index)
}

/// 각 박의 위치를, 양옆 박들에 맞춘 직선 위의 값으로 바꾼다(흔들림 제거).
/// 템포가 바뀌는 경계는 넘지 않으므로, 템포가 갑자기 변하는 곳도 뭉개지지 않는다.
fn smooth(beats: &[f64], index: &[f64], segment_starts: &[usize]) -> Vec<f64> {
    let count = beats.len();
    let mut bounds: Vec<usize> = segment_starts
        .iter()
        .copied()
        .filter(|start| *start > 0 && *start < count)
        .collect();
    bounds.insert(0, 0);
    bounds.push(count);
    bounds.dedup();

    let mut out = beats.to_vec();
    for range in bounds.windows(2) {
        let (start, end) = (range[0], range[1]);
        // 구간이 너무 짧으면 직선을 믿을 수 없으므로 검출한 값을 그대로 둔다.
        if end - start < 4 {
            continue;
        }
        for k in start..end {
            let from = k.saturating_sub(SMOOTH_HALF_WINDOW).max(start);
            let to = (k + SMOOTH_HALF_WINDOW + 1).min(end);
            out[k] = line_value(&index[from..to], &beats[from..to], index[k]);
        }
    }
    out
}

/// 점들 `(x, y)` 에 최소제곱 직선을 맞추고 `at` 에서의 값을 돌려준다.
fn line_value(x: &[f64], y: &[f64], at: f64) -> f64 {
    let count = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / count;
    let mean_y = y.iter().sum::<f64>() / count;
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (xi, yi) in x.iter().zip(y) {
        numerator += (xi - mean_x) * (yi - mean_y);
        denominator += (xi - mean_x) * (xi - mean_x);
    }
    if denominator <= 0.0 {
        return mean_y;
    }
    mean_y + numerator / denominator * (at - mean_x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady(count: usize, first: f64, gap: f64) -> Vec<f64> {
        (0..count).map(|i| first + gap * i as f64).collect()
    }

    #[test]
    fn steady_beats_land_on_the_grid() {
        let beats = steady(100, 1000.0, 26_460.0);
        let mut map = BeatMap::new(&beats, &[], 30_000.0).unwrap();
        assert!((map.mean_source_beat() - 26_460.0).abs() < 1e-6);
        // 원곡의 맨 앞은 결과의 맨 앞.
        assert!(map.at(0.0).abs() < 1e-6);
        let first = map.at(1000.0);
        for (k, beat) in beats.iter().enumerate() {
            let expected = first + 30_000.0 * k as f64;
            assert!((map.at(*beat) - expected).abs() < 1e-3, "박 {k}");
            assert!((map.beat_position(k) - expected).abs() < 1e-3, "박 {k}");
        }
        // 반대 방향으로도 같은 자리를 가리킨다.
        for position in [0.0, 1000.0, 123_456.7, beats[99] + 5000.0] {
            let output = map.at(position);
            assert!(
                (map.source_at(output) - position).abs() < 1e-6,
                "{position}"
            );
        }
        // 끝 박 뒤로도 같은 기울기로 이어진다.
        let after = map.at(beats[99] + 26_460.0);
        assert!((after - (first + 30_000.0 * 100.0)).abs() < 1e-3);
        assert!((map.slope_at(50_000.0) - 30_000.0 / 26_460.0).abs() < 1e-9);
    }

    #[test]
    fn drifting_tempo_is_followed() {
        // 박 간격이 26460 에서 조금씩 줄어드는 곡(점점 빨라짐).
        let mut beats = vec![500.0];
        for i in 0..399 {
            beats.push(beats[i] + 26_460.0 * (1.0 - 0.02 * i as f64 / 399.0));
        }
        let mut map = BeatMap::new(&beats, &[], 30_000.0).unwrap();
        let first = map.at(beats[0]);
        for (k, beat) in beats.iter().enumerate() {
            let error = map.at(*beat) - (first + 30_000.0 * k as f64);
            // 한 가지 템포로만 보면 수천 프레임씩 어긋나는 곡이다.
            assert!(error.abs() < 30.0, "박 {k}: {error}");
        }
    }

    #[test]
    fn jitter_is_smoothed_out() {
        let beats: Vec<f64> = steady(200, 0.0, 26_460.0)
            .iter()
            .enumerate()
            .map(|(i, b)| b + ((i * 5) % 7) as f64 * 100.0 - 300.0)
            .collect();
        let mut map = BeatMap::new(&beats, &[], 26_460.0).unwrap();
        // 흔들림(±300)을 그대로 따라가지 않는다: 지도의 기울기가 거의 일정하다.
        for (k, beat) in beats.iter().enumerate().take(190).skip(10) {
            let slope = map.slope_at(beat + 10.0);
            assert!((slope - 1.0).abs() < 0.004, "박 {k}: {slope}");
        }
    }

    #[test]
    fn a_missed_beat_does_not_shift_the_rest() {
        let mut beats = steady(100, 0.0, 26_460.0);
        beats.remove(40);
        let mut map = BeatMap::new(&beats, &[], 30_000.0).unwrap();
        // 빠진 박 뒤의 박들도 원래 번호의 자리에 놓인다.
        let position = map.at(26_460.0 * 80.0);
        assert!((position - 30_000.0 * 80.0).abs() < 1.0, "{position}");
    }

    #[test]
    fn a_tempo_change_is_not_blurred_across_the_boundary() {
        let mut beats = steady(60, 0.0, 26_460.0);
        let start = beats[59];
        beats.extend((1..=60).map(|i| start + 27_562.5 * f64::from(i)));
        let mut map = BeatMap::new(&beats, &[60], 30_000.0).unwrap();
        for (k, beat) in beats.iter().enumerate() {
            let error = map.at(*beat) - 30_000.0 * k as f64;
            assert!(error.abs() < 1.0, "박 {k}: {error}");
        }
    }

    #[test]
    fn too_few_or_disordered_beats_are_refused() {
        assert!(BeatMap::new(&steady(5, 0.0, 100.0), &[], 100.0).is_none());
        let mut beats = steady(20, 0.0, 100.0);
        beats[7] = beats[6];
        assert!(BeatMap::new(&beats, &[], 100.0).is_none());
    }
}
