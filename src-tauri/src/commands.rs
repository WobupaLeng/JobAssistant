//! Tauri 命令集合。
//!
//! 单独成模块的原因：`#[tauri::command]` 会在**定义所在模块**里生成
//! `__cmd__<name>` / `__tauri_command_name_<name>` 两个宏，而
//! `generate_handler!` 又会在同一模块里 `use` 它们。两者同模块时就报
//! E0255 "the name ... is defined multiple times"。放到子模块后，
//! `generate_handler!` 在父模块里通过 `commands::xxx` 路径引用，互不冲突。

use crate::{data_dir, BackendPort};

#[tauri::command]
pub fn get_backend_port(state: tauri::State<BackendPort>) -> u16 {
    *state.0.lock().unwrap()
}

#[tauri::command]
pub fn get_data_dir() -> String {
    data_dir().to_string_lossy().to_string()
}

/// 返回崩溃日志绝对路径（`%LOCALAPPDATA%\ai-job-assistant\logs\crash.log`）。
///
/// 打包态是窗口程序，没有控制台，启动失败时用户界面可能根本没出来。
/// 给前端一个入口（以及给用户一个能自己找到的固定路径），
/// 才能把「双击没反应 / 闪退」变成一份可贴回来的日志。
#[tauri::command]
pub fn get_crash_log_path() -> String {
    crate::app_local_dir()
        .join("logs")
        .join("crash.log")
        .to_string_lossy()
        .to_string()
}
