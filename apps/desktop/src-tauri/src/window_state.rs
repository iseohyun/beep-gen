//! 창이 마지막으로 있던 자리를 기억해 두고, 다음에 시작할 때 그 자리에서 연다.
//!
//! 기억하는 것: 창의 위치와 크기, 그리고 Windows 의 가상 데스크톱(Win+Tab 으로 바꾸는
//! 데스크톱 1, 2, 3 …). 가상 데스크톱을 기억하지 않으면 앱은 늘 지금 보고 있는 데스크톱에서 열린다.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// 기억한 내용을 둘 폴더를 직접 정하는 환경 변수(시험할 때 사용자의 설정을 건드리지 않으려고 쓴다).
const STATE_DIR_ENV: &str = "BEEP_GEN_STATE_DIR";
const FILE_NAME: &str = "window-state.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct WindowState {
    /// 창의 왼쪽 위 모서리(화면 픽셀).
    x: i32,
    y: i32,
    /// 창 안쪽의 크기(화면 픽셀). 0 이면 아직 기억한 것이 없다.
    width: u32,
    height: u32,
    /// 가상 데스크톱의 식별자(16진수 32자리). Windows 에서만 쓴다.
    desktop: Option<String>,
}

fn state_file(window: &WebviewWindow) -> Option<PathBuf> {
    let folder = match std::env::var_os(STATE_DIR_ENV) {
        Some(folder) => PathBuf::from(folder),
        None => window.app_handle().path().app_local_data_dir().ok()?,
    };
    Some(folder.join(FILE_NAME))
}

fn load(window: &WebviewWindow) -> Option<WindowState> {
    let text = std::fs::read_to_string(state_file(window)?).ok()?;
    serde_json::from_str(&text).ok()
}

/// 창이 지금 있는 자리를 적어 둔다. 실패해도 앱을 쓰는 데는 지장이 없으므로 조용히 넘어간다.
pub fn save(window: &WebviewWindow) {
    let Some(path) = state_file(window) else {
        return;
    };
    let mut state = load(window).unwrap_or_default();
    // 최소화하거나 최대화한 동안의 위치·크기는 원래 창의 것이 아니므로 적지 않는다.
    let plain = !window.is_minimized().unwrap_or(false) && !window.is_maximized().unwrap_or(false);
    if plain
        && let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size())
        && size.width > 0
        && size.height > 0
    {
        (state.x, state.y) = (position.x, position.y);
        (state.width, state.height) = (size.width, size.height);
    }
    if let Some(desktop) = desktop::current(window) {
        state.desktop = Some(desktop);
    }
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(text) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(path, text);
    }
}

/// 기억해 둔 자리로 창을 옮긴다. 앱을 시작할 때 한 번 부른다.
pub fn restore(window: &WebviewWindow) {
    let Some(state) = load(window) else {
        return;
    };
    if state.width > 0 && state.height > 0 && on_some_monitor(window, &state) {
        let _ = window.set_size(PhysicalSize::new(state.width, state.height));
        let _ = window.set_position(PhysicalPosition::new(state.x, state.y));
    }
    if let Some(desktop) = &state.desktop {
        desktop::move_to(window, desktop);
    }
}

/// 기억한 자리가 지금 연결된 모니터 안에 있는지(모니터를 뗀 뒤 창이 화면 밖에 열리지 않게).
fn on_some_monitor(window: &WebviewWindow, state: &WindowState) -> bool {
    // 제목표시줄 가운데쯤이 보이면 창을 잡아 옮길 수 있다.
    let (x, y) = (state.x + state.width as i32 / 2, state.y + 16);
    window.available_monitors().is_ok_and(|monitors| {
        monitors.iter().any(|monitor| {
            let (origin, size) = (monitor.position(), monitor.size());
            (origin.x..origin.x + size.width as i32).contains(&x)
                && (origin.y..origin.y + size.height as i32).contains(&y)
        })
    })
}

#[cfg(windows)]
mod desktop {
    use std::time::Duration;

    use tauri::WebviewWindow;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
    use windows::core::GUID;

    /// 창이 막 만들어졌을 때는 Windows 가 아직 그 창을 데스크톱에 올리지 않았을 수 있어서 다시 해 본다.
    const MOVE_TRIES: usize = 20;
    const MOVE_WAIT: Duration = Duration::from_millis(100);

    fn manager() -> Option<IVirtualDesktopManager> {
        // SAFETY: 표준 COM 객체를 만드는 호출이다. COM 은 부르는 스레드에서 이미 준비되어 있다.
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok() }
    }

    fn handle(window: &WebviewWindow) -> Option<isize> {
        window.hwnd().ok().map(|hwnd| hwnd.0 as isize)
    }

    /// 창이 지금 있는 가상 데스크톱.
    pub fn current(window: &WebviewWindow) -> Option<String> {
        let hwnd = HWND(handle(window)? as _);
        // SAFETY: 살아 있는 우리 창의 핸들을 넘긴다.
        let id = unsafe { manager()?.GetWindowDesktopId(hwnd).ok()? };
        // 창이 아직 어느 데스크톱에도 올라가지 않았으면 0 이 온다.
        (id != GUID::zeroed()).then(|| format!("{:032x}", id.to_u128()))
    }

    /// 창을 그 가상 데스크톱으로 옮긴다. 그 데스크톱이 없어졌으면 지금 자리에 둔다.
    pub fn move_to(window: &WebviewWindow, desktop: &str) {
        let (Some(hwnd), Ok(id)) = (handle(window), u128::from_str_radix(desktop, 16)) else {
            return;
        };
        let id = GUID::from_u128(id);
        // 창이 뜨는 것을 막지 않도록 다른 스레드에서 기다리며 옮긴다.
        std::thread::spawn(move || {
            // SAFETY: 이 스레드에서 COM 을 쓰기 위한 준비이고, 끝날 때 짝을 맞춰 정리한다.
            let ready = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
            if let Some(manager) = manager() {
                let hwnd = HWND(hwnd as _);
                for _ in 0..MOVE_TRIES {
                    // SAFETY: 우리 프로세스의 창 핸들과, 값으로 가진 식별자를 넘긴다.
                    let here = unsafe { manager.GetWindowDesktopId(hwnd) };
                    if here.as_ref().is_ok_and(|here| *here == id) {
                        break;
                    }
                    // SAFETY: 위와 같다.
                    if unsafe { manager.MoveWindowToDesktop(hwnd, &id) }.is_ok() {
                        break;
                    }
                    std::thread::sleep(MOVE_WAIT);
                }
            }
            if ready {
                // SAFETY: 위의 CoInitializeEx 와 짝이다.
                unsafe { CoUninitialize() };
            }
        });
    }
}

#[cfg(not(windows))]
mod desktop {
    use tauri::WebviewWindow;

    pub fn current(_window: &WebviewWindow) -> Option<String> {
        None
    }

    pub fn move_to(_window: &WebviewWindow, _desktop: &str) {}
}
