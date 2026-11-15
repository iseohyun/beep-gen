//! 화면을 그리는 데 필요한 정보.
//!
//! 웹(WASM)과 데스크톱 앱이 같은 정보를 받아 같은 화면을 그리도록 한 곳에서 만든다.

use serde::Serialize;

use crate::Excerpt;
use crate::encode::OutputFormat;
use crate::intro::{DEFAULT_INTRO_TEMPLATE, INTRO_TEMPLATES};
use crate::params::{ParamSpec, param_specs};

/// 출력 형식 하나.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormatInfo {
    /// 파일 확장자이자 식별자 (예: "mp3").
    pub id: &'static str,
    pub label: &'static str,
    pub mime: &'static str,
}

/// 시작 안내 음성에 대한 정보.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntroInfo {
    /// 기본 안내 문구.
    pub default_template: &'static str,
    /// 고를 수 있는 안내 문구. 첫 번째가 기본값이다.
    pub templates: Vec<&'static str>,
    /// 문구에 쓸 수 있는 자리표시자 설명.
    pub hint: &'static str,
}

/// 화면 구성 정보.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub params: Vec<ParamSpec>,
    pub formats: Vec<FormatInfo>,
    pub intro: IntroInfo,
    /// 미리듣기의 기본 길이와 시작 위치.
    pub preview: Excerpt,
    /// 고를 수 있는 결과 파일 이름 형식. 첫 번째가 기본값이다.
    pub name_formats: Vec<&'static str>,
    /// 이름 형식에 쓸 수 있는 자리표시자 설명.
    pub name_format_hint: &'static str,
}

/// 화면 구성 정보를 만든다.
pub fn describe() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        params: param_specs(),
        formats: OutputFormat::ALL
            .iter()
            .map(|format| FormatInfo {
                id: format.extension(),
                label: format.label(),
                mime: format.mime_type(),
            })
            .collect(),
        intro: IntroInfo {
            default_template: DEFAULT_INTRO_TEMPLATE,
            templates: INTRO_TEMPLATES.to_vec(),
            hint: "{n} 은 주기(초), {m} 은 분할 수로 바뀝니다. 비워 두면 안내 음성을 넣지 않습니다.",
        },
        preview: Excerpt::default(),
        name_formats: crate::naming::NAME_FORMATS.to_vec(),
        name_format_hint: crate::naming::NAME_FORMAT_HINT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_every_format_and_param() {
        let info = describe();
        assert_eq!(info.formats.len(), OutputFormat::ALL.len());
        assert_eq!(info.formats[0].id, "mp3");
        assert_eq!(info.params.len(), param_specs().len());
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["intro"]["default_template"], DEFAULT_INTRO_TEMPLATE);
        assert_eq!(json["params"][0]["key"], "interval");
        assert_eq!(json["preview"]["seconds"], 10.0);
        assert_eq!(json["preview"]["from_percent"], 30.0);
    }
}
