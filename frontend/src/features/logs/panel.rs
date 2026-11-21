//! 日志页外壳：运行日志 / 决策日志双视图 + 刷新 + 自动刷新循环。
//!
//! ## 两套数据都在，不互相替代
//!
//! 用户裁决第 ⑦ 条的原话是「加运行日志，同时保留决策日志」。因此本面板
//! **同时**持有两条数据：运行日志（命令层的 2000 条环形缓冲，毫秒时间戳）
//! 与决策日志（`jd_decisions` 表，秒时间戳）。切换页签只是换渲染，不触发重拉
//! ——切回来时数据还在，不会闪一下空白。
//!
//! ## 自动刷新为什么必须能被卸载掐断
//!
//! 循环每 2 秒调一次 `list_logs(since=游标)`。若不在卸载时置位停止标志，
//! 离开日志页后循环会一直跑：每 2 秒一次 IPC + 一次信号写入，
//! 在「概览」页上跑一整天也不会有任何可见收益，反而抢主线程。
//! 照 `features/auto/queue.rs:30-46` 的写法用 `Arc<AtomicBool>`：
//! 卸载钩子（`on_cleanup`）里只搬一个 `Send + Sync` 的小令牌，
//! `Rc`/`Cell` 搬不进去。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use leptos::prelude::*;

use super::decision::DecisionView;
use super::runtime::RuntimeView;
use super::state::{LogState, LogView, AUTO_REFRESH_MS};
use wasm_bindgen::prelude::*;

/// 睡 `ms` 毫秒（`setTimeout` 的 Promise 包装）。
///
/// 为什么自己写一份而不是复用 `features/auto/timer.rs`：那边的模块是私有的
/// （`features/auto/mod.rs` 只导出 `AutoBar` / `AutoQueue`），而
/// `features/auto/**` 不在本 lane 的写权限内——不越界改它，只借用它的
/// **写法**。三行 `js_sys::Promise` 的复制成本远低于一次跨 lane 冲突。
///
/// 行为与那边一致：**WebView 挂起时 `setTimeout` 不触发**，`await` 永远悬着，
/// 循环自然停在等一拍上——这是已知且接受的行为，不要改成 `Date.now()` 追赶。
async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let ok = web_sys::window()
            .map(|w| {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
                    .is_ok()
            })
            .unwrap_or(false);
        if !ok {
            // 拿不到 window 时不能把循环永久挂着：立即放行，下一拍重试
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// 启动一个**不要求 `Send`** 的后台任务（WASM 单线程里本来也只有一个线程）。
fn spawn_detached<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    wasm_bindgen_futures::spawn_local(fut);
}


/// 「该停了」的令牌（与 `features/auto/queue.rs` 同一手法）。
#[derive(Clone, Default)]
pub struct KillToken(Arc<AtomicBool>);

impl KillToken {
    fn killed(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// 置位：循环会在下一次 `await` 返回后退出（最长一个 tick，2 秒）。
    pub fn kill(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// 启动自动刷新循环。返回停止令牌，交给 `on_cleanup`。
///
/// 循环条件里同时看两个标志：`state.run_auto`（用户开关）与令牌（卸载）。
/// 任一方为假即退出——**用户关掉开关后令牌不重置**，重新打开会新建一个
/// 令牌与循环（见 `Panel` 里的 `SyncSignal`）。
pub fn start_auto_refresh(state: LogState) -> KillToken {
    let token = KillToken::default();
    let flag = token.clone();
    spawn_detached(async move {
        while !flag.killed() && state.run_auto.get_untracked() {
            sleep_ms(AUTO_REFRESH_MS).await;
            if flag.killed() || !state.run_auto.get_untracked() {
                break;
            }
            // 只在真的停在运行日志视图时增量拉：切到决策视图后继续拉运行日志
            // 是纯粹的浪费（用户看不到），而切回来时 `page_tick` 与手动刷新
            // 都能补上。
            if state.view.get_untracked() == LogView::Runtime {
                state.load_runs(false);
            }
        }
    });
    token
}

#[component]
pub fn LogPanel(
    state: LogState,
    #[prop(optional)] on_refresh: Option<Callback<()>>,
) -> impl IntoView {
    let view = state.view;
    let auto = state.run_auto;

    // 自动刷新的循环生命周期跟着开关走：开 → 起循环；关 → 杀令牌。
    // 令牌存 `StoredValue`（组件存活期内的局部状态，不需要触发渲染）。
    let token = StoredValue::new(Option::<KillToken>::None);
    Effect::new(move |_| {
        let on = auto.get();
        let current = token.get_value();
        if on && current.is_none() {
            token.set_value(Some(start_auto_refresh(state)));
        } else if !on {
            if let Some(t) = current {
                t.kill();
                token.set_value(None);
            }
        }
    });

    // 卸载：令牌与 `Drop` 都兜一层，双保险（`Effect` 可能来不及跑）
    on_cleanup(move || {
        if let Some(t) = token.get_value() {
            t.kill();
            token.set_value(None);
        }
    });

    let refreshing = Signal::derive(move || state.run_loading.get() || state.dec_loading.get());

    view! {
        <section class="card log-wrap" data-testid="logs-panel">
            <header class="card-head">
                <h2 class="card-title" data-testid="logs-title">"日志"</h2>
                <div class="tabs" role="tablist" aria-label="日志视图">
                    {LogView::all()
                        .iter()
                        .map(|v| {
                            let v = *v;
                            view! {
                                <button
                                    type="button"
                                    role="tab"
                                    class=move || if view.get() == v { "tab is-active" } else { "tab" }
                                    aria-selected=move || if view.get() == v { "true" } else { "false" }
                                    data-testid=format!("logs-tab-{}", v.slug())
                                    on:click=move |_| view.set(v)
                                >
                                    {v.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <span class="spacer"></span>
                <Show when=move || auto.get() && view.get() == LogView::Runtime>
                    <span class="log-live" data-testid="logs-auto-hint" title="每 2 秒增量拉取，离开本页自动停止">
                        <span class="dot is-running"></span>"实时"
                    </span>
                </Show>
                <button
                    type="button"
                    class="btn btn-sm"
                    data-testid="logs-refresh"
                    disabled=move || refreshing.get()
                    on:click=move |_| {
                        state.refresh();
                        if let Some(cb) = on_refresh {
                            cb.run(());
                        }
                    }
                >
                    {move || if refreshing.get() { "刷新中…" } else { "刷新" }}
                </button>
            </header>
            <div class="card-body log-body">
                <Show when=move || view.get() == LogView::Runtime>
                    <RuntimeView state=state />
                </Show>
                <Show when=move || view.get() == LogView::Decision>
                    <DecisionView state=state />
                </Show>
            </div>
        </section>
    }
}
