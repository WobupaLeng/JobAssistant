//! 日志页：装配 `features/logs`。
//!
//! 两套数据（运行日志 / 决策日志）都在这一页里，由 `LogPanel` 的分段控件切换。
//! 页面层不写业务规则：只做「首帧与切回本页时重拉」这一件事。

use leptos::prelude::*;

use crate::components::shell::Boot;
use crate::features::logs::{LogPanel, LogState};

/// `prev_tick` 的初值。
///
/// 用 `u32::MAX` 而不是 0：`page_tick` 的初值就是 0，若 `prev_tick` 也从 0 开始，
/// 首帧进入本页时「tick 变了吗」会判成没变，`reload_all()` 一次都不会跑，
/// 用户看到的是一张空日志表，而且没有任何报错。
const NO_TICK_YET: u32 = u32::MAX;

/// 本次 `Effect` 是否该重拉。
///
/// 判据是「`page_tick` 与上次记下的不同」，而不是「`Effect` 跑了」：
/// `Effect` 在内部读到的任何信号变化时都会重跑，用户点一下刷新按钮也会，
/// 若简单地每次都重拉，手动刷新会与切页重拉叠成两次请求。
fn should_reload_on_tick_change(prev_tick: u32, tick: u32) -> bool {
    prev_tick != tick
}

#[component]
pub fn LogsPage(page_tick: RwSignal<u32>, on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let _ = on_notice;

    let state = LogState::new();

    // 首帧 + 每次切回本页（`page_tick` 递增）都重拉两条数据。
    // 用 `StoredValue` 记住上次的 tick：`Effect` 在信号变化时也会重跑，
    // 只按「tick 变了」判断才不会把用户的手动刷新也当成一次切换。
    let prev_tick = StoredValue::new(NO_TICK_YET);
    Effect::new(move |_| {
        let tick = page_tick.get();
        if !should_reload_on_tick_change(prev_tick.get_value(), tick) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 首帧必须重拉：`page_tick` 初值是 0，记下的「上次」是 `u32::MAX`。
    /// 若把 `NO_TICK_YET` 改成 0，这一条会红——而界面现象是「日志页永远是空的」。
    #[test]
    fn first_visit_triggers_a_reload() {
        assert!(should_reload_on_tick_change(NO_TICK_YET, 0));
        assert_ne!(NO_TICK_YET, 0, "初值不能等于 page_tick 的初值，否则首帧不重拉");
    }

    /// 同一 tick 的重复触发（`Effect` 因其他信号重跑）不该再拉一次。
    #[test]
    fn same_tick_does_not_reload_twice() {
        assert!(!should_reload_on_tick_change(7, 7));
        assert!(!should_reload_on_tick_change(0, 0));
    }

    /// 切走再切回来：`page_tick` 递增，必须重拉。
    #[test]
    fn returning_to_the_page_reloads() {
        assert!(should_reload_on_tick_change(0, 1));
        assert!(should_reload_on_tick_change(1, 2));
    }

    /// `page_tick` 是 `u32`，回绕后从 0 重来：此时「变了」仍然成立，
    /// 不能因为 `tick < prev_tick` 就当成没变。
    #[test]
    fn tick_wraparound_still_counts_as_a_change() {
        assert!(should_reload_on_tick_change(u32::MAX, 0));
        assert!(should_reload_on_tick_change(5, 4));
    }
}
