//! Boss 消息页：装配 `features/chat` 的四个部件。
//!
//! 本文件**不写业务规则**：拉数据、发送、撤回、生成草稿全在
//! `features/chat/state.rs`。页面层只负责两件事——
//! 1) 首帧拉会话列表（以及 `page_tick` 变化时重拉）；
//! 2) 把 `Boot` 的错误通道包成 `fail`。

use leptos::prelude::*;

use crate::components::shell::Boot;
use crate::components::widgets::Card;
use crate::features::chat::{ChatList, ChatState, ChatThread};

#[component]
pub fn ChatPage(page_tick: RwSignal<u32>, on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let fail = Callback::new(move |e: String| boot.error.set(Some(e)));

    let state = ChatState::new();

    // 首次进入 + 每次从别的页签切回来（`page_tick` 递增）都重拉会话列表。
    // 用 `prev_tick` 而不是 `Effect::new` 里的 `on(untracked)`：这里要的正是
    // 「首帧也跑一次」，而 `page_tick` 初值是 0、组件重建后又回到 0。
    let prev_tick = StoredValue::new(u32::MAX);
    Effect::new(move |_| {
        let tick = page_tick.get();
        if prev_tick.get_value() == tick {
            return;
        }
        prev_tick.set_value(tick);
        let state = state;
        state.load_friends(fail);
    });

    let subtitle = Signal::derive(move || {
        if state.loading_friends.get() {
            return "正在读取会话…".to_string();
        }
        let n = state.friends.get().len();
        let mut s = format!("{n} 个会话");
        if let Some(f) = state.active.get() {
            let name = if f.name.is_empty() { f.uid } else { f.name };
            s.push_str(&format!(" · 当前：{name}"));
        }
        s
    });

    view! {
        <div class="chat-wrap" data-testid="chat-panel">
            <Card
                title="Boss 消息"
                subtitle=subtitle
                actions=Box::new(move || {
                    view! {
                        <button
                            type="button"
                            class="btn btn-sm"
                            data-testid="chat-refresh-btn"
                            disabled=move || state.loading_friends.get()
                            on:click=move |_| state.load_friends(fail)
                        >
                            {move || if state.loading_friends.get() { "刷新中…" } else { "刷新会话" }}
                        </button>
                    }
                    .into_any()
                }) as Children
            >
                <div class="chat-grid">
                    <ChatList state=state fail=fail />
                    <ChatThread state=state on_notice=on_notice fail=fail />
                </div>
            </Card>
        </div>
    }
}
