//! 底部输入区：草稿框 + 发送按钮。
//!
//! 单独成文件而不是塞进 `thread.rs`：输入区的可用性判定（草稿非空、发送中
//! 禁用）与线程区的数据流无关，混在一起会让线程组件的信号数量翻倍。

use leptos::prelude::*;

use super::state::ChatState;

#[component]
pub fn ChatComposer(
    state: ChatState,
    on_notice: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let can_send = Signal::derive(move || {
        !state.draft.get().trim().is_empty() && !state.sending.get()
    });

    view! {
        <div class="chat-send">
            <textarea
                class="textarea chat-input"
                data-testid="chat-input"
                rows=2
                placeholder="输入消息，Ctrl+Enter 发送"
                prop:value=move || state.draft.get()
                on:input=move |ev| state.draft.set(event_target_value(&ev))
                on:keydown=move |ev| {
                    if ev.key() == "Enter" && (ev.ctrl_key() || ev.meta_key()) {
                        ev.prevent_default();
                        state.send_draft(on_notice, fail);
                    }
                }
            ></textarea>
            <button
                type="button"
                class="btn btn-primary"
                data-testid="chat-send-btn"
                disabled=move || !can_send.get()
                on:click=move |_| state.send_draft(on_notice, fail)
            >
                {move || if state.sending.get() { "发送中…" } else { "发送" }}
            </button>
        </div>
    }
}
