// 릴리즈 빌드에서는 앱과 함께 콘솔 창이 뜨지 않게 한다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    beep_desktop_lib::run()
}
