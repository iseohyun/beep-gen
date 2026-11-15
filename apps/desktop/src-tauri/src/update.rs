//! 업데이트하기. 새 버전이 오는 곳은 두 가지이고, 화면의 업데이트 버튼 하나가 둘 다 처리한다.
//!
//! 1. 개발 빌드: 옆 폴더(`target/verify/release`)에 지금 실행 중인 것보다 새 실행 파일이 있으면
//!    실행 파일을 바꾼 뒤 앱을 다시 시작한다(`beep_native::dev_update`).
//! 2. 정식 릴리즈: 개발 빌드가 없으면 GitHub 릴리즈의 새 버전을 받아 설치한다(`release.rs`).
//!
//! 이 파일에는 화면에 넘기는 자료형만 둔다. 개발 빌드의 파일 다루기는
//! `beep_native::dev_update` 에, 릴리즈는 `release.rs` 에 있다.

use std::time::SystemTime;

use beep_native::dev_update;
use serde::{Deserialize, Serialize};

/// 새 버전이 온 곳.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateSource {
    /// 옆 폴더에 새로 빌드해 둔 실행 파일.
    DevBuild,
    /// GitHub 릴리즈에 올린 정식 버전.
    Release,
}

/// 찾은 새 버전.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub source: UpdateSource,
    /// 새 버전의 번호. 릴리즈일 때만 안다.
    pub version: Option<String>,
    /// 새 실행 파일의 위치. 개발 빌드일 때만 있다.
    pub path: Option<String>,
    /// 개발 빌드는 실행 파일이 만들어진 시각, 릴리즈는 올린 시각(1970년부터의 밀리초).
    pub built_at_ms: Option<u64>,
}

/// 지금 실행 중인 것보다 새로운 개발 빌드가 있으면 알려준다.
pub fn check_dev_build() -> Option<UpdateInfo> {
    let found = dev_update::check()?;
    Some(UpdateInfo {
        source: UpdateSource::DevBuild,
        version: None,
        path: Some(found.path.display().to_string()),
        built_at_ms: found
            .built_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|since| since.as_millis() as u64),
    })
}
