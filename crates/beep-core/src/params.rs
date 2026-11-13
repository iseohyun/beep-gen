//! 음원 생성 파라미터의 단일 정의.
//!
//! 이름·기본값·범위는 여기 한 곳에만 둔다. CLI 옵션(`cli` 기능)과
//! 이후의 UI 타입, MCP 도구 스키마는 모두 이 구조체에서 파생한다.

use crate::error::{Error, Result};

/// 기본값. UI·CLI·문서가 같은 값을 쓰도록 상수로 공개한다.
pub mod defaults {
    pub const INTERVAL_SEC: f64 = 1.0;
    pub const SUBDIVISION: u32 = 2;
    pub const WEAK_VOLUME_PCT: f64 = 35.0;
    pub const LENGTH_SEC: u32 = 300;
    pub const PRIMARY_FREQ: f64 = 880.0;
    pub const SUB_FREQ: f64 = 587.33;
    pub const SAMPLE_RATE: u32 = 44_100;
}

/// 허용 범위(양 끝 포함).
pub mod limits {
    pub const INTERVAL_SEC: (f64, f64) = (0.05, 600.0);
    pub const SUBDIVISION: (u32, u32) = (1, 64);
    pub const WEAK_VOLUME_PCT: (f64, f64) = (0.0, 100.0);
    pub const LENGTH_SEC: (u32, u32) = (1, 3600);
    pub const FREQ: (f64, f64) = (20.0, 20_000.0);
    /// MP3(MPEG-1 Layer III)가 지원하는 샘플 레이트.
    pub const SAMPLE_RATES: [u32; 3] = [32_000, 44_100, 48_000];
}

/// 비프 음원 하나를 정의하는 파라미터.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct TrackParams {
    /// 기본 비트 주기 n (초). 이 간격마다 강한 비프가 울린다.
    #[cfg_attr(
        feature = "cli",
        arg(short = 'n', long = "interval", default_value_t = defaults::INTERVAL_SEC)
    )]
    pub interval_sec: f64,

    /// 비트 쪼개기 수 m. 주기를 m등분한 지점마다 약한 비프가 울린다.
    #[cfg_attr(
        feature = "cli",
        arg(short = 'm', long = "subdivision", default_value_t = defaults::SUBDIVISION)
    )]
    pub subdivision: u32,

    /// 약한 비프의 볼륨 비율 (%, 0~100).
    #[cfg_attr(
        feature = "cli",
        arg(short = 'v', long = "weak-volume", default_value_t = defaults::WEAK_VOLUME_PCT)
    )]
    pub weak_volume_pct: f64,

    /// 비프 구간의 길이 l (초).
    #[cfg_attr(
        feature = "cli",
        arg(short = 'l', long = "length", default_value_t = defaults::LENGTH_SEC)
    )]
    pub length_sec: u32,

    /// 강한 비프의 주파수 (Hz).
    #[cfg_attr(
        feature = "cli",
        arg(long = "primary-freq", default_value_t = defaults::PRIMARY_FREQ)
    )]
    pub primary_freq: f64,

    /// 약한 비프의 주파수 (Hz).
    #[cfg_attr(
        feature = "cli",
        arg(long = "sub-freq", default_value_t = defaults::SUB_FREQ)
    )]
    pub sub_freq: f64,

    /// 샘플 레이트 (Hz). 32000, 44100, 48000 중 하나.
    #[cfg_attr(
        feature = "cli",
        arg(long = "sample-rate", default_value_t = defaults::SAMPLE_RATE)
    )]
    pub sample_rate: u32,
}

impl Default for TrackParams {
    fn default() -> Self {
        Self {
            interval_sec: defaults::INTERVAL_SEC,
            subdivision: defaults::SUBDIVISION,
            weak_volume_pct: defaults::WEAK_VOLUME_PCT,
            length_sec: defaults::LENGTH_SEC,
            primary_freq: defaults::PRIMARY_FREQ,
            sub_freq: defaults::SUB_FREQ,
            sample_rate: defaults::SAMPLE_RATE,
        }
    }
}

impl TrackParams {
    /// 모든 값이 허용 범위 안인지 검사한다. 첫 번째 위반을 돌려준다.
    pub fn validate(&self) -> Result<()> {
        check_f64("interval", self.interval_sec, limits::INTERVAL_SEC)?;
        check_u32("subdivision", self.subdivision, limits::SUBDIVISION)?;
        check_f64("weak-volume", self.weak_volume_pct, limits::WEAK_VOLUME_PCT)?;
        check_u32("length", self.length_sec, limits::LENGTH_SEC)?;
        check_f64("primary-freq", self.primary_freq, limits::FREQ)?;
        check_f64("sub-freq", self.sub_freq, limits::FREQ)?;
        if !limits::SAMPLE_RATES.contains(&self.sample_rate) {
            return Err(Error::InvalidParam {
                name: "sample-rate",
                reason: format!(
                    "{} 은(는) 지원하지 않습니다 (가능: {:?})",
                    self.sample_rate,
                    limits::SAMPLE_RATES
                ),
            });
        }
        // 나이퀴스트 한계: 주파수는 샘플 레이트의 절반보다 낮아야 한다.
        let nyquist = f64::from(self.sample_rate) / 2.0;
        for (name, freq) in [
            ("primary-freq", self.primary_freq),
            ("sub-freq", self.sub_freq),
        ] {
            if freq >= nyquist {
                return Err(Error::InvalidParam {
                    name,
                    reason: format!(
                        "{freq} Hz 는 샘플 레이트의 절반({nyquist} Hz)보다 낮아야 합니다"
                    ),
                });
            }
        }
        Ok(())
    }

    /// 비프 사이의 간격(초): 주기를 분할 수로 나눈 값.
    pub fn step_sec(&self) -> f64 {
        self.interval_sec / f64::from(self.subdivision.max(1))
    }

    /// 주기 n 을 사람이 읽는 형태로 (1.0 → "1", 1.4 → "1.4").
    pub fn interval_label(&self) -> String {
        format!("{}", self.interval_sec)
    }
}

fn check_f64(name: &'static str, value: f64, (min, max): (f64, f64)) -> Result<()> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidParam {
            name,
            reason: format!("{value} 은(는) {min}~{max} 범위를 벗어났습니다"),
        })
    }
}

fn check_u32(name: &'static str, value: u32, (min, max): (u32, u32)) -> Result<()> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidParam {
            name,
            reason: format!("{value} 은(는) {min}~{max} 범위를 벗어났습니다"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        TrackParams::default().validate().unwrap();
    }

    #[test]
    fn rejects_out_of_range_values() {
        let cases: [fn(&mut TrackParams); 7] = [
            |p| p.interval_sec = 0.0,
            |p| p.interval_sec = f64::NAN,
            |p| p.subdivision = 0,
            |p| p.weak_volume_pct = 100.1,
            |p| p.length_sec = 0,
            |p| p.sample_rate = 22_050,
            |p| p.primary_freq = 30_000.0,
        ];
        for change in cases {
            let mut p = TrackParams::default();
            change(&mut p);
            assert!(p.validate().is_err(), "{p:?} 가 통과하면 안 됩니다");
        }
    }

    #[test]
    fn interval_label_drops_trailing_zero() {
        let mut p = TrackParams::default();
        assert_eq!(p.interval_label(), "1");
        p.interval_sec = 1.4;
        assert_eq!(p.interval_label(), "1.4");
    }
}
