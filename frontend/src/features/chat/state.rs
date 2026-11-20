//! Boss 聊天的界面状态：会话列表、当前会话、消息、打招呼草稿、简历列表。
//!
//! 只有 Boss 消息页用，所以留在 `features/chat`。
//!
//! 关于自愈：`sidecar` 没起来时命令层返回的是一段中文文案
//! （判据见 `lib_ui/session.rs::needs_session`）。这种情况下本模块会
//! `session_open("boss", "chat")` 把浏览器带到聊天页再重试一次——
//! 用户不该为了看消息先去别的页面点一次「开始抓取」。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::*};
use crate::lib_ui::session::needs_session;

#[derive(Clone, Copy)]
pub struct ChatState {
    pub friends: RwSignal<Vec<Friend>>,
    pub loading_friends: RwSignal<bool>,
    pub search: RwSignal<String>,
    pub active: RwSignal<Option<Friend>>,
    pub messages: RwSignal<Vec<ChatMessage>>,
    pub loading_messages: RwSignal<bool>,
    pub typing: RwSignal<bool>,
    pub draft: RwSignal<String>,
    pub greet: RwSignal<Option<GreetReply>>,
    pub generating: RwSignal<bool>,
    pub sending: RwSignal<bool>,
    pub sending_greet: RwSignal<bool>,
    pub recalling: RwSignal<bool>,
    pub resumes: RwSignal<Vec<ResumeItem>>,
    pub resumes_open: RwSignal<bool>,
    pub resumes_loaded: RwSignal<bool>,
    /// 底部的灰字说明。空串表示不显示。
    pub note: RwSignal<String>,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            friends: RwSignal::new(Vec::new()),
            loading_friends: RwSignal::new(true),
            search: RwSignal::new(String::new()),
            active: RwSignal::new(None),
            messages: RwSignal::new(Vec::new()),
            loading_messages: RwSignal::new(false),
            typing: RwSignal::new(false),
            draft: RwSignal::new(String::new()),
            greet: RwSignal::new(None),
            generating: RwSignal::new(false),
            sending: RwSignal::new(false),
            sending_greet: RwSignal::new(false),
            recalling: RwSignal::new(false),
            resumes: RwSignal::new(Vec::new()),
            resumes_open: RwSignal::new(false),
            resumes_loaded: RwSignal::new(false),
            note: RwSignal::new(String::new()),
        }
    }

    /// 搜索框过滤后的会话。`name` / `company` / `title` / `last_msg` 任一命中即可。
    pub fn filtered(&self) -> Vec<Friend> {
        let q = self.search.get().trim().to_lowercase();
        self.friends
            .get()
            .into_iter()
            .filter(|f| {
                q.is_empty()
                    || [&f.name, &f.company, &f.title, &f.last_msg]
                        .iter()
                        .any(|s| s.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// 拉会话列表。首次失败若属于「sidecar 没起来」，拉起浏览器后重试一次。
    pub fn load_friends(self, fail: Callback<String>) {
        self.loading_friends.set(true);
        spawn_local(async move {
            let mut result = api::chat_friends().await;
            if let Err(e) = &result {
                if needs_session(e) {
                    self.note.set("正在启动浏览器…".to_string());
                    let _ = api::session_open("boss", Some("chat")).await;
                    result = api::chat_friends().await;
                }
            }
            match result {
                Ok(reply) => {
                    let items = reply.items;
                    if items.is_empty() {
                        self.note.set(
                            "会话列表是空的。确认已登录 Boss；未登录可到「概览」页点「扫码登录」。"
                                .to_string(),
                        );
                    } else if self.note.get_untracked() == "正在启动浏览器…" {
                        self.note.set(String::new());
                    }
                    // 当前会话被刷新：保留选中态，但用新的未读/摘要覆盖
                    let active_uid = self.active.get_untracked().map(|a| a.uid);
                    if let Some(uid) = active_uid {
                        if let Some(f) = items.iter().find(|x| x.uid == uid) {
                            self.active.set(Some(f.clone()));
                        }
                    }
                    self.friends.set(items);
                }
                Err(e) => {
                    self.note.set(e.clone());
                    fail.run(format!("读取会话失败：{e}"));
                }
            }
            self.loading_friends.set(false);
        });
    }

    /// 选一位 HR：清掉上一位的草稿/消息/未读，再拉历史消息。
    pub fn select(self, friend: Friend, fail: Callback<String>) {
        let uid = friend.uid.clone();
        let mut picked = friend;
        // 未读清零只影响本地角标：真正的已读状态在 Boss 侧，进会话即已读
        picked.unread = 0;
        if self.active.get_untracked().map(|a| a.uid) != Some(uid.clone()) {
            self.messages.set(Vec::new());
            self.greet.set(None);
            self.draft.set(String::new());
            self.resumes_open.set(false);
        }
        self.active.set(Some(picked));
        self.friends
            .update(|list| {
                if let Some(f) = list.iter_mut().find(|x| x.uid == uid) {
                    f.unread = 0;
                }
            });
        self.load_messages(fail);
        if !self.resumes_loaded.get_untracked() {
            self.load_resumes(fail);
        }
    }

    pub fn load_messages(self, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        self.loading_messages.set(true);
        spawn_local(async move {
            match api::chat_messages(&friend.uid).await {
                Ok(reply) => {
                    // 侧边车把「对方正在输入」也当成一条 system 消息推上来，
                    // 它不是历史消息，取出来单独做提示，避免刷新后残留在气泡里
                    let mut typing = false;
                    let mut list = Vec::new();
                    for m in reply.items {
                        if m.sender == "system" && m.content.contains("正在输入") {
                            typing = true;
                        } else {
                            list.push(m);
                        }
                    }
                    self.typing.set(typing);
                    self.messages.set(list);
                }
                Err(e) => fail.run(format!("读取消息失败：{e}")),
            }
            self.loading_messages.set(false);
        });
    }

    pub fn load_resumes(self, fail: Callback<String>) {
        spawn_local(async move {
            match api::chat_resumes().await {
                Ok(reply) => {
                    self.resumes.set(reply.items);
                    self.resumes_loaded.set(true);
                }
                Err(e) => fail.run(format!("读取简历列表失败：{e}")),
            }
        });
    }

    /// 生成打招呼草稿。命令只生成、不发送——刻意拆成两步。
    pub fn generate_greet(self, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        if self.generating.get_untracked() {
            return;
        }
        self.generating.set(true);
        spawn_local(async move {
            // 只有会话头里的岗位信息可用；job_id / 薪资 / 经验 / JD 传空，
            // 命令层会用这些字段构造一份临时岗位来出话术。
            let result = api::greet_draft(
                "",
                &friend.title,
                &friend.company,
                "",
                "",
                "",
            )
            .await;
            match result {
                Ok(reply) => {
                    self.note.set(reply.note.clone());
                    self.greet.set(Some(reply));
                }
                Err(e) => fail.run(format!("生成打招呼失败：{e}")),
            }
            self.generating.set(false);
        });
    }

    /// 「发送这句」：发的是草稿卡片里的文本，不是输入框里的。
    pub fn send_greet(self, on_notice: Callback<String>, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        let Some(greet) = self.greet.get_untracked() else {
            return;
        };
        let text = greet.text.trim().to_string();
        if text.is_empty() || self.sending_greet.get_untracked() {
            return;
        }
        self.sending_greet.set(true);
        spawn_local(async move {
            // 走 `greet_one` 而不是 `chat_send`：后者是普通消息，前者带打招呼的
            // 幂等与首句标记（命令层落点不同）。
            match api::greet_one(&friend.uid, &text).await {
                Ok(_) => {
                    self.messages.update(|list| {
                        list.push(ChatMessage {
                            sender: "me".to_string(),
                            content: text.clone(),
                            time: now_text(),
                            msg_id: String::new(),
                            recalled: false,
                        });
                    });
                    self.greet.set(None);
                    self.note.set(String::new());
                    on_notice.run("打招呼已发送".to_string());
                }
                Err(e) => fail.run(format!("发送打招呼失败：{e}")),
            }
            self.sending_greet.set(false);
        });
    }

    /// 输入框发送。成功后清空输入框并追加一条自己的气泡。
    pub fn send_draft(self, on_notice: Callback<String>, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        let text = self.draft.get_untracked().trim().to_string();
        if text.is_empty() || self.sending.get_untracked() {
            return;
        }
        self.sending.set(true);
        spawn_local(async move {
            match api::chat_send(&friend.uid, &text, None).await {
                // 列表里显示后端**真正发出去**的正文（过长时后端会截断），
                // 而不是本地输入框里的那份，否则用户会以为自己发了全文
                Ok(reply) => {
                    let sent = if reply.text.is_empty() { text.clone() } else { reply.text };
                    self.draft.set(String::new());
                    self.messages.update(|list| {
                        list.push(ChatMessage {
                            sender: "me".to_string(),
                            content: sent,
                            time: now_text(),
                            msg_id: String::new(),
                            recalled: false,
                        });
                    });
                    on_notice.run("消息已发送".to_string());
                }
                Err(e) => fail.run(format!("发送失败：{e}")),
            }
            self.sending.set(false);
        });
    }

    pub fn send_resume(self, resume_id: String, on_notice: Callback<String>, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match api::chat_send_resume(&friend.uid, &resume_id).await {
                Ok(()) => {
                    self.resumes_open.set(false);
                    on_notice.run("简历已发出".to_string());
                    self.load_messages(fail);
                }
                Err(e) => fail.run(format!("发送简历失败：{e}")),
            }
        });
    }

    /// 撤回最近一条自己发的消息。撤回后必须重拉，否则界面上那条还在。
    pub fn recall(self, on_notice: Callback<String>, fail: Callback<String>) {
        let Some(friend) = self.active.get_untracked() else {
            return;
        };
        if self.recalling.get_untracked() {
            return;
        }
        self.recalling.set(true);
        spawn_local(async move {
            match api::chat_recall(&friend.uid).await {
                Ok(()) => {
                    on_notice.run("已撤回上一条消息".to_string());
                    self.load_messages(fail);
                }
                Err(e) => fail.run(format!("撤回失败：{e}")),
            }
            self.recalling.set(false);
        });
    }
}

/// 本地时钟的 `HH:mm`。刚发出去的气泡没有真实时间戳，用当前时间占位。
fn now_text() -> String {
    let d = js_sys::Date::new_0();
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}
