//! 运行日志视图：级别筛选 + 关键字搜索 + 倒序日志流 + 清空（二次确认）。
//!
//! 数据源 `list_logs(since)`（`commands_logs.rs` 的 2000 条环形缓冲），
//! **不是** HTTP，也不是旧版 `web/src/App.tsx:314 pushLog` 那样的内存数组——
//! 旧版只在本次会话里累积 JS 数组、重启即丢；命令层的环形缓冲至少能让
//! 「刚才发生了什么」跨页面存活。
//!
//! 倒序：最新在最上面（`merge_logs` 已按 `ts` 降序排好），不再做「自动滚到底」。
//! 用户要的「倒序」就是让最新一条永远停在顶部，不必滚。

use leptos::prelude::*;

use super::state::{LogState, RunRow};
use crate::components::widgets::{EmptyState, SkeletonRows};
use crate::lib_ui::logfmt::{fmt_log_time, LogLevel};

#[component]
pub fn RuntimeView(state: LogState) -> impl IntoView {
    let total = Signal::derive(move || state.run_rows.get().len());
    let shown = Signal::derive(move || state.visible_runs().len());
    let auto = state.run_auto;
    // 清空二次确认：点「清空」只切到确认态，点「确认清空」才真调命令。
    let confirming = state.confirming_clear;

    view! {
        <div class="log-view" data-testid="logs-view-runtime">
            <div class="row wrap log-tools">
                <div class="tabs" role="group" aria-label="按级别筛选">
                    {LogLevel::all()
                        .iter()
                        .map(|lv| {
                            let lv = *lv;
                            view! {
                                <button
                                    type="button"
                                    class=move || {
                                        if state.levels.get().contains(&lv) { "tab is-active" } else { "tab" }
                                    }
                                    aria-pressed=move || {
                                        if state.levels.get().contains(&lv) { "true" } else { "false" }
                                    }
                                    data-testid=format!("logs-level-{}", lv.as_str())
                                    on:click=move |_| {
                                        state.levels.update(|v| toggle_level(v, lv));
                                    }
                                >
                                    {lv.cn()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <input
                    class="input log-search"
                    type="search"
                    data-testid="logs-search"
                    placeholder="搜索日志内容"
                    prop:value=move || state.run_keyword.get()
                    on:input=move |ev| state.run_keyword.set(event_target_value(&ev))
                />
                <span class="spacer"></span>
                <label class="log-auto" title="每 2 秒增量拉取；关闭开关或离开本页即停止">
                    <input
                        type="checkbox"
                        data-testid="logs-auto-toggle"
                        prop:checked=move || auto.get()
                        on:change=move |ev| auto.set(event_target_checked(&ev))
                    />
                    <span>"自动刷新"</span>
                </label>
                <span class="log-count" data-testid="logs-run-count">
                    {move || {
                        let (s, t) = (shown.get(), total.get());
                        if s == t { format!("{t} 条") } else { format!("{s} / {t} 条") }
                    }}
                </span>
                <Show
                    when=move || confirming.get()
                    fallback=move || {
                        view! {
                            <button
                                type="button"
                                class="btn btn-sm btn-danger"
                                data-testid="logs-clear-btn"
                                disabled=move || state.run_rows.get().is_empty()
                                on:click=move |_| confirming.set(true)
                            >
                                "清空"
                            </button>
                        }
                    }
                >
                    <span class="log-confirm" data-testid="logs-clear-confirm">
                        <span class="log-confirm-text">"清空全部运行日志？不可撤销"</span>
                        <button
                            type="button"
                            class="btn btn-sm btn-danger"
                            data-testid="logs-clear-yes"
                            on:click=move |_| {
                                confirming.set(false);
                                state.clear_runs();
                            }
                        >
                            "确认清空"
                        </button>
                        <button
                            type="button"
                            class="btn btn-sm btn-ghost"
                            data-testid="logs-clear-cancel"
                            on:click=move |_| confirming.set(false)
                        >
                            "取消"
                        </button>
                    </span>
                </Show>
            </div>

            <Show when=move || !state.run_error.get().is_empty()>
                <div class="log-inline-error" data-testid="logs-run-error">
                    {move || state.run_error.get()}
                </div>
            </Show>

            <Show
                when=move || !state.run_loading.get()
                fallback=move || {
                    view! {
                        <table class="table log-table">
                            <tbody>
                                <SkeletonRows cols=3 rows=8 />
                            </tbody>
                        </table>
                    }
                }
            >
                <Show
                    when=move || !state.run_rows.get().is_empty()
                    fallback=move || view! { <RunEmpty just_cleared=state.just_cleared /> }
                >
                    <Show
                        when=move || !state.visible_runs().is_empty()
                        fallback=move || {
                            view! {
                                <EmptyState test_id="logs-run-filtered-empty">
                                    "没有匹配的日志：换个关键字，或取消级别筛选"
                                </EmptyState>
                            }
                        }
                    >
                        <div
                            class="run-log-box"
                            data-testid="logs-run-feed"
                            role="log"
                            aria-live="off"
                        >
                            <For each=move || state.visible_runs() key=|r: &RunRow| r.key.clone() let:row>
                                <RunLine row=row />
                            </For>
                        </div>
                    </Show>
                </Show>
            </Show>
        </div>
    }
}

/// 级别多选切换。**选满三档自动回到「全部」**（清空选中集），
/// 这样「全部点亮」与「一个不选」不会变成两个都要解释的状态。
pub fn toggle_level(selected: &mut Vec<LogLevel>, lv: LogLevel) {
    if let Some(pos) = selected.iter().position(|x| *x == lv) {
        selected.remove(pos);
    } else {
        selected.push(lv);
    }
    if selected.len() >= LogLevel::all().len() {
        selected.clear();
    }
}

#[component]
fn RunLine(row: RunRow) -> impl IntoView {
    let level = row.level;
    let time = fmt_log_time(row.ts);
    let raw = row.level_raw.clone();
    view! {
        <div
            class=format!("run-log-line is-{}", level.as_str())
            data-testid=format!("logs-run-line-{}", row.key)
        >
            <span class="run-log-time">{time}</span>
            <span class="run-log-level" title=raw>
                {level.label()}
            </span>
            <span class="run-log-msg">{row.message}</span>
        </div>
    }
}

/// 空态分两种：从没抓到过 vs 刚被清空（后者要给「等新日志」的预期）。
#[component]
fn RunEmpty(just_cleared: RwSignal<bool>) -> impl IntoView {
    view! {
        <Show
            when=move || just_cleared.get()
            fallback=move || {
                view! {
                    <EmptyState test_id="logs-run-empty">
                        <span>"还没有运行日志。"</span>
                        <span>
                            "抓取、登录、自动投递等操作都会往这里写一行；到「概览」页开始一次抓取就有内容了。"
                        </span>
                    </EmptyState>
                }
            }
        >
            <EmptyState test_id="logs-run-empty">
                "已清空。下一次抓取或操作写入新日志后会出现在这里。"
            </EmptyState>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_level_cycles_back_to_all() {
        let mut v: Vec<LogLevel> = Vec::new();
        toggle_level(&mut v, LogLevel::Error);
        assert_eq!(v, vec![LogLevel::Error]);
        toggle_level(&mut v, LogLevel::Warn);
        assert_eq!(v.len(), 2);
        // 选满三档 = 等价于「全部」，自动清空
        toggle_level(&mut v, LogLevel::Info);
        assert!(v.is_empty(), "选满即回到不过滤");
        // 再点两次取消同一个
        toggle_level(&mut v, LogLevel::Warn);
        toggle_level(&mut v, LogLevel::Warn);
        assert!(v.is_empty());
    }
}
