//! 自动投递控制条（旧版「自动打招呼」）。
//!
//! # 行为契约（任务书原样，不可放宽）
//!
//! 1. 每调一次 `api::auto_greet_next(None)` **最多发一条**；
//! 2. 下一次调用的时刻**只由后端回执的 `next_at_ms` 决定**——
//!    前端不自己算「20000 ± 5 秒」，那个随机间隔由
//!    `app/src-tauri/src/core/domain/auto_greet.rs` 计算并随回执下发；
//! 3. 每秒轮询一次 `api::auto_greet_status()` 刷新倒计时，到点才调 `next`；
//! 4. `paused = true` 时不再调 `next`，状态文案**原样显示**后端给的
//!    `stop_reason`（「连续失败 3 次，已自动暂停」/「待投递列表已经处理完了」），
//!    不改写、不翻译、不截断；
//! 5. 窗口最小化或 WebView 挂起时循环自然停止——**已知且接受**，不要绕开。
//!
//! 循环本身在 `queue.rs`；本文件只管按钮与文案。
//!
//! # 为什么状态全部来自后端
//!
//! 前端若维护一份「我以为在跑」的本地状态机，就会出现「后端因连续失败已暂停、
//! 前端还在每秒发一条、每条都被拒」的刷屏。所以这里每个 tick 都用
//! `auto_greet_status` 覆盖本地快照，本地布尔量仅用于按钮的忙碌态。
//!
//! # 为什么不再自己渲染 `.auto-bar`
//!
//! 用户裁决：自动投递的「开始」按钮要贴在抓取条的「开始抓取」右边（图 2）。
//! 因此本组件只产出**扁平的按钮 + 状态节点**，由 `ScrapePanel` 的
//! `.scrape-bar` 作父容器排布（`div.scrape-bar > button[data-testid=auto-toggle]`）；
//! 数据契约与 `data-testid` 一个都没变，只是父节点换了。

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::queue::AutoQueue;
use super::state::{status_dot, status_text, Phase};
use crate::api;
use crate::api::types::AutoStatusReply;
use crate::components::widgets::{ErrorBar, StatusDot};

#[component]
pub fn AutoBar(
    /// 发出一条后回调（页面层据此刷新列表与页签计数）。「发出」= 后端回执 ok。
    #[prop(optional, into)] on_sent: Option<Callback<()>>,
    /// 错误/提示上抛给页面的错误条。`None` 时只在控制条内部显示。
    #[prop(optional, into)] on_error: Option<Callback<String>>,
) -> impl IntoView {
    let status = RwSignal::new(AutoStatusReply::default());
    let sending = RwSignal::new(false);
    let sending_label = RwSignal::new(String::new());
    // 按钮忙碌态：只覆盖「点击后到后端回执」这一小段，不给循环用。
    let busy = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    // 驱动循环。卸载时由 `on_cleanup` 把存活标志置 false，循环在下一个
    // `await` 返回后自行退出。`Rc` 不能进 `on_cleanup` 的闭包（该闭包要求
    // `Send + Sync`），所以句柄本体留在组件栈上，卸载时只搬运一个令牌信号。
    let queue = AutoQueue::start(status, sending, sending_label, on_sent, on_error);
    let token = queue.token();
    on_cleanup(move || token.kill());

    let phase = Memo::new(move |_| {
        let st = status.get();
        Phase::from_status(&st, sending.get())
    });

    // 状态点的语气（`dot is-{state}` 的 class 后缀）。`StatusDot` 的 props 是
    // 普通 `String`，不接收闭包，所以这里用 `Signal::derive` 包一层。
    let dot_state = Signal::derive(move || {
        let st = status.get();
        status_dot(Phase::from_status(&st, sending.get()), st.failures).to_string()
    });
    let on_toggle = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(String::new());
        let phase_now = phase.get_untracked();
        spawn_local(async move {
            let result = match phase_now {
                // 空闲 / 已停：先清零上一轮计数再开始，否则「已发 N 条」会接着上次累加。
                Phase::Idle | Phase::Stopped => {
                    let _ = api::auto_greet_reset().await;
                    api::auto_greet_resume().await.map(|_| ())
                }
                Phase::Running => api::auto_greet_pause(Some("用户已暂停")).await.map(|_| ()),
                Phase::Paused => api::auto_greet_resume().await.map(|_| ()),
            };
            match result {
                Ok(()) => {
                    if let Ok(st) = api::auto_greet_status().await {
                        status.set(st);
                    }
                }
                Err(e) => error.set(format!("自动投递操作失败：{e}")),
            }
            busy.set(false);
        });
    };

    let on_reset = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(String::new());
        spawn_local(async move {
            match api::auto_greet_reset().await {
                Ok(_) => {
                    if let Ok(st) = api::auto_greet_status().await {
                        status.set(st);
                    }
                }
                Err(e) => error.set(format!("重置失败：{e}")),
            }
            busy.set(false);
        });
    };

    let status_line = move || status_text(&status.get(), phase.get(), sending.get(), &sending_label.get());

    // 父容器是 `ScrapePanel` 的 `.scrape-bar`：四个节点平铺进去，
    // 顺序即「开始」→ 状态 → 重置 → 行内错误。
    view! {
        <button
            class="auto-toggle"
            class:is-on=move || matches!(phase.get(), Phase::Running)
            class:is-warn=move || matches!(phase.get(), Phase::Paused)
            data-testid="auto-toggle"
            title="自动投递：按后端下发的随机间隔逐条打招呼"
            prop:disabled=move || busy.get()
            on:click=on_toggle
        >
            {move || phase.get().toggle_label()}
        </button>
        <div class="auto-status" data-testid="auto-status" title=status_line>
            <StatusDot state=dot_state.get() label=String::new() />
            <span class="auto-status-text">{status_line}</span>
        </div>
        <button
            class="btn btn-sm btn-ghost"
            data-testid="auto-reset"
            prop:disabled=move || busy.get()
            title="清零本轮计数并退出自动投递"
            on:click=on_reset
        >
            "重置"
        </button>
        {move || {
            let err = error.get();
            (!err.is_empty()).then(|| view! {
                <div class="auto-error">
                    <ErrorBar message=err test_id="auto-error" />
                </div>
            })
        }}
    }
}
