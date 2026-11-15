//! Beep Gen 데스크톱 앱의 Rust 쪽.
//!
//! 화면(apps/web)이 부르는 명령을 받아 공용 절차(`beep_native::job`)로 넘긴다.
//! 여기에도 소리를 만드는 계산은 없다. 값을 옮길 뿐이다.

mod update;
mod window_state;

use std::path::PathBuf;
use std::sync::Mutex;

use beep_core::{Excerpt, MusicReport, OutputFormat, SongInfo, TrackParams};
use beep_native::job::{self, IntroRequest, JobRequest};
use beep_native::song::{self, Identified};
use beep_native::tags::{self, SongTags};
use beep_native::{fetch, tts};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeBody, Request, Response};
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

/// 명령 사이에 넘겨받는 큰 데이터. 화면과는 필요한 때에만 주고받는다.
#[derive(Default)]
struct AppState {
    /// 화면에서 고른 음악 파일의 내용.
    uploaded_music: Mutex<Option<Vec<u8>>>,
    /// 그 파일의 위치. 파일 고르기 창으로 골랐을 때만 안다(곡 정보를 원본에 저장할 때 쓴다).
    music_path: Mutex<Option<PathBuf>>,
    /// 마지막으로 만든 음원.
    result: Mutex<Option<Vec<u8>>>,
    /// 마지막으로 만든 미리듣기. 완성한 음원과 따로 둔다(미리듣기가 저장할 음원을 덮지 않게).
    preview: Mutex<Option<Vec<u8>>>,
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
    // 화면이 내용만 보내 준 파일은 어디에 있는지 알 수 없다.
    *lock(&state.music_path) = None;
    Ok(())
}

/// 고른 음악 파일.
#[derive(Debug, Serialize)]
struct PickedMusic {
    name: String,
    path: String,
}

/// 파일 고르기 창을 띄워 음악 파일을 고르게 한다. 취소하면 `None`.
/// 이렇게 고른 파일은 위치를 알기 때문에 곡 정보를 원본에 저장할 수 있다.
#[tauri::command]
async fn pick_music(app: tauri::AppHandle) -> Result<Option<PickedMusic>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("음악", &["mp3", "wav", "flac", "ogg", "m4a"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|e| format!("고른 파일의 위치를 알 수 없습니다: {e}"))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("{} 을(를) 읽지 못했습니다: {e}", path.display()))?;
        let state = app.state::<AppState>();
        *lock(&state.uploaded_music) = Some(bytes);
        *lock(&state.music_path) = Some(path.clone());
        Ok(Some(PickedMusic {
            name: file_name_of(&path),
            path: path.display().to_string(),
        }))
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 가수·곡 추정에, 음악 파일에 적혀 있던 나머지 정보를 더한 것.
#[derive(Debug, Serialize)]
struct IdentifyResponse {
    #[serde(flatten)]
    identified: Identified,
    composer: Option<String>,
    source_url: Option<String>,
    ai_generated: Option<bool>,
    /// 지난번에 분석해 적어 둔 박자 계열과 템포.
    meter: Option<u32>,
    bpm: Option<f64>,
}

/// 지금 고른 음악 파일(원본)에 곡 정보를 적고 이름을 "가수 - 곡" 으로 바꾼다.
#[tauri::command]
async fn save_song_info(
    app: tauri::AppHandle,
    info: SongTags,
    rename: bool,
) -> Result<PickedMusic, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let path = lock(&state.music_path).clone().ok_or(
            "원본 파일의 위치를 몰라 저장할 수 없습니다. \"음악 파일 고르기\" 로 고른 파일만 저장할 수 있습니다.",
        )?;
        let saved = tags::save(&path, &info, rename).map_err(|e| e.to_string())?;
        // 파일이 바뀌었으므로 내용과 위치를 새로 읽어 둔다.
        let bytes = std::fs::read(&saved.path)
            .map_err(|e| format!("{} 을(를) 읽지 못했습니다: {e}", saved.path.display()))?;
        *lock(&state.uploaded_music) = Some(bytes);
        *lock(&state.music_path) = Some(saved.path.clone());
        Ok(PickedMusic {
            name: file_name_of(&saved.path),
            path: saved.path.display().to_string(),
        })
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

/// 배경 음악을 어디서 가져올지.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum MusicSource {
    /// `set_music` 으로 미리 보낸 파일. `name` 은 그 파일의 원래 이름이다.
    Upload {
        #[serde(default)]
        name: Option<String>,
    },
    /// 오디오 파일의 직접 주소.
    Url { url: String },
}

/// 화면에서 고른 음악의 소리 크기를 재서 음량 안내를 돌려준다.
#[tauri::command]
async fn music_levels(app: tauri::AppHandle) -> Result<beep_core::LevelGuide, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let music = lock(&state.uploaded_music)
            .clone()
            .ok_or("음악 파일이 전달되지 않았습니다.")?;
        beep_core::level_guide(&music).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

/// 배경 음악의 가수와 곡 제목을 알아낸다: 파일의 태그 → 파일 이름 → 로컬 AI.
/// 알아내지 못하면 지어내지 않고 빈 값을 돌려준다.
#[tauri::command]
async fn identify_music(
    app: tauri::AppHandle,
    source: MusicSource,
) -> Result<IdentifyResponse, String> {
    // 로컬 AI 가 답하는 데 시간이 걸릴 수 있으므로 별도 스레드에서 한다.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (identified, written) = match source {
            MusicSource::Upload { name } => {
                let music = lock(&state.uploaded_music).clone();
                // 파일에 적어 둔 나머지 정보(작곡자·링크·AI 활용)도 함께 읽는다.
                let written = lock(&state.music_path)
                    .clone()
                    .and_then(|path| tags::read(&path).ok())
                    .unwrap_or_default();
                (
                    song::identify(music.as_deref(), name.as_deref(), true),
                    written,
                )
            }
            // 주소는 아직 내려받기 전이므로 이름만 보고 찾는다.
            MusicSource::Url { url } => (
                song::identify(None, fetch::file_name_of(&url).as_deref(), true),
                SongTags::default(),
            ),
        };
        IdentifyResponse {
            identified,
            composer: written.composer,
            source_url: written.source_url,
            ai_generated: written.ai_generated,
            meter: written.meter,
            bpm: written.bpm,
        }
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))
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
    /// 화면에서 사용자가 확인한 가수와 곡.
    #[serde(default)]
    song: SongInfo,
    /// 미리듣기: 전체 대신 이 구간만 만든다.
    #[serde(default)]
    preview: Option<Excerpt>,
    /// 결과 파일의 이름 형식.
    #[serde(default)]
    name_format: Option<String>,
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

    let preview = request.preview;
    let mut music_name = None;
    let music = match request.music {
        None => None,
        Some(MusicSource::Url { url }) => Some(job::MusicSource::Url(url)),
        Some(MusicSource::Upload { name }) => {
            music_name = name;
            Some(job::MusicSource::Bytes(
                lock(&state.uploaded_music)
                    .clone()
                    .ok_or("음악 파일이 전달되지 않았습니다.")?,
            ))
        }
    };
    let job_request = JobRequest {
        music,
        intro: request.intro.map(|intro| IntroRequest {
            template: intro.template,
            voice: intro.voice_id,
        }),
        // 가수와 곡은 화면에서 이미 찾아 사용자에게 보여줬다. 여기서 다시 찾지 않는다.
        song: request.song,
        music_name,
        preview,
        name_format: request.name_format,
        ..JobRequest::new(params, format)
    };

    let output = job::run(&job_request, |percent, stage| {
        // 화면이 닫히는 중이면 보내지 못할 수 있다. 음원 생성과는 무관하다.
        let _ = on_progress.send(ProgressEvent {
            percent,
            stage: stage.to_owned(),
        });
    })
    .map_err(|e| e.to_string())?;

    let slot = if preview.is_some() {
        &state.preview
    } else {
        &state.result
    };
    *lock(slot) = Some(output.bytes);
    Ok(GenerateResponse {
        file_name: output.file_name,
        music_report: output.music_report,
        notices: output.notices,
    })
}

/// 마지막으로 만든 음원(또는 `preview` 가 참이면 미리듣기)의 내용. 화면에서 재생하는 데 쓴다.
#[tauri::command]
fn result_bytes(preview: bool, state: State<'_, AppState>) -> Result<Response, String> {
    let slot = if preview {
        &state.preview
    } else {
        &state.result
    };
    lock(slot)
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

/// 창의 제목을 바꾼다. 화면이 버전과 빌드 번호를 제목표시줄에 보여주는 데 쓴다.
#[tauri::command]
fn set_title(title: String, window: tauri::WebviewWindow) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

/// 결과를 저장할 곳.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SaveWhere {
    /// 배경 음악 파일이 있는 폴더 아래의 `output` 폴더.
    MusicOutput,
    /// 배경 음악 파일이 있는 폴더.
    MusicFolder,
    Downloads,
    MyMusic,
    /// `folder` 로 준 폴더.
    Folder,
}

#[derive(Debug, Deserialize)]
struct SaveTarget {
    #[serde(rename = "where")]
    place: SaveWhere,
    #[serde(default)]
    folder: Option<String>,
}

/// 배경 음악 파일이 있는 폴더. 파일 고르기 창으로 고른 음악이 아니면 모른다.
fn music_folder(state: &AppState) -> Option<PathBuf> {
    lock(&state.music_path)
        .as_ref()
        .and_then(|path| path.parent().map(PathBuf::from))
}

/// 저장할 폴더를 정한다. 배경 음악의 위치를 모르면 다운로드 폴더를 쓴다.
fn resolve_folder(app: &tauri::AppHandle, target: &SaveTarget) -> Result<PathBuf, String> {
    let state = app.state::<AppState>();
    let downloads = || {
        app.path()
            .download_dir()
            .map_err(|e| format!("다운로드 폴더를 찾지 못했습니다: {e}"))
    };
    match target.place {
        SaveWhere::MusicOutput => match music_folder(&state) {
            Some(folder) => Ok(folder.join("output")),
            None => downloads(),
        },
        SaveWhere::MusicFolder => match music_folder(&state) {
            Some(folder) => Ok(folder),
            None => downloads(),
        },
        SaveWhere::Downloads => downloads(),
        SaveWhere::MyMusic => app
            .path()
            .audio_dir()
            .map_err(|e| format!("내 음악 폴더를 찾지 못했습니다: {e}")),
        SaveWhere::Folder => target
            .folder
            .as_deref()
            .filter(|folder| !folder.trim().is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| "저장할 폴더를 고르지 않았습니다.".to_owned()),
    }
}

/// 폴더 고르기 창을 띄운다. 배경 음악 파일이 있는 폴더에서 시작한다. 취소하면 `None`.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if let Some(folder) = music_folder(&app.state::<AppState>()) {
            dialog = dialog.set_directory(folder);
        }
        let Some(picked) = dialog.blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|e| format!("고른 폴더의 위치를 알 수 없습니다: {e}"))?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

/// 마지막으로 만든 음원을 정해진 폴더에 `file_name` 으로 저장하고, 저장한 위치를 돌려준다.
/// 같은 이름의 파일이 있으면 덮어쓸지 묻고, 사용자가 거절하면 저장하지 않고 `None` 을 돌려준다.
#[tauri::command]
async fn save_result_to(
    app: tauri::AppHandle,
    target: SaveTarget,
    file_name: String,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // 이름에 폴더 구분자가 섞여 다른 곳에 쓰이는 일을 막는다.
        let name = std::path::Path::new(&file_name);
        if file_name.trim().is_empty() || name.file_name() != Some(name.as_os_str()) {
            return Err(format!("파일 이름이 올바르지 않습니다: {file_name}"));
        }
        let folder = resolve_folder(&app, &target)?;
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("{} 폴더를 만들지 못했습니다: {e}", folder.display()))?;
        let path = folder.join(name);
        if path.exists() {
            let overwrite = app
                .dialog()
                .message(format!(
                    "같은 이름의 파일이 이미 있습니다.\n{}\n\n덮어쓸까요?",
                    path.display()
                ))
                .title("Beep Gen")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "덮어쓰기".into(),
                    "취소".into(),
                ))
                .blocking_show();
            if !overwrite {
                return Ok(None);
            }
        }
        let state = app.state::<AppState>();
        let guard = lock(&state.result);
        let bytes = guard.as_ref().ok_or("아직 만든 음원이 없습니다.")?;
        std::fs::write(&path, bytes)
            .map_err(|e| format!("{} 에 저장하지 못했습니다: {e}", path.display()))?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| format!("작업이 중단되었습니다: {e}"))?
}

/// 파일 탐색기에서 `path` 가 있는 폴더를 열고 그 파일을 골라 보여준다.
#[tauri::command]
fn show_in_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("{} 을(를) 찾지 못했습니다.", path.display()));
    }
    let failed = |e: std::io::Error| format!("폴더를 열지 못했습니다: {e}");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 탐색기는 "/select,경로" 를 한 덩어리로 받아야 한다(경로에 쉼표나 공백이 있어도 되도록 따옴표로 감싼다).
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map_err(failed)?;
    }
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(failed)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(&path))
        .spawn()
        .map_err(failed)?;
    Ok(())
}

/// 지금 실행 중인 것보다 새로운 개발 빌드가 있는지.
#[tauri::command]
fn check_update() -> Option<update::UpdateInfo> {
    update::check()
}

/// 새 개발 빌드로 바꾸고 앱을 다시 시작한다.
#[tauri::command]
fn apply_update(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    // 새로 뜨는 앱이 같은 자리(같은 가상 데스크톱)에서 열리도록 먼저 적어 둔다.
    window_state::save(&window);
    update::apply()?;
    app.exit(0);
    Ok(())
}

pub fn run() {
    update::clean_up();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window_state::restore(&window);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 창이 앞으로 나오거나 뒤로 갈 때와 닫힐 때 자리를 적어 둔다
            // (다른 가상 데스크톱으로 옮기면 그때도 앞뒤가 바뀐다).
            if matches!(
                event,
                WindowEvent::Focused(_) | WindowEvent::CloseRequested { .. }
            ) && let Some(window) = window.app_handle().get_webview_window(window.label())
            {
                window_state::save(&window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            describe,
            list_voices,
            set_music,
            pick_music,
            save_song_info,
            identify_music,
            music_levels,
            generate,
            result_bytes,
            save_result,
            set_title,
            pick_folder,
            save_result_to,
            show_in_folder,
            check_update,
            apply_update
        ])
        .run(tauri::generate_context!())
        .expect("앱을 시작하지 못했습니다");
}
