//! 곡의 소리 크기 재기.
//!
//! 비프와 음악의 균형을 잡을 때 기준으로 쓴다. 화면의 음량 슬라이더는 여기서 구한 세 지점
//! (최대·평균·최빈 크기)에 달라붙는다.
//!
//! 크기는 dB 로 나타낸다. 0 dB 가 낼 수 있는 가장 큰 소리이고, 작을수록 음수로 내려간다.
//! 6 dB 내려가면 진폭이 절반이다.

use serde::Serialize;

use crate::decode::Audio;
use crate::synth::HEADROOM;

/// 크기를 재는 구간의 길이와 이동 간격(초).
const WINDOW_SEC: f64 = 0.4;
const HOP_SEC: f64 = 0.1;
/// 이보다 작은 구간은 무음으로 보고 평균·최빈값에서 뺀다(곡의 최대 크기 대비 dB).
const SILENCE_BELOW_PEAK_DB: f64 = -50.0;
/// 화면의 슬라이더가 다루는 가장 작은 크기(dB). 이 아래는 "끔" 이다.
pub const SLIDER_FLOOR_DB: f64 = -40.0;
/// 음량 슬라이더의 맨 위(dB). 0 dB 를 넘으면 그만큼 더 키운다.
pub const SLIDER_CEIL_DB: f64 = 10.0;

/// 진폭 비율을 dB 로.
pub fn to_db(ratio: f64) -> f64 {
    if ratio > 0.0 {
        20.0 * libm::log10(ratio)
    } else {
        f64::NEG_INFINITY
    }
}

/// 음량이 100% 일 때 비프의 크기(dB). 비프는 한 음높이의 사인파라 평균 크기가 최대 진폭보다
/// 3 dB 낮다. 음악의 "평균 크기" 와 같은 잣대로 견주려고 평균 크기로 나타낸다.
pub fn beep_top_db() -> f64 {
    to_db(HEADROOM / std::f64::consts::SQRT_2)
}

/// 곡의 소리 크기.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MusicLevels {
    /// 파일에 담긴 그대로의 최대 크기(dB). 음원을 만들 때는 이 값이 0 dB 가 되도록 키운다.
    pub source_peak_db: f64,
    /// 아래 두 값은 최대 크기를 0 dB 로 맞췄을 때의 값이다(음악 음량 100% 기준).
    /// 평균 크기: 무음을 뺀 구간들의 평균 세기.
    pub mean_db: f64,
    /// 최빈 크기: 곡에서 가장 오래 머무는 크기.
    pub mode_db: f64,
}

impl MusicLevels {
    /// 곡의 소리 크기를 잰다. 소리가 없는 파일이면 `None`.
    pub fn measure(audio: &Audio) -> Option<Self> {
        let peak = f64::from(audio.peak());
        if peak <= 0.0 {
            return None;
        }
        let channels = audio.channels.max(1);
        let rate = f64::from(audio.sample_rate);
        let window = (libm::round(WINDOW_SEC * rate) as usize).max(1);
        let hop = (libm::round(HOP_SEC * rate) as usize).max(1);
        let frames = audio.frames();

        // 프레임별 세기(채널 평균)의 누적 합으로 구간 평균을 빠르게 구한다.
        let mut sums = Vec::with_capacity(frames / hop + 2);
        let mut total = 0.0f64;
        sums.push(0.0);
        for (i, frame) in audio.samples.chunks_exact(channels).enumerate() {
            total += frame
                .iter()
                .map(|s| f64::from(*s) * f64::from(*s))
                .sum::<f64>()
                / channels as f64;
            if (i + 1) % hop == 0 {
                sums.push(total);
            }
        }
        let per_window = (window / hop).max(1);
        let silence = peak * peak * libm::pow(10.0, SILENCE_BELOW_PEAK_DB / 10.0);

        let mut power_sum = 0.0;
        let mut count = 0usize;
        // 1 dB 칸의 도수 분포(최대 크기 아래 0~60 dB).
        let mut histogram = [0usize; 60];
        for i in per_window..sums.len() {
            let power = (sums[i] - sums[i - per_window]) / (per_window * hop) as f64;
            if power < silence {
                continue;
            }
            power_sum += power;
            count += 1;
            let below_peak = -(to_db(libm::sqrt(power)) - to_db(peak));
            let bin = (libm::floor(below_peak).max(0.0) as usize).min(histogram.len() - 1);
            histogram[bin] += 1;
        }
        if count == 0 {
            return None;
        }
        let mean_db = to_db(libm::sqrt(power_sum / count as f64)) - to_db(peak);
        // 가장 많이 나온 칸의 가운데. 같은 수면 더 큰 소리 쪽을 고른다.
        let (mode_bin, _) =
            histogram.iter().enumerate().fold(
                (0, 0),
                |best, (bin, n)| if *n > best.1 { (bin, *n) } else { best },
            );
        Some(Self {
            source_peak_db: to_db(peak),
            mean_db,
            mode_db: -(mode_bin as f64 + 0.5),
        })
    }

    /// 비프 음량 슬라이더가 달라붙는 지점들. 값은 음악 음량이 100% 일 때의 dB 이다.
    /// 음악 음량을 낮추면 그만큼 함께 내려간다.
    pub fn magnets(&self) -> Vec<Magnet> {
        vec![
            Magnet {
                key: "max",
                label: "최대",
                description: "곡에서 가장 큰 순간과 같은 크기",
                db: 0.0,
            },
            Magnet {
                key: "mean",
                label: "평균",
                description: "곡의 평균 크기 (적정)",
                db: self.mean_db,
            },
            Magnet {
                key: "mode",
                label: "최빈",
                description: "곡에서 가장 오래 머무는 크기",
                db: self.mode_db,
            },
        ]
    }
}

/// 슬라이더가 달라붙는 지점 하나.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Magnet {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub db: f64,
}

/// 화면에 넘기는 음량 안내.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelGuide {
    pub levels: MusicLevels,
    /// 비프 음량 슬라이더가 달라붙는 지점들(음악 음량 100% 기준 dB).
    pub magnets: Vec<Magnet>,
    /// 처음에 맞춰 둘 비프 크기(음악 음량 100% 기준 dB): 곡의 평균 크기.
    pub default_beep_db: f64,
    /// 곡의 박자 계열과 템포. 박자를 찾지 못한 곡은 `None`.
    pub tempo: Option<crate::music::TempoInfo>,
}

impl LevelGuide {
    pub fn new(levels: MusicLevels) -> Self {
        Self {
            levels,
            magnets: levels.magnets(),
            default_beep_db: levels.mean_db,
            tempo: None,
        }
    }
}

/// 음악 파일의 내용에서 음량 안내를 만든다.
pub fn level_guide(music: &[u8]) -> crate::Result<LevelGuide> {
    let audio = crate::decode::decode_audio(music)?;
    let mut guide = MusicLevels::measure(&audio)
        .map(LevelGuide::new)
        .ok_or_else(|| crate::Error::Decode("소리가 없는 파일입니다".into()))?;
    guide.tempo = crate::music::analyze_tempo(&audio);
    Ok(guide)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: Vec<f32>) -> Audio {
        Audio {
            sample_rate: 44_100,
            channels: 1,
            samples,
        }
    }

    fn sine(seconds: f64, amplitude: f32) -> Vec<f32> {
        (0..(seconds * 44_100.0) as usize)
            .map(|i| {
                amplitude * libm::sin(std::f64::consts::TAU * 440.0 * i as f64 / 44_100.0) as f32
            })
            .collect()
    }

    #[test]
    fn a_steady_tone_sits_three_db_under_its_peak() {
        let levels = MusicLevels::measure(&audio(sine(5.0, 0.5))).unwrap();
        assert!(
            (levels.source_peak_db - to_db(0.5)).abs() < 0.05,
            "{levels:?}"
        );
        // 사인파의 평균 크기는 최대 진폭보다 3.01 dB 낮다.
        assert!((levels.mean_db + 3.01).abs() < 0.05, "{levels:?}");
        assert!((levels.mode_db + 3.5).abs() < 0.01, "{levels:?}");
    }

    #[test]
    fn mode_follows_where_the_song_spends_most_time() {
        // 8초는 작게(-20 dB), 2초는 크게, 그리고 무음 5초.
        let mut samples = sine(8.0, 0.1);
        samples.extend(sine(2.0, 1.0));
        samples.extend(vec![0.0; 44_100 * 5]);
        let levels = MusicLevels::measure(&audio(samples)).unwrap();
        // 가장 오래 머문 크기는 작은 쪽(-23 dB 근처)이다.
        assert!((levels.mode_db + 23.5).abs() < 1.0, "{levels:?}");
        // 평균은 큰 구간에 끌려 올라가고, 무음은 계산에서 빠진다.
        assert!(
            levels.mean_db > -12.0 && levels.mean_db < -8.0,
            "{levels:?}"
        );
        let guide = LevelGuide::new(levels);
        assert_eq!(guide.magnets.len(), 3);
        assert_eq!(guide.default_beep_db, levels.mean_db);
        assert_eq!(guide.magnets[0].db, 0.0);
    }

    #[test]
    fn silence_has_no_levels() {
        assert_eq!(MusicLevels::measure(&audio(vec![0.0; 44_100])), None);
        assert!(level_guide(&[1, 2, 3]).is_err());
    }

    #[test]
    fn beep_top_matches_a_full_scale_beep() {
        assert!((beep_top_db() - (to_db(0.95) - 3.0103)).abs() < 1e-3);
    }
}
