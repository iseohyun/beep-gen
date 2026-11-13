//! 음원 생성 파라미터의 단일 정의.
//!
//! 이름·기본값·범위는 여기 한 곳에만 둔다. CLI 옵션(`cli` 기능)과
//! 이후의 UI 타입, MCP 도구 스키마는 모두 이 구조체에서 파생한다.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct TrackParams {
    /// 기본 비트 주기 n (초). 이 간격마다 강한 비프가 울린다.
    #[cfg_attr(
        feature = "cli",
        arg(short = 'n', long = "interval", default_value_t = defaults::INTERVAL_SEC)
    )]
    #[serde(rename = "interval")]
    pub interval_sec: f64,

    /// 비트 쪼개기 수 m. 주기를 m등분한 지점마다 약한 비프가 울린다.
    #[cfg_attr(
        feature = "cli",
        arg(short = 'm', long = "subdivision", default_value_t = defaults::SUBDIVISION)
    )]
    #[serde(rename = "subdivision")]
    pub subdivision: u32,

    /// 약한 비프의 볼륨 비율 (%, 0~100).
    #[cfg_attr(
        feature = "cli",
        arg(short = 'v', long = "weak-volume", default_value_t = defaults::WEAK_VOLUME_PCT)
    )]
    #[serde(rename = "weak-volume")]
    pub weak_volume_pct: f64,

    /// 비프 구간의 길이 l (초).
    #[cfg_attr(
        feature = "cli",
        arg(short = 'l', long = "length", default_value_t = defaults::LENGTH_SEC)
    )]
    #[serde(rename = "length")]
    pub length_sec: u32,

    /// 강한 비프의 주파수 (Hz).
    #[cfg_attr(
        feature = "cli",
        arg(long = "primary-freq", default_value_t = defaults::PRIMARY_FREQ)
    )]
    #[serde(rename = "primary-freq")]
    pub primary_freq: f64,

    /// 약한 비프의 주파수 (Hz).
    #[cfg_attr(
        feature = "cli",
        arg(long = "sub-freq", default_value_t = defaults::SUB_FREQ)
    )]
    #[serde(rename = "sub-freq")]
    pub sub_freq: f64,

    /// 샘플 레이트 (Hz). 32000, 44100, 48000 중 하나.
    #[cfg_attr(
        feature = "cli",
        arg(long = "sample-rate", default_value_t = defaults::SAMPLE_RATE)
    )]
    #[serde(rename = "sample-rate")]
    pub sample_rate: u32,
}

/// 입력값 하나의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// 소수를 허용하는 숫자.
    Number,
    /// 정수.
    Integer,
    /// 정해진 값 중 하나.
    Choice,
}

/// 파라미터 하나의 명세. UI는 이 목록으로 입력 폼을 만든다.
///
/// `key` 는 CLI 옵션 이름이자 JSON 필드 이름이다(예: `weak-volume`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub unit: &'static str,
    pub kind: ParamKind,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    /// 입력 칸의 증감 단위.
    pub step: f64,
    /// `kind` 가 `Choice` 일 때 고를 수 있는 값.
    pub choices: Vec<f64>,
    /// 자주 바꾸지 않는 고급 설정인지.
    pub advanced: bool,
}

impl ParamSpec {
    fn number(key: &'static str, label: &'static str, description: &'static str) -> Self {
        Self {
            key,
            label,
            description,
            unit: "",
            kind: ParamKind::Number,
            default: 0.0,
            min: 0.0,
            max: 0.0,
            step: 1.0,
            choices: Vec::new(),
            advanced: false,
        }
    }

    fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    fn range(mut self, default: f64, (min, max): (f64, f64), step: f64) -> Self {
        self.default = default;
        self.min = min;
        self.max = max;
        self.step = step;
        self
    }

    fn integer_range(mut self, default: u32, (min, max): (u32, u32)) -> Self {
        self.kind = ParamKind::Integer;
        self.range(f64::from(default), (f64::from(min), f64::from(max)), 1.0)
    }

    fn choices(mut self, default: u32, choices: &[u32]) -> Self {
        self.kind = ParamKind::Choice;
        self.default = f64::from(default);
        self.choices = choices.iter().map(|c| f64::from(*c)).collect();
        self.min = self.choices.iter().copied().fold(f64::INFINITY, f64::min);
        self.max = self
            .choices
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        self
    }

    fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }
}

/// 모든 파라미터의 명세를 화면에 보일 순서대로 돌려준다.
pub fn param_specs() -> Vec<ParamSpec> {
    vec![
        ParamSpec::number(
            "interval",
            "기본 주기 (n)",
            "이 간격마다 강한 비프가 울립니다.",
        )
        .unit("초")
        .range(defaults::INTERVAL_SEC, limits::INTERVAL_SEC, 0.1),
        ParamSpec::number(
            "subdivision",
            "쪼개기 (m)",
            "주기를 m등분한 지점마다 약한 비프가 울립니다. 1이면 약한 비프가 없습니다.",
        )
        .unit("분할")
        .integer_range(defaults::SUBDIVISION, limits::SUBDIVISION),
        ParamSpec::number(
            "weak-volume",
            "약한 비프 볼륨",
            "강한 비프 대비 약한 비프의 크기입니다.",
        )
        .unit("%")
        .range(defaults::WEAK_VOLUME_PCT, limits::WEAK_VOLUME_PCT, 5.0),
        ParamSpec::number("length", "음원 길이 (l)", "비프 구간의 전체 길이입니다.")
            .unit("초")
            .integer_range(defaults::LENGTH_SEC, limits::LENGTH_SEC),
        ParamSpec::number(
            "primary-freq",
            "강한 비프 주파수",
            "강한 비프의 음높이입니다.",
        )
        .unit("Hz")
        .range(defaults::PRIMARY_FREQ, limits::FREQ, 1.0)
        .advanced(),
        ParamSpec::number("sub-freq", "약한 비프 주파수", "약한 비프의 음높이입니다.")
            .unit("Hz")
            .range(defaults::SUB_FREQ, limits::FREQ, 1.0)
            .advanced(),
        ParamSpec::number("sample-rate", "샘플 레이트", "음원의 초당 샘플 수입니다.")
            .unit("Hz")
            .choices(defaults::SAMPLE_RATE, &limits::SAMPLE_RATES)
            .advanced(),
    ]
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

    /// 명세의 key·기본값이 구조체(JSON 필드)와 어긋나면 UI가 엉뚱한 값을 보내게 된다.
    #[test]
    fn specs_match_struct_fields_and_defaults() {
        let json = serde_json::to_value(TrackParams::default()).unwrap();
        let fields = json.as_object().unwrap();
        let specs = param_specs();
        assert_eq!(
            specs.len(),
            fields.len(),
            "명세와 구조체의 항목 수가 다릅니다"
        );
        for spec in &specs {
            let value = fields
                .get(spec.key)
                .unwrap_or_else(|| panic!("구조체에 {} 가 없습니다", spec.key));
            assert_eq!(
                value.as_f64().unwrap(),
                spec.default,
                "{} 의 기본값",
                spec.key
            );
            assert!(
                spec.min <= spec.default && spec.default <= spec.max,
                "{}",
                spec.key
            );
        }
    }

    /// 명세의 최솟값·최댓값은 실제 검증을 통과해야 하고, 그 바깥은 거부되어야 한다.
    #[test]
    fn spec_limits_agree_with_validation() {
        for spec in param_specs() {
            // 정수 항목은 JSON 에서도 정수로 보내야 한다(UI 도 그렇게 보낸다).
            let build = |value: f64| -> TrackParams {
                let json = if spec.kind == ParamKind::Number {
                    serde_json::json!({ spec.key: value })
                } else {
                    serde_json::json!({ spec.key: value as u64 })
                };
                serde_json::from_value(json).unwrap()
            };
            if spec.kind == ParamKind::Choice {
                for choice in &spec.choices {
                    build(*choice).validate().unwrap();
                }
                continue;
            }
            // 주파수는 샘플 레이트의 절반 미만이어야 해서 최댓값 검사에서 뺀다.
            if !spec.key.ends_with("-freq") {
                build(spec.max).validate().unwrap();
                assert!(
                    build(spec.max + spec.step).validate().is_err(),
                    "{} 최댓값 초과",
                    spec.key
                );
            }
            build(spec.min).validate().unwrap();
            if spec.min - spec.step >= 0.0 {
                assert!(
                    build(spec.min - spec.step).validate().is_err(),
                    "{} 최솟값 미만",
                    spec.key
                );
            }
        }
    }

    #[test]
    fn json_uses_cli_option_names_and_rejects_unknown_keys() {
        let p: TrackParams = serde_json::from_str(r#"{"interval":1.4,"weak-volume":20}"#).unwrap();
        assert_eq!(p.interval_sec, 1.4);
        assert_eq!(p.weak_volume_pct, 20.0);
        assert_eq!(p.length_sec, defaults::LENGTH_SEC);
        assert!(serde_json::from_str::<TrackParams>(r#"{"intervl":1}"#).is_err());
    }

    /// CLI 옵션 이름이 명세의 key 와 같아야 문서·UI·CLI 가 같은 이름을 쓴다.
    #[cfg(feature = "cli")]
    #[test]
    fn cli_option_names_match_spec_keys() {
        use clap::{Args, Command};
        let command = TrackParams::augment_args(Command::new("test"));
        let mut longs: Vec<String> = command
            .get_arguments()
            .filter_map(|a| a.get_long().map(str::to_owned))
            .collect();
        let mut keys: Vec<String> = param_specs().iter().map(|s| s.key.to_owned()).collect();
        longs.sort();
        keys.sort();
        assert_eq!(longs, keys);
    }

    #[test]
    fn interval_label_drops_trailing_zero() {
        let mut p = TrackParams::default();
        assert_eq!(p.interval_label(), "1");
        p.interval_sec = 1.4;
        assert_eq!(p.interval_label(), "1.4");
    }
}
