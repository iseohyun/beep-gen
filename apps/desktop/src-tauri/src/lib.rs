//! Beep Gen 데스크톱 앱의 Rust 쪽.
//!
//! 화면(apps/web)이 부르는 명령을 받아 공용 절차(`beep_native::job`)로 넘긴다.
//! 여기에도 소리를 만드는 계산은 없다. 값을 옮길 뿐이다.

use std::sync::Mutex;

use beep_core::{MusicReport, OutputFormat, TrackParams};
use beep_native::job::{self, IntroRequest, JobRequest};
use beep_native::tts;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeBody, Request, Response};
use tauri::{Manager, State};

/// 명령 사이에 넘겨받는 큰 데이터. 화면과는 필요한 때에만 주고받는다.
#[derive(Default)]
struct AppState {
    /// 화면에서 고른 음악 파일의 내용.
    uploaded_music: Mutex<Option<Vec<u8>>>,
    /// 마지막으로 만든 음원.
    result: Mutex<Option<Vec<u8>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // 다른 스레드가 잠근 채 멈췄더라도 데이터 자체는 그대로 쓸 수 있다.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 화면 구성 정보. 웹의 `describe()` 와 같은 내용이다.
#[tauri::command]
fn describe() -> beep_core::AppInfo {
    beep_core::describe()
}

/// 설치된 음성 목록.
#[tauri::command]
fn list_voices() -> Result<Vec<tts::Voice>, String> {
    tts::list_voices().map_err(|e| e.to_string())
}

/// 화면에서 고른 음악 파일의 내용을 받아 둔다. 본문이 비어 있으면 지운다.
#[tauri::command]
fn set_music(request: Request<'_>, state: State<'_, AppState>) -> Result<(), String> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("음악 파일의 내용이 전달되지 않았습니다.".into());
    };
    *lock(&state.uploaded_music) = (!bytes.is_empty()).then(|| bytes.clone());
    Ok(())
}

/// 배경 음악을 어디서 가져올지.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum MusicSource {
    /// `set_music` 으로 미리 보낸 파일.
    Upload,
    /// 오디오 파일의 직접 주소.
    Url { url: String },
}

#[derive(Debug, Deserialize)]
struct IntroOptions {
    template: String,
    #[serde(default, rename = "voiceId")]
    voice_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateRequest {
    params_json: String,
    format: String,
    music: Option<MusicSource>,
    intro: Option<IntroOptions>,
}

#[derive(Debug, Clone, Serialize)]
struct ProgressEvent {
    percent: u8,
    stage: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResponse {
    file_name: String,
    music_report: Option<MusicReport>,
    /// 음원은 만들었지만 사용자가 알아야 할 일(예: 안내 음성을 넣지 못함).
    notices: Vec<String>,
}

/// 음원을 만든다. 결과 바이트는 앱 안에 두고, 화면에는 요약만 돌려준다.
#[tauri::command]
async fn generate(
    app: tauri::AppHandle,
    request: GenerateRequest,
    on_progress: Channel<ProgressEvent>,
) -> Result<GenerateResponse, String> {
    // 오래 걸리는 작업이므로 화면이 멈추지 않게 별도 스레드에서 한다.
    tauri::async_runtime::spawn_blocking(move || generate_blocking(&app, request, &on_progress))
        .await
        .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

fn generate_blocking(
    app: &tauri::AppHandle,
    request: GenerateRequest,
    on_progress: &Channel<ProgressEvent>,
) -> Result<GenerateResponse, String> {
    let state = app.state::<AppState>();
    let params: TrackParams =
        serde_json::from_str(&request.params_json).map_err(|e| format!("잘못된 설정값: {e}"))?;
    let format = OutputFormat::from_extension(&request.format)
        .ok_or_else(|| format!("지원하지 않는 형식: {}", request.format))?;

    let music = match request.music {
        None => None,
        Some(MusicSource::Url { url }) => Some(job::MusicSource::Url(url)),
        Some(MusicSource::Upload) => Some(job::MusicSource::Bytes(
            lock(&state.uploaded_music)
                .clone()
                .ok_or("음악 파일이 전달되지 않았습니다.")?,
        )),
    };
    let job_request = JobRequest {
        params,
        format,
        music,
        intro: request.intro.map(|intro| IntroRequest {
            template: intro.template,
            voice: intro.voice_id,
        }),
    };

    let output = job::run(&job_request, |percent, stage| {
        // 화면이 닫히는 중이면 보내지 못할 수 있다. 음원 생성과는 무관하다.
        let _ = on_progress.send(ProgressEvent {
            percent,
            stage: stage.to_owned(),
        });
    })
    .map_err(|e| e.to_string())?;

    *lock(&state.result) = Some(output.bytes);
    Ok(GenerateResponse {
        file_name: output.file_name,
        music_report: output.music_report,
        notices: output.notices,
    })
}

/// 마지막으로 만든 음원의 내용(미리듣기용).
#[tauri::command]
fn result_bytes(state: State<'_, AppState>) -> Result<Response, String> {
    lock(&state.result)
        .clone()
        .map(Response::new)
        .ok_or_else(|| "아직 만든 음원이 없습니다.".to_owned())
}

/// 마지막으로 만든 음원을 사용자가 고른 위치에 저장한다.
#[tauri::command]
fn save_result(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = lock(&state.result);
    let bytes = guard.as_ref().ok_or("아직 만든 음원이 없습니다.")?;
    std::fs::write(&path, bytes).map_err(|e| format!("{path} 에 저장하지 못했습니다: {e}"))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            describe,
            list_voices,
            set_music,
            generate,
            result_bytes,
            save_result
        ])
        .run(tauri::generate_context!())
        .expect("앱을 시작하지 못했습니다");
}
