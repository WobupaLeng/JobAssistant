//! 线程区（右栏）：会话头 + 三个动作按钮 + 气泡列表 + 打招呼草稿卡 + 简历条。
//!
//! 类名与文案逐字对齐旧版 `.chat-main` / `.chat-head` / `.chat-feed` /
//! `.greet-box` / `.resume-bar`。**只有行为在本地组装**，每条命令都直连
//! 同进程 Rust（没有本地 Mock）。

use leptos::prelude::*;

use super::composer::ChatComposer;
use super::state::ChatState;
use crate::api::types::{ChatMessage, GreetReply, ResumeItem};
use crate::components::widgets::{Chip, SkeletonRows};

#[component]
pub fn ChatThread(
    state: ChatState,
    on_notice: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let head_sub = move || match state.active.get() {
        Some(f) => {
            let parts: Vec<String> = [f.company.clone(), f.title.clone()]
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
            if parts.is_empty() {
                "—".to_string()
            } else {
                parts.join(" · ")
            }
        }
        None => "—".to_string(),
    };

    let feed = Signal::derive(move || state.messages.get());
    let typing = Signal::derive(move || state.typing.get());
    let greet = Signal::derive(move || state.greet.get());

    view! {
        <section class="chat-main">
            <Show
                when=move || state.active.get().is_some()
                fallback=move || {
                    view! {
                        <div class="empty" data-testid="chat-hint">
                            "左侧选一位 HR 开始。"
                        </div>
                    }
                }
            >
                <div class="chat-head">
                    <div class="chat-head-text">
                        <div class="chat-head-name">
                            {move || {
                                state
                                    .active
                                    .get()
                                    .map(|f| if f.name.is_empty() { f.uid } else { f.name })
                                    .unwrap_or_default()
                            }}
                        </div>
                        <div class="chat-head-sub">{head_sub}</div>
                    </div>
                    <div class="chat-head-actions">
                        <button
                            type="button"
                            class="btn btn-sm"
                            data-testid="chat-greet-btn"
                            disabled=move || state.generating.get()
                            on:click=move |_| state.generate_greet(fail)
                        >
                            {move || if state.generating.get() { "生成中…" } else { "生成打招呼" }}
                        </button>
                        <button
                            type="button"
                            class="btn btn-sm"
                            data-testid="chat-resume-btn"
                            on:click=move |_| state.resumes_open.update(|v| *v = !*v)
                        >
                            "选简历"
                        </button>
                        <button
                            type="button"
                            class="btn btn-sm btn-ghost"
                            data-testid="chat-recall-btn"
                            disabled=move || state.recalling.get()
                            on:click=move |_| {
                                if confirm("确定撤回最近一条自己发的消息？") {
                                    state.recall(on_notice, fail);
                                }
                            }
                        >
                            {move || if state.recalling.get() { "撤回中…" } else { "撤回上一条" }}
                        </button>
                    </div>
                </div>

                <Show when=move || state.resumes_open.get()>
                    <ResumeBar state=state on_notice=on_notice fail=fail />
                </Show>

                <div class="chat-feed" data-testid="chat-feed">
                    <Show
                        when=move || !state.loading_messages.get()
                        fallback=move || {
                            view! {
                                <table class="chat-skeleton">
                                    <tbody>
                                        <SkeletonRows cols=1 rows=5 />
                                    </tbody>
                                </table>
                            }
                        }
                    >
                        <Show
                            when=move || !feed.get().is_empty()
                            fallback=move || view! { <div class="empty">"还没有消息记录"</div> }
                        >
                            <For each=move || feed.get() key=|m: &ChatMessage| m.msg_id.clone() let:msg>
                                <Bubble msg=msg />
                            </For>
                        </Show>
                        <Show when=move || typing.get()>
                            <div class="bubble-row is-system" data-testid="chat-typing">
                                <div class="bubble-typing">"对方正在输入…"</div>
                            </div>
                        </Show>
                    </Show>
                </div>

                <Show when=move || greet.get().is_some()>
                    <GreetBox state=state on_notice=on_notice fail=fail />
                </Show>

                <ChatComposer state=state on_notice=on_notice fail=fail />

                <Show when=move || !state.note.get().is_empty()>
                    <div class="cfg-hint" data-testid="chat-note">{move || state.note.get()}</div>
                </Show>
            </Show>
        </section>
    }
}

/// 一条气泡。`sender` 为 `me` 靠右、`system` 居中、其余靠左。
#[component]
fn Bubble(msg: ChatMessage) -> impl IntoView {
    let cls = format!("bubble-row is-{}", sender_slug(&msg.sender));
    let test_id = if msg.msg_id.is_empty() {
        None
    } else {
        Some(format!("chat-msg-{}", msg.msg_id))
    };
    let content = msg.content.clone();
    let recalled = msg.recalled;
    let time = if msg.time.trim().is_empty() {
        "—".to_string()
    } else {
        msg.time.clone()
    };
    view! {
        <div class=cls>
            <div class="bubble" data-testid=test_id>
                {if recalled {
                    view! { <div class="bubble-recall">"该消息已撤回"</div> }.into_any()
                } else {
                    view! { <div class="bubble-text">{content}</div> }.into_any()
                }}
                <div class="bubble-time">{time}</div>
            </div>
        </div>
    }
}

fn sender_slug(sender: &str) -> String {
    match sender {
        "me" | "self" => "me".to_string(),
        "system" | "typing" => "system".to_string(),
        other => {
            if other.trim().is_empty() {
                "hr".to_string()
            } else {
                other.to_string()
            }
        }
    }
}

/// 打招呼草稿卡：绿 chip「大模型 · N tokens」/「规则话术」+ 正文 + 三个动作。
#[component]
fn GreetBox(
    state: ChatState,
    on_notice: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let source_chip = move || {
        state.greet.get().map(|g: GreetReply| {
            let is_llm = g.source == "llm";
            let tone = if is_llm { "primary" } else { "warn" };
            let text = if is_llm {
                if g.tokens > 0 {
                    format!("大模型 · {} tokens", g.tokens)
                } else {
                    "大模型".to_string()
                }
            } else {
                "规则话术".to_string()
            };
            view! { <Chip tone=tone.to_string()>{text}</Chip> }
        })
    };

    view! {
        <div class="greet-box" data-testid="chat-greet-box">
            <div class="greet-head">
                <span class="section-label">"打招呼草稿"</span>
                {source_chip}
            </div>
            <div class="greet-text" data-testid="chat-greet-text">
                {move || state.greet.get().map(|g| g.text).unwrap_or_default()}
            </div>
            <Show when=move || {
                state.greet.get().map(|g| !g.note.trim().is_empty()).unwrap_or(false)
            }>
                <div class="cfg-hint">
                    {move || state.greet.get().map(|g| g.note).unwrap_or_default()}
                </div>
            </Show>
            <div class="greet-actions">
                <button
                    type="button"
                    class="btn btn-sm btn-primary"
                    data-testid="chat-greet-send"
                    disabled=move || state.sending_greet.get()
                    on:click=move |_| state.send_greet(on_notice, fail)
                >
                    {move || if state.sending_greet.get() { "发送中…" } else { "发送这句" }}
                </button>
                <button
                    type="button"
                    class="btn btn-sm btn-ghost"
                    data-testid="chat-greet-todraft"
                    on:click=move |_| {
                        if let Some(g) = state.greet.get_untracked() {
                            state.draft.set(g.text);
                        }
                    }
                >
                    "放到输入框再改"
                </button>
                <button
                    type="button"
                    class="btn btn-sm btn-ghost"
                    data-testid="chat-greet-discard"
                    on:click=move |_| {
                        state.greet.set(None);
                        state.note.set(String::new());
                    }
                >
                    "丢弃"
                </button>
            </div>
        </div>
    }
}

/// 简历条：每份简历一个可点的 chip。发出前二次确认——外发不可撤回。
#[component]
fn ResumeBar(
    state: ChatState,
    on_notice: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    view! {
        <div class="resume-bar" data-testid="chat-resume-bar">
            <span class="section-label">"简历"</span>
            <Show
                when=move || !state.resumes.get().is_empty()
                fallback=move || {
                    view! {
                        <span class="cfg-hint" data-testid="chat-resume-empty">
                            "没读到简历列表。确认 Boss 账号里已上传附件简历。"
                        </span>
                    }
                }
            >
                <For each=move || state.resumes.get() key=|r: &ResumeItem| r.resume_id.clone() let:item>
                    {
                        let id = item.resume_id.clone();
                        let name = item.name.clone();
                        let name_for_confirm = item.name.clone();
                        view! {
                            <button
                                type="button"
                                class="chip chip-accent as-btn"
                                data-testid=format!("chat-resume-{}", item.resume_id)
                                on:click=move |_| {
                                    let peer = state
                                        .active
                                        .get_untracked()
                                        .map(|f| if f.name.is_empty() { f.uid } else { f.name })
                                        .unwrap_or_default();
                                    let ask = format!(
                                        "确定把简历「{}」发给「{}」？发出去之后无法收回。",
                                        name_for_confirm, peer
                                    );
                                    if confirm(&ask) {
                                        state.send_resume(id.clone(), on_notice, fail);
                                    }
                                }
                            >
                                {name}
                            </button>
                        }
                    }
                </For>
            </Show>
            <button
                type="button"
                class="btn btn-sm btn-ghost"
                data-testid="chat-resume-close"
                on:click=move |_| state.resumes_open.set(false)
            >
                "收起"
            </button>
        </div>
    }
}

/// 二次确认。WebView 里 `window.confirm` 可用，与旧版一致。
pub(super) fn confirm(msg: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(msg).ok())
        .unwrap_or(false)
}
