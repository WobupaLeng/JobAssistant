// 桌面壳入口：只做转发，逻辑在 lib.rs。
//
// Windows 发布构建下隐藏控制台窗口，避免用户看到黑框。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    jobassistant_app_lib::run()
}
