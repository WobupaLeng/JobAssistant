//! 驱动自动投递的「泵」：一个异步循环，每秒看一眼后端状态，到点发一条。
//!
//! # 为什么是不依赖定时器的 async 循环
//!
//! 早先的实现用 `setInterval`，定时器句柄要在闭包里自我引用才能停。
//! 改用 `spawn_local` + `timer::sleep_ms`（内部就是 `setTimeout` 的 Promise）之后，
//! 循环就是一个普通的 Rust `loop`：**等待期间不占栈、不占事件循环**，
//! 只要把 `running` 置 false，循环会在下一次 `await` 返回后自己退出。
//! WebView 挂起时 `setTimeout` 不触发，循环自然停在 `await` 上——
//! 这正是任务书里「循环自然停止，这是已知且接受的行为」。
//!
//! # 铁律：间隔由后端给
//!
//! 循环**从不**自己算「20000 ± 5 秒」。它只读后端 `auto_greet_status` 回的
//! `next_at_ms`：没到点就继续睡，到点才调 `auto_greet_next(None)`，
//! 而 `next_at_ms` 又由背端在回执里重新下发。
//! 前端唯一算的是「还有几秒」和「该不该现在发」——两者都是后端时间的函数。

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use leptos::prelude::*;

use super::state::{reply_note, should_send, TICK_MS};
use super::timer::{sleep_ms, spawn_detached};
use crate::api;
use crate::api::types::AutoStatusReply;

/// 「该停了」的令牌。`Arc<AtomicBool>` 而不是 `Rc<Cell<bool>>`：
/// 卸载钩子（`on_cleanup`）要求闭包 `Send + Sync`，`Rc` 搬不进去。
///
/// 原子操作在 WASM 单线程下就是一次普通读写，没有额外开销。
#[derive(Clone, Default)]
pub struct KillToken(Arc<AtomicBool>);

impl KillToken {
    fn killed(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// 置位：循环会在下一次 `await` 返回后退出（最长一个 tick，1 秒）。
    pub fn kill(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// 自动投递的驱动句柄。持有它即视为「循环存在」；`Drop` 时停止。
///
/// 标志放 `Arc<AtomicBool>` 而不是 `RwSignal`：这个标志只被循环与 `Drop` 读写，
/// 不需要触发任何重渲染，用信号反而会让「每轮 tick」都去跑一遍依赖图。
pub struct AutoQueue {
    kill: KillToken,
}

impl AutoQueue {
    /// 启动循环。
    ///
    /// - `status`：后端状态快照信号（每个 tick 用 `auto_greet_status` 覆盖）
    /// - `sending` / `sending_label`：正在发送时的界面反馈
    /// - `on_sent`：发出一条（后端回执 `ok=true`）后回调
    /// - `on_error`：发送失败或 IPC 失败时回调（人类可读文案）
    pub fn start(
        status: RwSignal<AutoStatusReply>,
        sending: RwSignal<bool>,
        sending_label: RwSignal<String>,
        on_sent: Option<Callback<()>>,
        on_error: Option<Callback<String>>,
    ) -> Self {
        let kill = KillToken::default();
        let flag = kill.clone();
        spawn_detached(async move {
            while !flag.killed() {
                tick(status, sending, sending_label, on_sent, on_error).await;
                sleep_ms(TICK_MS).await;
            }
        });
        Self { kill }
    }

    /// 显式停止（与 `Drop` 等价，保留以便页面层提前收手）。
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.kill.kill();
    }

    /// 生成一个可跨线程搬运（`Send + Sync`）的「关闭令牌」。
    ///
    /// 句柄本体留在组件栈上，卸载钩子里只搬这份令牌。
    pub fn token(&self) -> KillToken {
        self.kill.clone()
    }
}

impl Drop for AutoQueue {
    fn drop(&mut self) {
        self.kill.kill();
    }
}

/// 一个 tick：刷新后端状态 → 判断该不该发 → 发一条（最多一条）。
async fn tick(
    status: RwSignal<AutoStatusReply>,
    sending: RwSignal<bool>,
    sending_label: RwSignal<String>,
    on_sent: Option<Callback<()>>,
    on_error: Option<Callback<String>>,
) {
    match api::auto_greet_status().await {
        Ok(st) => status.set(st),
        Err(e) => {
            // 状态拿不到时不要盲目发送：宁可这一拍跳过。
            push_error(on_error, format!("读取自动投递状态失败：{e}"));
            return;
        }
    }

    let snapshot = status.get_untracked();
    let in_flight = sending.get_untracked();
    if !should_send(now_ms(), &snapshot, in_flight) {
        return;
    }

    send_one(status, sending, sending_label, snapshot, on_sent, on_error).await;
}

/// 发一条。**每个 tick 最多调一次** `auto_greet_next`。
async fn send_one(
    status: RwSignal<AutoStatusReply>,
    sending: RwSignal<bool>,
    sending_label: RwSignal<String>,
    snapshot: AutoStatusReply,
    on_sent: Option<Callback<()>>,
    on_error: Option<Callback<String>>,
) {
    sending.set(true);
    sending_label.set(label_of(&snapshot));

    match api::auto_greet_next(None).await {
        Ok(reply) => {
            if !reply.ok {
                let msg = if reply.error.trim().is_empty() {
                    reply_note(&reply)
                } else {
                    reply.error.clone()
                };
                if !msg.trim().is_empty() {
                    push_error(on_error, msg.trim().to_string());
                }
            } else {
                if let Some(cb) = on_sent {
                    cb.run(());
                }
                // 发成功也把 note 显示出来（例如「连续失败 2 次，下一次重试」）。
                let note = reply_note(&reply);
                if !note.trim().is_empty() {
                    push_error(on_error, note.trim().to_string());
                }
            }
        }
        Err(e) => push_error(on_error, format!("发送招呼失败：{e}")),
    }

    // 不论成功与否都立刻重读状态：`next_at_ms` 只有后端知道，
    // 本次回执里的值不能直接拿去算下一次（跳过、失败、暂停的分支都不一样）。
    match api::auto_greet_status().await {
        Ok(st) => status.set(st),
        Err(e) => push_error(on_error, format!("刷新自动投递状态失败：{e}")),
    }

    sending_label.set(String::new());
    sending.set(false);
}

/// 正在发送时的人类可读描述：公司 + 岗位。
fn label_of(st: &AutoStatusReply) -> String {
    let _ = st;
    // 后端状态里没有「当前这条」的字段（只有 AutoNextReply 有），
    // 所以这里给一句不含公司名的话，避免前端自己缓存一个可能过期的「当前条」。
    "下一条".to_string()
}

fn push_error(on_error: Option<Callback<String>>, msg: String) {
    if let Some(cb) = on_error {
        cb.run(msg);
    }
}

/// 当前时间（毫秒）。与后端 `next_at_ms` 同一个时钟：都是 `Date.now()` 系。
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}
