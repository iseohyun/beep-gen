//! 길이를 아주 조금 바꾸는 재샘플링(창 함수 sinc 보간).
//!
//! 템포 변환 라이브러리는 배율을 내부 단위로 반올림해서, 요청한 길이와 실제 길이가
//! 0.1% 안팎으로 다르다. 그 차이를 그대로 두면 수백 박 뒤에는 비프와 음악이 눈에 띄게
//! 어긋나므로, 남은 차이만큼 여기서 다시 맞춘다. 배율이 1에 아주 가까운 경우만 쓰기
//! 때문에 별도의 저역 필터는 두지 않는다.
//!
//! 구간 단위로 받아 구간 단위로 내보내므로 전체 음원을 메모리에 쌓지 않는다.

/// 보간에 쓰는 이웃 샘플 수.
const TAPS: usize = 32;
const HALF: i64 = (TAPS / 2) as i64;
/// 샘플 사이 위치를 나누는 단계 수. 클수록 정밀하다.
const PHASES: usize = 1024;

/// 구간 단위 재샘플러. 출력 길이는 입력의 `ratio` 배가 된다.
pub struct StreamResampler {
    channels: usize,
    /// 출력 한 프레임이 입력에서 차지하는 길이 (= 1 / ratio).
    step: f64,
    table: Vec<f32>,
    /// 아직 필요한 입력 프레임들.
    pending: Vec<f32>,
    /// `pending[0]` 의 입력 프레임 번호.
    pending_start: i64,
    /// 지금까지 받은 입력 프레임 수.
    received: i64,
    /// 다음에 만들 출력 프레임 번호.
    next_output: u64,
    passthrough: bool,
}

impl StreamResampler {
    pub fn new(channels: usize, ratio: f64) -> Self {
        let passthrough = (ratio - 1.0).abs() < 1e-9;
        Self {
            channels,
            step: 1.0 / ratio,
            table: if passthrough {
                Vec::new()
            } else {
                build_table()
            },
            pending: Vec::new(),
            pending_start: 0,
            received: 0,
            next_output: 0,
            passthrough,
        }
    }

    /// 입력 구간을 받아, 만들 수 있는 만큼의 출력을 `out` 뒤에 붙인다.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.pending.extend_from_slice(input);
        self.received += (input.len() / self.channels) as i64;
        self.produce(out, false);
    }

    /// 입력이 끝났음을 알리고 남은 출력을 내보낸다. 끝부분의 모자란 이웃은 무음으로 본다.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if !self.passthrough {
            self.produce(out, true);
        }
    }

    fn produce(&mut self, out: &mut Vec<f32>, finishing: bool) {
        let channels = self.channels;
        loop {
            let position = self.next_output as f64 * self.step;
            let base = libm::floor(position) as i64;
            if finishing {
                // 입력 길이에 해당하는 출력까지만 만든다.
                if position >= self.received as f64 {
                    break;
                }
            } else if base + HALF >= self.received {
                // 뒤쪽 이웃이 아직 도착하지 않았다.
                break;
            }

            let phase = libm::round((position - base as f64) * PHASES as f64) as usize;
            let coefficients = &self.table[phase * TAPS..(phase + 1) * TAPS];
            let first = base - HALF + 1;
            let out_start = out.len();
            out.resize(out_start + channels, 0.0);
            for (k, &coefficient) in coefficients.iter().enumerate() {
                let index = first + k as i64;
                if index < 0 || index >= self.received {
                    continue;
                }
                let at = (index - self.pending_start) as usize * channels;
                for c in 0..channels {
                    out[out_start + c] += self.pending[at + c] * coefficient;
                }
            }
            self.next_output += 1;
        }

        // 다음 출력에 더는 쓰이지 않는 앞쪽 입력을 버린다.
        let next_base = libm::floor(self.next_output as f64 * self.step) as i64;
        let keep_from = (next_base - HALF + 1).clamp(self.pending_start, self.received);
        let drop_frames = (keep_from - self.pending_start) as usize;
        if drop_frames > 0 {
            self.pending.drain(..drop_frames * channels);
            self.pending_start = keep_from;
        }
    }
}

/// 위상별 보간 계수표. `table[p * TAPS + k]` 는 위치 `(k - HALF + 1) - p / PHASES` 의 값이다.
fn build_table() -> Vec<f32> {
    let mut table = vec![0.0f32; (PHASES + 1) * TAPS];
    for phase in 0..=PHASES {
        let fraction = phase as f64 / PHASES as f64;
        let mut row = [0.0f64; TAPS];
        let mut sum = 0.0;
        for (k, value) in row.iter_mut().enumerate() {
            let x = (k as i64 - HALF + 1) as f64 - fraction;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                let px = std::f64::consts::PI * x;
                libm::sin(px) / px
            };
            // 블랙먼 창: 가장자리로 갈수록 부드럽게 0이 된다.
            let w = x / HALF as f64;
            let window = if w.abs() >= 1.0 {
                0.0
            } else {
                let c = std::f64::consts::PI * w;
                0.42 + 0.5 * libm::cos(c) + 0.08 * libm::cos(2.0 * c)
            };
            *value = sinc * window;
            sum += *value;
        }
        // 합이 1이 되도록 맞춰 음량이 변하지 않게 한다.
        for (k, value) in row.iter().enumerate() {
            table[phase * TAPS + k] = (value / sum) as f32;
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, period: f64) -> Vec<f32> {
        (0..frames)
            .map(|i| libm::sin(std::f64::consts::TAU * i as f64 / period) as f32)
            .collect()
    }

    fn run(input: &[f32], channels: usize, ratio: f64, chunk: usize) -> Vec<f32> {
        let mut resampler = StreamResampler::new(channels, ratio);
        let mut out = Vec::new();
        for part in input.chunks(chunk * channels) {
            resampler.push(part, &mut out);
        }
        resampler.finish(&mut out);
        out
    }

    #[test]
    fn output_length_follows_ratio() {
        let input = sine(100_000, 100.0);
        for ratio in [0.9985, 1.0, 1.00056, 1.0015] {
            let out = run(&input, 1, ratio, 4096);
            let expected = 100_000.0 * ratio;
            assert!(
                (out.len() as f64 - expected).abs() <= 1.0,
                "ratio {ratio}: {} vs {expected}",
                out.len()
            );
        }
    }

    #[test]
    fn chunk_size_does_not_change_output() {
        let input = sine(20_000, 37.0);
        let whole = run(&input, 1, 1.0007, 20_000);
        for chunk in [1, 13, 1024, 5000] {
            assert_eq!(run(&input, 1, 1.0007, chunk), whole, "chunk = {chunk}");
        }
    }

    #[test]
    fn stretches_a_sine_to_the_expected_period() {
        // 주기 100 샘플인 사인파를 1.001 배로 늘리면 주기가 100.1 샘플이 된다.
        let ratio = 1.001;
        let out = run(&sine(50_000, 100.0), 1, ratio, 4096);
        let worst = (100..out.len() - 100)
            .map(|i| {
                let expected = libm::sin(std::f64::consts::TAU * i as f64 / (100.0 * ratio));
                (f64::from(out[i]) - expected).abs()
            })
            .fold(0.0f64, f64::max);
        assert!(worst < 2e-3, "최대 오차 {worst}");
    }

    #[test]
    fn stereo_channels_stay_separate() {
        // 왼쪽은 사인파, 오른쪽은 무음. 섞이면 안 된다.
        let left = sine(10_000, 50.0);
        let input: Vec<f32> = left.iter().flat_map(|l| [*l, 0.0]).collect();
        let out = run(&input, 2, 1.0005, 1000);
        assert!(out.chunks_exact(2).all(|f| f[1] == 0.0));
        assert!(out.chunks_exact(2).any(|f| f[0].abs() > 0.5));
    }
}
