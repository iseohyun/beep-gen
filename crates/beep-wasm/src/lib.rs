//! beep-core 의 웹(WASM) 연결부.
//!
//! 자바스크립트와 값을 주고받는 일만 한다. 소리를 만드는 계산은 전부 코어에 있다.
//! 파라미터는 JSON 문자열로 받는다. 필드 이름은 CLI 옵션 이름과 같다(예: `weak-volume`).

use beep_core::{OutputFormat, TrackInputs, TrackParams};
use wasm_bindgen::prelude::*;

fn parse_params(params_json: &str) -> Result<TrackParams, JsError> {
    serde_json::from_str(params_json).map_err(|e| JsError::new(&format!("잘못된 설정값: {e}")))
}

fn parse_format(format: &str) -> Result<OutputFormat, JsError> {
    OutputFormat::from_extension(format)
        .ok_or_else(|| JsError::new(&format!("지원하지 않는 형식: {format}")))
}

/// 화면을 그리는 데 필요한 정보를 JSON 으로 돌려준다:
/// 버전, 파라미터 명세(이름·기본값·범위), 출력 형식 목록, 안내 음성 정보.
#[wasm_bindgen]
pub fn describe() -> String {
    serde_json::to_string(&beep_core::describe()).unwrap_or_default()
}

/// 만들어진 음원.
#[wasm_bindgen]
pub struct TrackResult {
    bytes: Vec<u8>,
    file_name: String,
    music_report_json: Option<String>,
}

#[wasm_bindgen]
impl TrackResult {
    /// 완성된 파일 바이트를 꺼낸다. 큰 데이터를 두 번 복사하지 않도록 한 번만 꺼낼 수 있다.
    #[wasm_bindgen(js_name = takeBytes)]
    pub fn take_bytes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }

    /// 설정값에 맞는 기본 파일 이름.
    #[wasm_bindgen(getter, js_name = fileName)]
    pub fn file_name(&self) -> String {
        self.file_name.clone()
    }

    /// 배경 음악을 넣었을 때의 보고(JSON). 음악이 없으면 `undefined`.
    #[wasm_bindgen(getter, js_name = musicReportJson)]
    pub fn music_report_json(&self) -> Option<String> {
        self.music_report_json.clone()
    }
}

/// 안내 문구의 자리표시자({n}, {m}, {l})를 설정값으로 바꾼다.
/// 음성 합성기에 넘길 최종 문구를 만들 때 쓴다. 비어 있으면 안내 음성을 넣지 않는다는 뜻이다.
#[wasm_bindgen]
pub fn intro_text(template: &str, params_json: &str) -> Result<String, JsError> {
    Ok(beep_core::intro_text(template, &parse_params(params_json)?))
}

/// 음원을 만든다.
///
/// `music` 에 음악 파일 바이트를 주면 비프 템포에 맞춰 함께 넣는다.
/// `intro` 에 안내 음성 파일 바이트를 주면 맨 앞에 넣는다.
/// `on_progress(percent, stageKey, stageLabel)` 는 진행 상황이 바뀔 때마다 호출된다.
#[wasm_bindgen]
pub fn build_track(
    params_json: &str,
    format: &str,
    music: Option<Vec<u8>>,
    intro: Option<Vec<u8>>,
    on_progress: &js_sys::Function,
) -> Result<TrackResult, JsError> {
    let params = parse_params(params_json)?;
    let format = parse_format(format)?;
    let inputs = TrackInputs {
        music: music.as_deref(),
        intro: intro.as_deref(),
    };
    let mut last = None;
    let output = beep_core::build_track(&params, format, inputs, |progress| {
        if last != Some(progress) {
            last = Some(progress);
            // 진행 표시 실패는 음원 생성과 무관하므로 무시한다.
            let _ = on_progress.call3(
                &JsValue::NULL,
                &JsValue::from(progress.percent),
                &JsValue::from(progress.stage.key()),
                &JsValue::from(progress.stage.label()),
            );
        }
    })
    .map_err(|e| JsError::new(&e.to_string()))?;

    Ok(TrackResult {
        bytes: output.bytes,
        file_name: beep_core::default_file_name(&params, format, music.is_some()),
        music_report_json: output
            .music
            .as_ref()
            .map(|report| serde_json::to_string(report).unwrap_or_default()),
    })
}
