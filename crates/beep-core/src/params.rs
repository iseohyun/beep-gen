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
    // 비프는 한 음높이의 또렷한 소리라 같은 크기여도 음악보다 훨씬 크게 들린다.
    // 음악이 주인공이 되도록 비프를 낮춰 둔다.
    pub const BEEP_VOLUME_PCT: f64 = 30.0;
    pub const MUSIC_VOLUME_PCT: f64 = 100.0;
    pub const PRIMARY_FREQ: f64 = 880.0;
    pub const SUB_FREQ: f64 = 587.33;
    pub const SAMPLE_RATE: u32 = 44_100;
    pub const METER: u32 = 0;
    pub const TRIPLE_ACCENT: u32 = 0;
}

/// 허용 범위(양 끝 포함).
pub mod limits {
    pub const INTERVAL_SEC: (f64, f64) = (0.05, 600.0);
    pub const SUBDIVISION: (u32, u32) = (1, 64);
    pub const WEAK_VOLUME_PCT: (f64, f64) = (0.0, 100.0);
    pub const LENGTH_SEC: (u32, u32) = (1, 3600);
    /// 음량(%). 100 을 넘기면 그만큼 더 키운다(화면의 +10 dB 까지 낼 수 있는 범위).
    pub const VOLUME_PCT: (f64, f64) = (0.0, 500.0);
    /// 박자 계열: 0 자동, 2 = 2박 계열, 3 = 3박 계열.
    pub const METERS: [u32; 3] = [0, 2, 3];
    /// 3박 계열의 강약 패턴: 0 강약강, 1 강약약.
    pub const TRIPLE_ACCENTS: [u32; 2] = [0, 1];
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

    /// 비프 구간의 길이 l (초). 배경 음악이 있으면 음악 길이를 따르고 이 값은 쓰지 않는다.
    #[cfg_attr(
        feature = "cli",
        arg(short = 'l', long = "length", default_value_t = defaults::LENGTH_SEC)
    )]
    #[serde(rename = "length")]
    pub length_sec: u32,

    /// 배경 음악과 함께 넣을 때의 비프 음량 (%, 0~500). 0이면 비프 없이 음악만 나오고,
    /// 100 을 넘기면 그만큼 더 키운다. 배경 음악이 없으면 쓰지 않는다(비프는 늘 제 크기로 나온다).
    #[cfg_attr(
        feature = "cli",
        arg(long = "beep-volume", default_value_t = defaults::BEEP_VOLUME_PCT)
    )]
    #[serde(rename = "beep-volume")]
    pub beep_volume_pct: f64,

    /// 배경 음악의 음량 (%, 0~500). 100이면 곡의 가장 큰 순간이 최대 크기가 된다
    /// (작게 녹음된 곡은 그만큼 키운다). 음악과 비프를 더한 크기가 최대를 넘으면 둘을 같은
    /// 비율로 줄이므로, 100 을 넘는 값은 비프에 비해 음악을 더 키우는 뜻이다. 배경 음악이 있을 때만 쓴다.
    #[cfg_attr(
        feature = "cli",
        arg(long = "music-volume", default_value_t = defaults::MUSIC_VOLUME_PCT)
    )]
    #[serde(rename = "music-volume")]
    pub music_volume_pct: f64,

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

    /// 배경 음악의 박자 계열. 0 = 소리를 보고 정함, 2 = 2박 계열(2/4, 4/4), 3 = 3박 계열(3/4, 6/8).
    #[cfg_attr(feature = "cli", arg(long = "meter", default_value_t = defaults::METER))]
    #[serde(rename = "meter")]
    pub meter: u32,

    /// 3박 계열 곡에서 한 마디(비프 세 개)의 강약. 0 = 강약강(분할대로 강약을 놓고 마디마다
    /// 다시 시작), 1 = 강약약(마디의 첫 비프만 강하게).
    #[cfg_attr(
        feature = "cli",
        arg(long = "triple-accent", default_value_t = defaults::TRIPLE_ACCENT)
    )]
    #[serde(rename = "triple-accent")]
    pub triple_accent: u32,
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
    /// 고를 수 있는 값마다의 이름(`choices` 와 같은 순서). 비어 있으면 값을 그대로 보여준다.
    pub choice_labels: Vec<&'static str>,
    /// 자주 바꾸지 않는 고급 설정인지.
    pub advanced: bool,
    /// 이 항목이 쓰이는 경우. 쓰이지 않는 경우에는 화면에서 숨긴다.
    pub when: ParamWhen,
    /// 소리 크기(%)인 항목이면, 화면에서 dB 슬라이더로 보여줄 때의 눈금.
    pub level: Option<LevelScale>,
}

/// 소리 크기 항목을 dB 로 보여줄 때의 눈금. 값(%)과 dB 의 관계:
/// `dB = top_db + 20 × log10(값 / 100)`, 그리고 `floor_db` 이하는 0%(끔)이다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LevelScale {
    /// 값이 100% 일 때의 크기(dB).
    pub top_db: f64,
    /// 슬라이더의 맨 아래(dB). 여기까지 내리면 소리를 끈다.
    pub floor_db: f64,
    /// 슬라이더의 맨 위(dB).
    pub ceil_db: f64,
}

/// 파라미터가 쓰이는 경우.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParamWhen {
    /// 항상.
    Always,
    /// 배경 음악이 있을 때만.
    WithMusic,
    /// 배경 음악이 없을 때만.
    WithoutMusic,
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
            choice_labels: Vec::new(),
            advanced: false,
            when: ParamWhen::Always,
            level: None,
        }
    }

    /// 소리 크기 항목으로 표시한다. `top_db` 는 100% 일 때의 크기다.
    fn level(mut self, top_db: f64) -> Self {
        self.level = Some(LevelScale {
            top_db,
            floor_db: crate::levels::SLIDER_FLOOR_DB,
            ceil_db: crate::levels::SLIDER_CEIL_DB,
        });
        self
    }

    fn when(mut self, when: ParamWhen) -> Self {
        self.when = when;
        self
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

    fn labels(mut self, labels: &[&'static str]) -> Self {
        self.choice_labels = labels.to_vec();
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
            "분할 비프 볼륨",
            "주기를 쪼갠 지점에서 울리는 약한 비프의 크기입니다(강한 비프 대비).",
        )
        .unit("%")
        .range(defaults::WEAK_VOLUME_PCT, limits::WEAK_VOLUME_PCT, 5.0),
        ParamSpec::number("length", "음원 길이 (l)", "비프 구간의 전체 길이입니다.")
            .unit("초")
            .integer_range(defaults::LENGTH_SEC, limits::LENGTH_SEC)
            .when(ParamWhen::WithoutMusic),
        ParamSpec::number(
            "beep-volume",
            "비프 음량",
            "음악과 함께 나올 때의 비프 크기입니다. 0이면 비프 없이 음악만 나옵니다. 100% 를 넘기면 그만큼 더 키웁니다.",
        )
        .unit("%")
        .range(defaults::BEEP_VOLUME_PCT, limits::VOLUME_PCT, 5.0)
        .level(crate::levels::beep_top_db())
        .when(ParamWhen::WithMusic),
        ParamSpec::number(
            "music-volume",
            "음악 음량",
            "배경 음악의 크기입니다. 100% 는 곡의 가장 큰 순간이 낼 수 있는 최대 크기가 되는 값입니다. 음악과 비프를 더한 크기가 최대를 넘으면 둘을 같은 비율로 줄입니다.",
        )
        .unit("%")
        .range(defaults::MUSIC_VOLUME_PCT, limits::VOLUME_PCT, 5.0)
        .level(0.0)
        .when(ParamWhen::WithMusic),
        ParamSpec::number(
            "meter",
            "박자",
            "곡의 박자 계열입니다. 자동은 소리를 보고 정합니다. 분석이 틀렸으면 직접 고릅니다.",
        )
        .choices(defaults::METER, &limits::METERS)
        .labels(&["자동", "2박 계열 (2/4, 4/4)", "3박 계열 (3/4, 6/8)"])
        .when(ParamWhen::WithMusic),
        ParamSpec::number(
            "triple-accent",
            "3박 패턴",
            "3박 계열 곡에서 비프 세 개가 한 마디입니다. 강약강은 분할대로 강약을 놓고 마디마다 다시 시작하고, 강약약은 마디의 첫 비프만 강하게 울립니다.",
        )
        .choices(defaults::TRIPLE_ACCENT, &limits::TRIPLE_ACCENTS)
        .labels(&["강약강", "강약약"])
        .when(ParamWhen::WithMusic),
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
            beep_volume_pct: defaults::BEEP_VOLUME_PCT,
            music_volume_pct: defaults::MUSIC_VOLUME_PCT,
            primary_freq: defaults::PRIMARY_FREQ,
            sub_freq: defaults::SUB_FREQ,
            sample_rate: defaults::SAMPLE_RATE,
            meter: defaults::METER,
            triple_accent: defaults::TRIPLE_ACCENT,
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
        check_f64("beep-volume", self.beep_volume_pct, limits::VOLUME_PCT)?;
        check_f64("music-volume", self.music_volume_pct, limits::VOLUME_PCT)?;
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
        for (name, value, allowed) in [
            ("meter", self.meter, &limits::METERS[..]),
            (
                "triple-accent",
                self.triple_accent,
                &limits::TRIPLE_ACCENTS[..],
            ),
        ] {
            if !allowed.contains(&value) {
                return Err(Error::InvalidParam {
                    name,
                    reason: format!("{value} 은(는) 쓸 수 없습니다 (가능: {allowed:?})"),
                });
            }
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
