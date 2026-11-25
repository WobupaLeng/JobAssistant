//! Boss 聊天命令层。
//!
//! 这一层的职责只有三件：**拿会话句柄 → 发命令 → 把回执转成前端结构**。
//! 业务判定（谁发的、能不能发简历）全在 `scraper/chat.py`，不在这里重写——
//! 命令层写业务规则正是重写前 `main.py` 40 处全局可变状态的老路（AGENTS.md 2.3）。
//!
//! ## 为什么不复用 `SessionUpdate` 事件流
//!
//! 聊天是**请求/应答**：前端点一下必须立刻拿到结果或错误。抓取那种「发完就不管、
//! 进度靠事件流推」的模型套不上来。所以这里用 `SidecarSession::request`，
//! 它会在 `core/src/sidecar/process.rs` 的在途表里等同一个 `id` 的回执。
//!
//! ## 为什么先克隆句柄再还锁
//!
//! 一次聊天可能要等几十秒（页面加载 + 接口返回）。如果全程占着
//! `Mutex<Option<SidecarSession>>`，界面上的 `scrape_progress` 轮询会被饿死，
//! 表现为「抓取中」的数字突然不动。所以：加锁 → 克隆句柄 → **立刻释放锁**
//! → 在锁外等回执。
//!
//! ## 为什么这个文件里等回执的命令全是 `async`
//!
//! 同步 `#[tauri::command]` 由 Tauri **在主线程上执行**（`Webview::on_message`
//! → `Window::manager` 都是 `run_on_main_thread`）。主线程上的
//! `rx.recv_timeout(timeout)` 会把 WebView2 的消息泵一起按停，最长 `CHAT_TIMEOUT`
//! 也就是 90 秒——窗口拖不动、按钮点不动、日志不滚，用户看到的就是「程序无响应」。
//! 改成 `async` 之后 Tauri 把它派到异步运行时的工作线程上，主线程只负责收发，
//! 等回执期间界面照常可点可拖。
//!
//! 剩下的限制是 sidecar 自己的单车道（`_chat_q`）：第二条聊天命令会排队等第一条，
//! 但**排队发生在后台线程，不耽误界面**。纯本地读库的命令（`*_local`）保持同步。

use std::time::Duration;

use crate::core::sidecar::process::SidecarHandle;
use crate::core::sidecar::protocol::{ChatMessage, Command, Expectation, Friend, ResumeItem};
use crate::core::infra::sqlite::{ChatFriendRow, ChatMessageRow};
use serde::{Deserialize, Serialize};

use crate::commands_scrape::ScrapeState;
use crate::commands_db::{store_err, with_store, CmdResult};
use crate::core::domain::store_ports::JobAdminStore;

/// 聊天回执的等待上限。
///
/// 不设更短：`chat.friends` 会 reload 整个聊天页，网络慢时 15 秒都不够。
/// 也不设更长：用户点了「发消息」却在 90 秒内毫无反馈，会以为程序死了。
const CHAT_TIMEOUT: Duration = Duration::from_secs(90);

// ---------- 回执结构 ----------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FriendsReply {
    pub items: Vec<Friend>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessagesReply {
    pub items: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResumesReply {
    pub items: Vec<ResumeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SendReply {
    /// 实际发出去的文本（过长时被截到 1000 字，前端要显示真正发出去的内容）
    pub text: String,
    /// 顺带标记为已投递的岗位 id；没传 `jobId` 或标记失败时为空串
    #[serde(default)]
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OkReply {
    pub ok: bool,
}

/// 「建立会话」的回执：服务端为这个岗位分配的会话 uid。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenReply {
    pub uid: String,
}

// ---------- 内部：取句柄并发命令 ----------

/// 拿到一个**已脱离锁**的句柄；没有会话就明确报错。
///
/// 返回 `SidecarHandle` 而不是 `SidecarSession`：句柄全是 `Arc`，克隆即用，
/// 调用方拿到后锁已经还回去了。
pub(crate) fn handle_of(state: &tauri::State<'_, ScrapeState>) -> CmdResult<SidecarHandle> {
    let guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    match guard.as_ref() {
        Some(s) => Ok(s.handle_clone()),
        // 说清楚「怎么办」：用户看到「未启动」往往不知道下一步要做什么
        None => Err(
            "sidecar 尚未启动。请先点「开始抓取」或「扫码登录」把浏览器拉起来，再打开 Boss 消息"
                .to_string(),
        ),
    }
}

/// 在锁外等待一条聊天回执。
pub(crate) fn request_chat(
    handle: &SidecarHandle,
    cmd: &Command,
) -> CmdResult<serde_json::Value> {
    handle.request(cmd, CHAT_TIMEOUT).map_err(|e| {
        // 超时与业务失败要分开说：前者多半是浏览器卡住，后者是「没登录/找不到会话」
        //
        // 失败也打到 stderr：界面只能显示一句话，而排查需要完整链条
        // （哪条命令、sidecar 死前说了什么）。历史上 sidecar 静默崩溃，
        // 界面上只剩「stdin 已关闭」，谁也接不上话。
        eprintln!("[sidecar] {cmd:?} 失败：{e}");
        crate::commands_logs::push("warn", format!("聊天命令失败：{e}"));
        format!("{e}")
    })
}

/// 把回执里的 `items` 反序列化成目标类型。
///
/// 不直接 `serde_json::from_value` 整个 data：sidecar 可能多传字段，
/// 逐项转换能容错，不会因为一个无关字段让整个列表失败。
fn items_of<T: for<'de> Deserialize<'de>>(data: &serde_json::Value) -> CmdResult<Vec<T>> {
    let raw = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(raw
        .into_iter()
        .filter_map(|v| serde_json::from_value::<T>(v).ok())
        .collect())
}

// ---------- 命令 ----------

/// 会话列表（左侧 HR 列表）。
///
/// 每次调用都会重新拉一次接口，所以第一次可能比之后几次慢（要 reload 聊天页）。
#[tauri::command]
pub async fn chat_friends(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, crate::commands_db::StoreState>,
) -> CmdResult<FriendsReply> {
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatFriends)?;
    let mut items: Vec<Friend> = items_of(&data)?;
    // 远程成功后才落库；落库失败不影响本次结果（缓存是增强，不是前置条件）
    let rows: Vec<ChatFriendRow> = items.iter().map(friend_to_row).collect();
    if let Err(e) = with_store(&store, |s| s.upsert_chat_friends(BOSS, &rows).map_err(store_err)) {
        eprintln!("[chat] 会话列表写库失败：{e}");
        crate::commands_logs::push("warn", format!("会话列表写库失败：{e}"));
    }
    fill_job_ids(&store, &mut items);
    Ok(FriendsReply { items })
}

/// 给会话列表补上岗位 id。
///
/// Boss 返回的会话行只有公司名，没有岗位 id；而这条会话的 uid 一旦失效，前端唯一
/// 能走通的路是「按岗位重新点立即沟通」，所以必须在会话行上带上它。
///
/// 按公司名翻库（`JobStore::find_job_id_by_company`）。查不到就留空——前端据此
/// 放弃重建并照实报错，不编造岗位。
///
/// 查库失败与查不到一样处理：补岗位 id 是增强，不该让整个会话列表因为一次
/// 读库失败而报错。
fn fill_job_ids(store: &tauri::State<'_, crate::commands_db::StoreState>, items: &mut [Friend]) {
    let mut cache: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut db_down = false;
    for f in items.iter_mut() {
        if db_down {
            break;
        }
        let Some(company) = f.company.as_deref().map(str::trim) else {
            continue;
        };
        if company.is_empty() {
            continue;
        }
        if let Some(hit) = cache.get(company) {
            f.job_id = hit.clone();
            continue;
        }
        match with_store(store, |s| s.find_job_id_by_company(company).map_err(store_err)) {
            Ok(hit) => {
                cache.insert(company.to_string(), hit.clone());
                f.job_id = hit;
            }
            Err(e) => {
                // 读库坏了：记一次日志就停手，别对 100 行各报一遍。
                eprintln!("[chat] 补岗位 id 读库失败：{e}");
                crate::commands_logs::push("warn", format!("补岗位 id 读库失败：{e}"));
                db_down = true;
            }
        }
    }
}

/// 读本地缓存的会话列表（不触发浏览器，供界面先展示）。
#[tauri::command]
pub fn chat_friends_local(
    store: tauri::State<'_, crate::commands_db::StoreState>,
) -> CmdResult<FriendsReply> {
    let rows = with_store(&store, |s| s.list_chat_friends(BOSS).map_err(store_err))?;
    let mut items: Vec<Friend> = rows.iter().map(row_to_friend).collect();
    fill_job_ids(&store, &mut items);
    Ok(FriendsReply { items })
}

/// 读本地缓存的某会话消息（不触发浏览器）。
#[tauri::command]
pub fn chat_messages_local(
    store: tauri::State<'_, crate::commands_db::StoreState>,
    uid: String,
) -> CmdResult<MessagesReply> {
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Ok(MessagesReply::default());
    }
    let rows = with_store(&store, |s| s.list_chat_messages(BOSS, &uid).map_err(store_err))?;
    Ok(MessagesReply {
        items: rows.iter().map(row_to_message).collect(),
    })
}

/// 平台标识：当前聊天命令只服务 Boss。
const BOSS: &str = "boss";

fn friend_to_row(f: &Friend) -> ChatFriendRow {
    ChatFriendRow {
        uid: f.uid.clone(),
        name: f.name.clone(),
        company: f.company.clone(),
        title: f.title.clone(),
        last_msg: f.last_msg.clone(),
        last_time: f.last_time.clone(),
        unread: f.unread.unwrap_or(0),
        avatar: f.avatar.clone(),
        // 判新旧用的时间戳（毫秒）：sidecar 在 `chat.friends` 里给。
        // 缺省为 `None`（DOM 兜底路径取不到时间），落库时保留库里已有的值。
        last_ts: f.last_ts.filter(|v| *v > 0),
        // 送达态同样原样落库：界面靠它给会话副行加 `[送达]`/`[已读]`。
        // `last_status` 为 0 视同没有（Boss 用 0 表示「对方发的」）。
        last_from_me: f.last_from_me,
        last_status: f.last_status.filter(|v| *v > 0),
    }
}

fn row_to_friend(r: &ChatFriendRow) -> Friend {
    Friend {
        uid: r.uid.clone(),
        name: r.name.clone(),
        company: r.company.clone(),
        title: r.title.clone(),
        last_msg: r.last_msg.clone(),
        last_time: r.last_time.clone(),
        unread: Some(r.unread),
        avatar: r.avatar.clone(),
        last_ts: r.last_ts,
        last_from_me: r.last_from_me,
        last_status: r.last_status,
        // 由 `fill_job_ids` 从库里补；缓存表里没有这一列。
        job_id: None,
    }
}

fn row_to_message(r: &ChatMessageRow) -> ChatMessage {
    ChatMessage {
        sender: r.sender.clone(),
        content: r.content.clone(),
        time: r.time.clone(),
        msg_id: Some(r.msg_id.clone()),
        recalled: Some(r.recalled),
    }
}

/// 为某个岗位在 Boss 上建立会话，返回这条会话的真实 uid。
///
/// 用于「还没打过招呼」的岗位：前端在此前根本拿不到 uid（会话列表里没有这一条），
/// 旧做法是给一个前端自编的占位 uid，点开必然报「找不到这条会话」。
/// 现在真去岗位页点「立即沟通」，由 Boss 服务端建会话并分配 uid。
///
/// 前端拿到 uid 后应把它当作该会话的正式标识：后续 `chat_messages` / `chat_send`
/// 都用它，并且可以把占位行替换掉。
#[tauri::command]
pub async fn chat_open(state: tauri::State<'_, ScrapeState>, job_id: String) -> CmdResult<OpenReply> {
    let job_id = job_id.trim().to_string();
    if job_id.is_empty() {
        return Err("缺少 jobId：无法建立会话".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatOpen { job_id })?;
    let uid = data
        .get("uid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if uid.is_empty() {
        return Err("没能拿到新会话的 uid".to_string());
    }
    Ok(OpenReply { uid })
}

/// 与某个 HR 的历史消息。
#[tauri::command]
pub async fn chat_messages(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, crate::commands_db::StoreState>,
    uid: String,
) -> CmdResult<MessagesReply> {
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Err("缺少 uid：请先在左侧选择一位 HR".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatMessages { uid: uid.clone() })?;
    let items: Vec<ChatMessage> = items_of(&data)?;
    // 增量追加：按 msg_id 去重写入，已有消息不重复
    let rows: Vec<ChatMessageRow> = items.iter().filter_map(message_to_row).collect();
    if let Err(e) = with_store(&store, |s| {
        s.upsert_chat_messages(BOSS, &uid, &rows).map_err(store_err)
    }) {
        eprintln!("[chat] 消息写库失败：{e}");
        crate::commands_logs::push("warn", format!("消息写库失败：{e}"));
    }
    Ok(MessagesReply { items })
}

fn message_to_row(m: &ChatMessage) -> Option<ChatMessageRow> {
    // 无 msg_id 的消息无法去重，跳过落库（仍照常返回给界面）
    let msg_id = m.msg_id.clone()?;
    Some(ChatMessageRow {
        msg_id,
        sender: m.sender.clone(),
        content: m.content.clone(),
        time: m.time.clone(),
        recalled: m.recalled.unwrap_or(false),
    })
}

/// 发送一条文本消息。
///
/// **这是唯一会真的把内容发出去的命令**。前端必须先让用户确认，不得自动触发。
///
/// 可选的 `jobId`：从岗位卡片直接发起对话时带上它，发出去之后**顺带把该岗位
/// 标记为已投递**（旧版 `chatSend(uid, text, jobId)` 的行为）。标记失败不算
/// 发送失败——消息已经在 Boss 那边了，回滚没有意义，只在回执里说明。
#[tauri::command]
pub async fn chat_send(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, crate::commands_db::StoreState>,
    uid: String,
    text: String,
    job_id: Option<String>,
) -> CmdResult<SendReply> {
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Err("缺少 uid：请先在左侧选择一位 HR".to_string());
    }
    if text.trim().is_empty() {
        return Err("消息内容为空".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatSend { uid, text })?;
    let mut marked = String::new();
    if let Some(id) = job_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        use crate::core::domain::store_ports::JobAdminStore;
        match with_store(&store, |s| {
            s.set_delivered(&[id.clone()], true).map_err(store_err)
        }) {
            Ok(n) if n > 0 => marked = id,
            Ok(_) => marked = String::new(),
            Err(e) => {
                // 不冒泡：消息已发出，把「标记失败」当成错误会让用户以为没发出去
                eprintln!("[chat] 标记已投递失败：{e}");
                crate::commands_logs::push("warn", format!("标记已投递失败：{e}"));
            }
        }
    }
    Ok(SendReply {
        text: data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        job_id: marked,
    })
}

/// 当前账号可发送的简历列表。
#[tauri::command]
pub async fn chat_resumes(
    state: tauri::State<'_, ScrapeState>,
) -> CmdResult<ResumesReply> {
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatResumes)?;
    Ok(ResumesReply {
        items: items_of(&data)?,
    })
}

/// 发送一份简历。与 `chat_send` 一样属于**真实外发动作**。
#[tauri::command]
pub async fn chat_send_resume(
    state: tauri::State<'_, ScrapeState>,
    uid: String,
    resume_id: String,
) -> CmdResult<OkReply> {
    let uid = uid.trim().to_string();
    let resume_id = resume_id.trim().to_string();
    if uid.is_empty() || resume_id.is_empty() {
        return Err("缺少 uid 或 resume_id".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatSendResume { uid, resume_id })?;
    Ok(OkReply {
        ok: data.get("ok").and_then(|v| v.as_bool()).unwrap_or(true),
    })
}

/// 撤回最近一条自己发的消息。
#[tauri::command]
pub async fn chat_recall(
    state: tauri::State<'_, ScrapeState>,
    uid: String,
) -> CmdResult<OkReply> {
    let uid = uid.trim().to_string();
    if uid.is_empty() {
        return Err("缺少 uid：请先在左侧选择一位 HR".to_string());
    }
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ChatRecall { uid })?;
    Ok(OkReply {
        ok: data.get("ok").and_then(|v| v.as_bool()).unwrap_or(true),
    })
}

/// 读取账号里配置的「求职期望」职位。
///
/// 现行版从 Chrome profile 里解 cookie 再发 HTTP（`backend/app/main.py:1241`），
/// 新版放弃 CDP 后没有必要再碰 cookie 文件：浏览器本来就开着且已登录，
/// sidecar 直接用上下文发一次请求即可。
#[tauri::command]
pub async fn list_expectations(state: tauri::State<'_, ScrapeState>) -> CmdResult<Vec<Expectation>> {
    let handle = handle_of(&state)?;
    let data = request_chat(&handle, &Command::ExpectationList)?;
    items_of(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多传字段不能让整个列表解析失败：sidecar 加字段是常事，
    /// 一旦要求逐字段严格匹配，加个新字段就会让界面整块空白。
    #[test]
    fn items_of_tolerates_extra_fields() {
        let data = serde_json::json!({
            "items": [
                {"uid": "u1", "name": "李女士", "某天新增的字段": 1},
                {"uid": "u2", "name": "王先生"}
            ]
        });
        let items: Vec<Friend> = items_of(&data).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "李女士");
    }

    /// 坏行被跳过而不是让整批失败：一条脏数据不该毁掉整个会话列表。
    #[test]
    fn items_of_skips_broken_rows() {
        let data = serde_json::json!({
            "items": [{"uid": "u1", "name": "正常"}, {"name": "缺 uid"}]
        });
        let items: Vec<Friend> = items_of(&data).unwrap();
        assert_eq!(items.len(), 1, "缺必填字段的行应被跳过，其余保留");
    }

    #[test]
    fn items_of_handles_missing_items_key() {
        let items: Vec<Friend> = items_of(&serde_json::json!({})).unwrap();
        assert!(items.is_empty());
    }

    /// 消息的 sender 只允许三个值：前端按它决定气泡靠左还是靠右。
    #[test]
    fn chat_message_sender_roundtrips() {
        let m: ChatMessage = serde_json::from_value(serde_json::json!({
            "sender": "me", "content": "您好"
        }))
        .unwrap();
        assert_eq!(m.sender, "me");
        assert_eq!(m.content, "您好");
        // 缺 time/msg_id 不应导致解析失败
        assert!(m.time.is_none());
    }
}
