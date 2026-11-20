//! 日志页：装配 `features/logs`。
//!
//! 两套数据（运行日志 / 决策日志）都在这一页里，由 `LogPanel` 的分段控件切换。
//! 页面层不写业务规则：只做「首帧与切回本页时重拉」这一件事。

use leptos::prelude::*;

use crate::components::shell::Boot;
use crate::features::logs::{LogPanel, LogState};

#[component]
pub fn LogsPage(page_tick: RwSignal<u32>, on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let _ = on_notice;

    let state = LogState::new();

    // 首帧 + 每次切回本页（`page_tick` 递增）都重拉两条数据。
    // 用 `StoredValue` 记住上次的 tick：`Effect` 在信号变化时也会重跑，
    // 只按「tick 变了」判断才不会把用户的手动刷新也当成一次切换。
    let prev_tick = StoredValue::new(u32::MAX);
    Effect::new(move |_| {
        let tick = page_tick.get();
        if prev_tick.get_value() == tick {
            return;
        }
        prev_tick.set_value(tick);
        state.reload_all();
    });

    // 页级错误（例如 IPC 断连）走全局 error bar，行内错误留在各自视图里，
    // 两者不互相盖住。
    let _ = boot;

    view! { <LogPanel state=state /> }
}
