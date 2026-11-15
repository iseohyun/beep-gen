//! 개발 빌드로 업데이트하기.
//!
//! 앱을 켜 둔 채로 새로 빌드하면 실행 중인 파일은 덮어쓸 수 없어서, 새 빌드는 옆 폴더
//! (`target/verify/release`)에 만들어진다. 그곳에 지금 실행 중인 것보다 새 실행 파일이 있으면
//! 실행 파일을 바꾼 뒤 앱을 다시 시작한다. 인터넷은 쓰지 않는다. 설치해서 쓰는 앱 옆에는
//! 그런 폴더가 없으므로 아무 일도 하지 않는다.
//!
//! 데스크톱 앱만 쓰지만 여기에 둔다: 앱 크레이트의 테스트 실행 파일에는 창·대화상자 코드가
//! 딸려 들어가 Windows 에서 뜨지 못하므로(공용 컨트롤 v6 매니페스트가 없다), 테스트할 수 있는
//! 순수한 파일 다루기는 Tauri 에 기대지 않는 이 크레이트에 있어야 한다.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 새 빌드가 있는 곳을 직접 정하는 환경 변수(실행 파일의 전체 경로).
const UPDATE_FROM_ENV: &str = "BEEP_GEN_UPDATE_FROM";
/// 파일 시각이 이만큼은 더 새로워야 새 빌드로 본다(복사하면서 생기는 작은 차이를 무시한다).
const NEWER_BY: Duration = Duration::from_secs(2);

/// 찾은 새 개발 빌드.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevBuild {
    /// 새 실행 파일의 위치.
    pub path: PathBuf,
    /// 새 실행 파일이 만들어진 시각.
    pub built_at: SystemTime,
}

/// 실행 파일 `current` 를 대신할 새 빌드가 놓일 수 있는 곳.
fn candidates(current: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Some(path) = std::env::var_os(UPDATE_FROM_ENV) {
        found.push(PathBuf::from(path));
    }
    let (Some(folder), Some(name)) = (current.parent(), current.file_name()) else {
        return found;
    };
    // target/release 에서 실행 중이면 target/verify/release 를 본다.
    if let Some(target) = folder.parent() {
        found.push(target.join("verify").join("release").join(name));
    }
    // 거꾸로 target/verify/release 에서 실행 중이면 target/release 를 본다.
    if folder.ends_with(Path::new("verify").join("release"))
        && let Some(target) = folder.parent().and_then(Path::parent)
    {
        found.push(target.join("release").join(name));
    }
    found
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// `current` 보다 새로운 빌드 중 가장 새로운 것.
fn newest(current: &Path) -> Option<(PathBuf, SystemTime)> {
    let running = modified(current)?;
    candidates(current)
        .into_iter()
        .filter(|path| path != current)
        .filter_map(|path| modified(&path).map(|time| (path, time)))
        .filter(|(_, time)| *time > running + NEWER_BY)
        .max_by_key(|(_, time)| *time)
}

/// 지금 실행 중인 것보다 새로운 개발 빌드가 있으면 알려준다.
pub fn check() -> Option<DevBuild> {
    let current = std::env::current_exe().ok()?;
    let (path, built_at) = newest(&current)?;
    Some(DevBuild { path, built_at })
}

/// 바꾸기 전의 실행 파일을 잠시 옮겨 두는 이름.
fn old_path(current: &Path) -> PathBuf {
    current.with_extension("old.exe")
}

/// 지난번 업데이트가 남긴 예전 실행 파일을 지운다. 앱을 시작할 때 부른다.
pub fn clean_up() {
    if let Ok(current) = std::env::current_exe() {
        // 예전 프로세스가 아직 끝나지 않았으면 지워지지 않는다. 다음에 시작할 때 지운다.
        let _ = std::fs::remove_file(old_path(&current));
    }
}

/// `current` 자리의 실행 파일을 새 빌드로 바꾼다. 실행 중인 파일은 덮어쓸 수 없지만 이름은
/// 바꿀 수 있으므로, 옮겨 두고 그 자리에 새 파일을 복사한다.
fn swap(current: &Path) -> Result<(), String> {
    let (source, time) = newest(current).ok_or("새 빌드가 없습니다.")?;
    let old = old_path(current);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old).map_err(|e| format!("실행 파일을 옮기지 못했습니다: {e}"))?;
    let copied = std::fs::copy(&source, current).and_then(|_| {
        // 복사한 파일이 다시 "새 빌드보다 오래된 것" 으로 보이지 않게 시각을 맞춘다.
        std::fs::File::options()
            .write(true)
            .open(current)?
            .set_modified(time)
    });
    if let Err(e) = copied {
        // 실패하면 원래대로 돌려놓는다.
        let _ = std::fs::remove_file(current);
        let _ = std::fs::rename(&old, current);
        return Err(format!(
            "{} 을(를) 복사하지 못했습니다: {e}",
            source.display()
        ));
    }
    Ok(())
}

/// 실행 파일을 새 개발 빌드로 바꾸고 새 프로세스를 띄운다. 그 뒤 부른 쪽이 앱을 끝낸다.
pub fn apply() -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    swap(&current)?;
    std::process::Command::new(&current)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map_err(|e| format!("새 빌드를 실행하지 못했습니다: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("beep-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn write(path: &Path, content: &str, age: Duration) {
        std::fs::create_dir_all(path.parent().expect("폴더")).expect("폴더 만들기");
        std::fs::write(path, content).expect("쓰기");
        std::fs::File::options()
            .write(true)
            .open(path)
            .expect("열기")
            .set_modified(SystemTime::now() - age)
            .expect("시각");
    }

    #[test]
    fn a_newer_build_in_the_side_folder_replaces_the_running_one() {
        let target = folder("swap");
        let current = target.join("release").join("app.exe");
        let side = target.join("verify").join("release").join("app.exe");
        write(&current, "old", Duration::from_secs(600));
        write(&side, "new", Duration::from_secs(60));

        assert_eq!(newest(&current).map(|found| found.0), Some(side.clone()));
        swap(&current).expect("바꾸기");
        assert_eq!(std::fs::read_to_string(&current).expect("읽기"), "new");
        assert_eq!(
            std::fs::read_to_string(old_path(&current)).expect("읽기"),
            "old"
        );
        // 바꾼 뒤에는 더 새로운 빌드가 없다.
        assert!(newest(&current).is_none());
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn an_older_or_missing_side_build_is_ignored() {
        let target = folder("ignore");
        let current = target.join("release").join("app.exe");
        write(&current, "now", Duration::from_secs(60));
        assert!(newest(&current).is_none());
        write(
            &target.join("verify").join("release").join("app.exe"),
            "old",
            Duration::from_secs(600),
        );
        assert!(newest(&current).is_none());
        assert!(swap(&current).is_err());
        assert_eq!(std::fs::read_to_string(&current).expect("읽기"), "now");
        let _ = std::fs::remove_dir_all(&target);
    }

    #[test]
    fn a_build_running_from_the_side_folder_looks_at_the_main_one() {
        let target = folder("back");
        let current = target.join("verify").join("release").join("app.exe");
        let main = target.join("release").join("app.exe");
        write(&current, "old", Duration::from_secs(600));
        write(&main, "new", Duration::from_secs(60));
        assert_eq!(newest(&current).map(|found| found.0), Some(main));
        let _ = std::fs::remove_dir_all(&target);
    }
}
