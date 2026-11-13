//! beep-core 의 웹(WASM) 연결부.
//!
//! 자바스크립트와 값을 주고받는 일만 한다. 소리를 만드는 계산은 전부 코어에 있다.
//! 파라미터는 JSON 문자열로 받는다. 필드 이름은 CLI 옵션 이름과 같다(예: `weak-volume`).

use beep_core::{OutputFormat, TrackParams};
use wasm_bindgen::prelude::*;

fn parse_params(params_json: &str) -> Result<TrackParams, JsError> {
    serde_json::from_str(params_json).map_err(|e| JsError::new(&format!("잘못된 설정값: {e}")))
}

fn parse_format(format: &str) -> Result<OutputFormat, JsError> {
    OutputFormat::from_extension(format)
        .ok_or_else(|| JsError::new(&format!("지원하지 않는 형식: {format}")))
}

/// 화면을 그리는 데 필요한 정보를 JSON 으로 돌려준다:
/// 버전, 파라미터 명세(이름·기본값·범위), 출력 형식 목록.
#[wasm_bindgen]
pub fn describe() -> String {
    let formats: Vec<serde_json::Value> = OutputFormat::ALL
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.extension(),
                "label": f.label(),
                "mime": f.mime_type(),
            })
        })
        .collect();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "params": beep_core::param_specs(),
        "formats": formats,
    })
    .to_string()
}

/// 비프 음원을 만들어 파일 바이트로 돌려준다.
///
/// `on_progress` 는 백분율(0~100)이 바뀔 때마다 그 값을 인자로 호출된다.
#[wasm_bindgen]
pub fn build_track(
    params_json: &str,
    format: &str,
    on_progress: &js_sys::Function,
) -> Result<Vec<u8>, JsError> {
    let params = parse_params(params_json)?;
    let format = parse_format(format)?;
    let mut last_percent = None;
    beep_core::build_track(&params, format, |progress| {
        let percent = progress.percent();
        if last_percent != Some(percent) {
            last_percent = Some(percent);
            // 진행 표시 실패는 음원 생성과 무관하므로 무시한다.
            let _ = on_progress.call1(&JsValue::NULL, &JsValue::from(percent));
        }
    })
    .map_err(|e| JsError::new(&e.to_string()))
}

/// 설정값에 맞는 기본 파일 이름.
#[wasm_bindgen]
pub fn default_file_name(params_json: &str, format: &str) -> Result<String, JsError> {
    Ok(beep_core::default_file_name(
        &parse_params(params_json)?,
        parse_format(format)?,
    ))
}
