//! Boss 消息页：装配 `features/chat` 的四个部件。
//!
//! 本文件**不写业务规则**：拉数据、发送、撤回、生成草稿全在
//! `features/chat/state.rs`。页面层只负责两件事——
//! 1) 首帧拉会话列表（以及 `page_tick` 变化时重拉）；
//! 2) 把 `Boot` 的错误通道包成 `fail`。

use leptos::prelude::*;

use crate::api::types::Friend;
use crate::components::shell::Boot;
use crate::components::widgets::Card;
use crate::features::chat::{ChatList, ChatState, ChatThread};

/// 卡片副标题：会话数 + 当前选中的会话。
///
/// 三件事按优先级排：还在读列表就只说「正在读取会话…」（此时的会话数是上一轮的
/// 陈旧值，报出来会让用户以为已经读完了）；否则报会话数；选中了某个人再追加
/// 「当前：某某」。
///
/// 名字为空串时回落到 `uid`：Boss 的会话名偶尔抓不到，显示一个空名字比显示
/// 一串数字更让人迷惑。
fn chat_subtitle(loading: bool, friends_count: usize, active: Option<&Friend>) -> String {
    if loading {
        return "正在读取会话…".to_string();
    }
    let mut s = format!("{friends_count} 个会话");
    if let Some(f) = active {
        let name = if f.name.is_empty() { f.uid.as_str() } else { f.name.as_str() };
        s.push_str(&format!(" · 当前：{name}"));
    }
    s
}

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
        chat_subtitle(
            state.loading_friends.get(),
            state.friends.get().len(),
            state.active.get().as_ref(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Friend;

    fn friend(uid: &str, name: &str) -> Friend {
        Friend { uid: uid.to_string(), name: name.to_string(), ..Default::default() }
    }

    /// 加载中优先：此时 `friends` 里装的还是上一轮的结果，
    /// 报出数量会让人以为刷新已经完成。
    #[test]
    fn subtitle_says_loading_before_it_reports_counts() {
        assert_eq!(chat_subtitle(true, 7, None), "正在读取会话…");
        // 即使在加载中且已选中某人，也不该透露数量
        assert_eq!(
            chat_subtitle(true, 7, Some(&friend("u1", "张三"))),
            "正在读取会话…"
        );
    }

    #[test]
    fn subtitle_reports_session_count_when_idle() {
        assert_eq!(chat_subtitle(false, 0, None), "0 个会话");
        assert_eq!(chat_subtitle(false, 12, None), "12 个会话");
    }

    #[test]
    fn subtitle_appends_active_session_name() {
        assert_eq!(
            chat_subtitle(false, 3, Some(&friend("u9", "李四"))),
            "3 个会话 · 当前：李四"
        );
    }

    /// 会话名抓不到时回落到 uid：显示空串会让副标题变成「3 个会话 · 当前：」。
    #[test]
    fn subtitle_falls_back_to_uid_when_name_is_blank() {
        assert_eq!(
            chat_subtitle(false, 1, Some(&friend("u42", ""))),
            "1 个会话 · 当前：u42"
        );
        // 只有全空白的名字**不**当成空：它是抓取侧的脏数据，显示出来才能被
        // 发现；静默替换成 uid 会让这类脏数据永远看不见。
        assert_eq!(
            chat_subtitle(false, 1, Some(&friend("u42", "  "))),
            "1 个会话 · 当前：  "
        );
    }
}
