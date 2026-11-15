//! 정식 릴리즈로 업데이트하기.
//!
//! GitHub 릴리즈의 `latest.json` 을 보고, 지금보다 높은 버전이 있으면 설치 파일을 받아 서명을
//! 확인한 뒤 설치하고 다시 시작한다(`tauri-plugin-updater`). 주소와 공개 키는 `tauri.conf.json`
//! 의 `plugins.updater` 에 있다. 확인만 스스로 하고, 받아서 설치하는 것은 사용자가 버튼을
//! 눌렀을 때만 한다. 개발 빌드 쪽은 `update.rs` 에 있다.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri_plugin_updater::{Update, UpdaterExt};

use crate::update::{UpdateInfo, UpdateSource};

/// 릴리즈가 있는지 다시 물어보는 간격. 화면은 훨씬 자주 묻지만 그 사이에는 지난 답을 돌려준다.
const RELEASE_CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// 확인하지 못했을 때(인터넷이 끊겼을 때 등) 다시 해 보기까지의 간격.
const RELEASE_RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// 마지막으로 릴리즈를 확인한 결과. 앱이 켜져 있는 동안만 기억한다.
#[derive(Default)]
pub struct ReleaseState(Mutex<Option<ReleaseCheck>>);

struct ReleaseCheck {
    at: Instant,
    found: Result<Option<Update>, String>,
}

impl ReleaseCheck {
    fn is_fresh(&self) -> bool {
        let keep = if self.found.is_ok() {
            RELEASE_CHECK_EVERY
        } else {
            RELEASE_RETRY_AFTER
        };
        self.at.elapsed() < keep
    }

    fn info(&self) -> Result<Option<UpdateInfo>, String> {
        let found = self.found.as_ref().map_err(Clone::clone)?;
        Ok(found.as_ref().map(|update| UpdateInfo {
            source: UpdateSource::Release,
            version: Some(update.version.clone()),
            path: None,
            built_at_ms: update
                .date
                .and_then(|date| u64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok()),
        }))
    }
}

/// GitHub 릴리즈에 지금보다 높은 버전이 있으면 알려준다. 내려받지는 않는다.
/// 확인한 지 얼마 안 됐으면 다시 묻지 않고 지난 답을 돌려준다.
pub async fn check_release(
    app: &tauri::AppHandle,
    state: &ReleaseState,
) -> Result<Option<UpdateInfo>, String> {
    if let Some(checked) = crate::lock(&state.0).as_ref()
        && checked.is_fresh()
    {
        return checked.info();
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    }
    .map_err(|e| format!("새 버전이 있는지 확인하지 못했습니다: {e}"));
    let checked = ReleaseCheck {
        at: Instant::now(),
        found,
    };
    let info = checked.info();
    *crate::lock(&state.0) = Some(checked);
    info
}

/// 확인해 둔 릴리즈를 받아 서명을 확인하고 설치한다. Windows 에서는 설치 프로그램을 띄운 뒤
/// 이 함수 안에서 앱이 끝나고, 설치가 끝나면 설치 프로그램이 앱을 다시 띄운다.
pub async fn apply_release(state: &ReleaseState) -> Result<(), String> {
    let update = crate::lock(&state.0)
        .as_ref()
        .and_then(|checked| checked.found.as_ref().ok().cloned().flatten())
        .ok_or("새 버전이 없습니다.")?;
    let installed = update.download_and_install(|_, _| {}, || {}).await;
    if let Err(e) = installed {
        // 릴리즈가 그 사이에 바뀌었을 수 있으니 다음에는 처음부터 다시 확인한다.
        *crate::lock(&state.0) = None;
        return Err(format!(
            "{} 버전을 설치하지 못했습니다: {e}",
            update.version
        ));
    }
    Ok(())
}
