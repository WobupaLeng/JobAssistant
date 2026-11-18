// Windows 下避免弹出控制台窗口（release 构建）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ai_job_assistant_lib::run()
}
