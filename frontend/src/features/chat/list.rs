//! 会话列表（左栏）：搜索框 + 会话卡片 + 未读角标 + 加载骨架 + 空态。
//!
//! 结构、类名与文案逐字对齐旧版 `skeleton-web/src/features/chat/ChatPanel.tsx`
//! 的 `.chat-side`：`.chat-item-top`（名字 + 时间）、`.chat-item-sub`（最后消息）、
//! `.chat-item-meta`（公司 + 未读）。

use leptos::prelude::*;

use super::state::ChatState;
use crate::api::types::Friend;
use crate::components::widgets::SkeletonRows;

#[component]
pub fn ChatList(state: ChatState, fail: Callback<String>) -> impl IntoView {
    let items = move || state.filtered();
    let empty_text = move || {
        if state.friends.get().is_empty() {
            "还没有会话".to_string()
        } else {
            "没有匹配的会话".to_string()
        }
    };

    view! {
        <aside class="chat-side">
            <input
                class="input"
                data-testid="chat-search"
                type="search"
                placeholder="搜索 HR / 公司 / 消息"
                prop:value=move || state.search.get()
                on:input=move |ev| state.search.set(event_target_value(&ev))
            />

            <Show
                when=move || !state.loading_friends.get()
                fallback=move || {
                    view! {
                        // `SkeletonRows` 产出的是 `<tr>`，必须待在表格里
                        <table class="chat-skeleton">
                            <tbody>
                                <SkeletonRows cols=2 rows=6 />
                            </tbody>
                        </table>
                    }
                }
            >
                <Show
                    when=move || !items().is_empty()
                    fallback=move || {
                        view! {
                            <div class="empty" data-testid="chat-empty">
                                {empty_text}
                            </div>
                        }
                    }
                >
                    <div class="chat-list" role="listbox" aria-label="会话列表">
                        <For each=items key=|f: &Friend| f.uid.clone() let:friend>
                            {
                                let f = friend;
                                let f_click = f.clone();
                                let uid = f.uid.clone();
                                let uid_badge = f.uid.clone();
                                let uid_check = f.uid.clone();
                                view! {
                                    <button
                                        type="button"
                                        class=move || {
                                            let on = state
                                                .active
                                                .get()
                                                .map(|a| a.uid == uid_check)
                                                .unwrap_or(false);
                                            if on { "chat-item is-active" } else { "chat-item" }
                                        }
                                        data-testid=format!("chat-friend-item-{}", f.uid)
                                        on:click=move |_| state.select(
                                            f_click.clone(),
                                            fail,
                                        )
                                    >
                                        <div class="chat-item-top">
                                            <span class="chat-item-name">
                                                {if f.name.is_empty() {
                                                    f.uid.clone()
                                                } else {
                                                    f.name.clone()
                                                }}
                                            </span>
                                            <span class="chat-item-time">
                                                {last_time_text(&f.last_time)}
                                            </span>
                                        </div>
                                        <div class="chat-item-sub">
                                            {friend_line(&f)}
                                        </div>
                                        <div class="chat-item-meta">
                                            <span class="chat-item-company">{f.company.clone()}</span>
                                            <Show when=move || f.unread != 0>
                                                <span
                                                    class="unread"
                                                    data-testid=format!("chat-friend-unread-{}", uid_badge)
                                                    title=format!("{} 条未读", f.unread)
                                                >
                                                    {unread_text(f.unread)}
                                                </span>
                                            </Show>
                                        </div>
                                    </button>
                                }
                            }
                        </For>
                    </div>
                </Show>
            </Show>
        </aside>
    }
}

/// 最后一条消息；没有就用岗位名，再没有就占位。
fn friend_line(f: &Friend) -> String {
    if !f.last_msg.trim().is_empty() {
        return f.last_msg.clone();
    }
    if !f.title.trim().is_empty() {
        return f.title.clone();
    }
    "（暂无消息）".to_string()
}

fn unread_text(n: u32) -> String {
    if n > 99 {
        "99+".to_string()
    } else {
        n.to_string()
    }
}

/// 侧边车给的时间是 `MM-DD HH:mm` 一类的展示串，原样呈现；空则「—」。
fn last_time_text(v: &str) -> String {
    if v.trim().is_empty() {
        "—".to_string()
    } else {
        v.to_string()
    }
}
