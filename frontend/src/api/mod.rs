//! Tauri 命令的 Rust 封装。
//!
//! **为什么不用 `tauri::` crate**：前端编译成 WASM，跑在 WebView 里，而
//! `tauri::` 是 host 侧 crate（依赖 wry/winit，无法编到 wasm32）。浏览器里的
//! 调用约定只有一个：`window.__TAURI_INTERNALS__.invoke(cmd, args)`。
//!
//! 这里手写薄封装而不引第三方 IPC crate，理由是参数形状已在
//! `commands_*.rs` 里用 `#[serde(alias)]` 定死（驼峰），多一层封装反而
//! 会让「参数名写错」变成运行时静默失败。

pub mod types;

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use self::types::*;

#[wasm_bindgen]
extern "C" {
    /// Tauri 2 注入到 window 上的 IPC 入口。
    #[wasm_bindgen(js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = invoke)]
    fn tauri_invoke_raw(cmd: &str, args: JsValue) -> js_sys::Promise;
}

/// 调一个 Tauri 命令。
///
/// 失败时把 JS 异常转成可读字符串。命令层返回的是 `Result<T, String>`，
/// Tauri 会把 Err 变成 rejected promise，`JsValue::as_string()` 直接拿到那句中文。
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, String> {
    let payload = serde_wasm_bindgen::to_value(&args).map_err(|e| format!("参数序列化失败：{e}"))?;
    let promise = tauri_invoke_raw(cmd, payload);
    let value = JsFuture::from(promise)
        .await
        .map_err(|e| js_error_text(&e))?;
    serde_wasm_bindgen::from_value(value).map_err(|e| format!("{cmd} 返回值解析失败：{e}"))
}

fn js_error_text(e: &JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    // 命令层 Err(String) 会作为普通字符串抛出；其余情况交给 JS 侧 String()
    js_sys::Error::from(e.clone()).message().into()
}

macro_rules! no_args {
    ($($name:ident => $cmd:literal -> $ret:ty),* $(,)?) => {
        $(
            pub async fn $name() -> Result<$ret, String> {
                invoke($cmd, serde_json::json!({})).await
            }
        )*
    };
}

no_args! {
    app_info        => "app_info"          -> AppInfo,
    scoring_spec    => "get_scoring_spec"  -> ScoringSpec,
    llm_status      => "llm_status"        -> LlmStatus,
    db_overview     => "db_overview"       -> DbOverview,
    list_jobs_default => "list_jobs"       -> Vec<Job>,
}

pub async fn list_jobs(args: ListJobsArgs) -> Result<Vec<Job>, String> {
    invoke("list_jobs", args).await
}

pub async fn get_job(job_id: &str) -> Result<Job, String> {
    invoke("get_job", serde_json::json!({ "jobId": job_id })).await
}

pub async fn migrate_scores() -> Result<bool, String> {
    invoke("migrate_scores", serde_json::json!({})).await
}

pub async fn score_job(args: ScoreArgs) -> Result<ScoreReply, String> {
    invoke("score_job", args).await
}

/// 全量重算的一批。`afterJobId` 是驼峰（命令层用 serde alias 接受），
/// 必须是 `null` 而不是缺省——缺省会与「从头开始」混淆。
pub async fn rescore_jobs(args: RescoreArgs) -> Result<RescoreReply, String> {
    invoke(
        "rescore_jobs",
        serde_json::json!({
            "args": {
                "afterJobId": args.after_job_id,
                "limit": args.limit,
                "useLlm": args.use_llm,
                "llmPrefilter": args.llm_prefilter,
                "logChanges": args.log_changes,
            }
        }),
    )
    .await
}

pub async fn scrape_progress() -> Result<ScrapeProgress, String> {
    invoke("scrape_progress", serde_json::json!({})).await
}

pub async fn scrape_drain(limit: u32) -> Result<Vec<ScrapeUpdate>, String> {
    invoke("scrape_drain", serde_json::json!({ "limit": limit })).await
}

pub async fn scrape_start(
    platform: &str,
    keywords: Vec<String>,
    max_jd: u32,
    min_score: u8,
) -> Result<ScrapeReply, String> {
    invoke(
        "scrape_start",
        serde_json::json!({
            "platform": platform,
            "keywords": keywords,
            "maxJd": max_jd,
            "minScore": min_score,
        }),
    )
    .await
}

no_args! {
    scrape_stop => "scrape_stop" -> (),
    scrape_close => "scrape_close" -> (),
    chat_resumes => "chat_resumes" -> ResumesReply,
}

pub async fn login_start(platform: &str) -> Result<u32, String> {
    invoke("login_start", serde_json::json!({ "platform": platform })).await
}

pub async fn session_open(platform: &str, page: Option<&str>) -> Result<u32, String> {
    let args = match page {
        Some(p) => serde_json::json!({ "platform": platform, "page": p }),
        None => serde_json::json!({ "platform": platform }),
    };
    invoke("session_open", args).await
}

pub async fn chat_friends() -> Result<FriendsReply, String> {
    invoke("chat_friends", serde_json::json!({})).await
}

pub async fn chat_messages(uid: &str) -> Result<MessagesReply, String> {
    invoke("chat_messages", serde_json::json!({ "uid": uid })).await
}

/// 发一条消息给 HR。
///
/// `job_id` 非空时后端会**顺带把该岗位标记为已投递**（旧版 `chatSend(uid, text, jobId)`），
/// 这是「聊过就算投过」的唯一入口——不要在前端再补一次 `mark_delivered`。
/// 回执的 `text` 是**真正发出去的正文**（后端可能截断），列表要显示它而不是本地输入。
pub async fn chat_send(uid: &str, text: &str, job_id: Option<&str>) -> Result<SendReply, String> {
    let args = match job_id {
        Some(id) if !id.trim().is_empty() => serde_json::json!({ "uid": uid, "text": text, "jobId": id }),
        _ => serde_json::json!({ "uid": uid, "text": text }),
    };
    invoke("chat_send", args).await
}

// ---------- 敏感库 ----------

/// 列出敏感库条目。`kind` 取 `all` / `stored` / `rejected`（拼错按 `all` 处理）。
pub async fn sensitive_list(
    platform: Option<&str>,
    keyword: Option<&str>,
    kind: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<SensitivePage, String> {
    let mut args = serde_json::Map::new();
    if let Some(p) = platform {
        args.insert("platform".into(), serde_json::json!(p));
    }
    if let Some(k) = keyword {
        args.insert("keyword".into(), serde_json::json!(k));
    }
    if let Some(k) = kind {
        args.insert("kind".into(), serde_json::json!(k));
    }
    if let Some(l) = limit {
        args.insert("limit".into(), serde_json::json!(l));
    }
    if let Some(o) = offset {
        args.insert("offset".into(), serde_json::json!(o));
    }
    invoke("sensitive_list", serde_json::json!({ "args": args })).await
}

/// 把所选条目移出敏感库，返回**实际删除**的条数（不存在/重复的 id 不计数）。
pub async fn sensitive_remove(ids: Vec<i64>) -> Result<usize, String> {
    if ids.is_empty() {
        // 空选不该真跑一次 SQL；后端也会回 0，但这里更省一次 IPC
        return Ok(0);
    }
    let reply: RemoveReply = invoke("sensitive_remove", serde_json::json!({ "args": { "ids": ids } })).await?;
    Ok(reply.deleted)
}

// ---------- 投递状态 ----------

async fn set_job_flag(cmd: &str, job_ids: Vec<String>) -> Result<usize, String> {
    if job_ids.is_empty() {
        return Ok(0);
    }
    invoke(cmd, serde_json::json!({ "args": { "jobIds": job_ids } })).await
}

/// 标为「不投递」（页签 `rejected`）。返回受影响条数。
pub async fn reject_jobs(job_ids: Vec<String>) -> Result<usize, String> {
    set_job_flag("reject_jobs", job_ids).await
}

/// 把岗位从「不投递」放回待投递。返回受影响条数。
pub async fn allow_jobs(job_ids: Vec<String>) -> Result<usize, String> {
    set_job_flag("allow_jobs", job_ids).await
}

/// 标为「已投递」（页签 `delivered`）。返回受影响条数。
pub async fn mark_delivered(job_ids: Vec<String>) -> Result<usize, String> {
    set_job_flag("mark_delivered", job_ids).await
}

// ---------- 页签与排序 ----------

/// 三个页签的条数。列表页每次刷新后调一次（旧版 5 秒轮询）。
pub async fn job_tab_counts() -> Result<JobTabCounts, String> {
    invoke("job_tab_counts", serde_json::json!({})).await
}

// ---------- 自动投递 ----------

/// 投递下一个岗位：取待投递队首 → 生成话术 → 发送 → 标记已投递。
///
/// 后端每调一次最多发一条，回执带 `next_at_ms`；调用方要按它安排下一次调用
/// （节奏与随机间隔是业务规则，在后端，前端**不得**自己算多长）。
/// 传 `job_id` 可以指定岗位，不传就由后端挑分数最高的待投递岗位。
pub async fn auto_greet_next(job_id: Option<&str>) -> Result<AutoNextReply, String> {
    match job_id {
        Some(id) if !id.trim().is_empty() => {
            invoke("auto_greet_next", serde_json::json!({ "args": { "jobId": id } })).await
        }
        _ => invoke("auto_greet_next", serde_json::json!({ "args": {} })).await,
    }
}

pub async fn auto_greet_pause(reason: Option<&str>) -> Result<bool, String> {
    match reason {
        Some(r) if !r.trim().is_empty() => {
            invoke("auto_greet_pause", serde_json::json!({ "reason": r })).await
        }
        _ => invoke("auto_greet_pause", serde_json::json!({})).await,
    }
}

/// 恢复自动投递，返回下一次该调用的时刻（Unix 毫秒）。
pub async fn auto_greet_resume() -> Result<u64, String> {
    invoke("auto_greet_resume", serde_json::json!({})).await
}

pub async fn auto_greet_status() -> Result<AutoStatusReply, String> {
    invoke("auto_greet_status", serde_json::json!({})).await
}

/// 停止并清零计数（用户点「停止」时用；`pause` 只是暂停）。
pub async fn auto_greet_reset() -> Result<bool, String> {
    invoke("auto_greet_reset", serde_json::json!({})).await
}

// ---------- 配置 ----------

pub async fn get_app_config() -> Result<AppConfigReply, String> {
    invoke("get_app_config", serde_json::json!({})).await
}

/// 保存配置。**只提交 diff**：没改的键不要放进来。
///
/// 尤其不要把 `api_key_hint`（`****abcd`）当成 `api_key` 回传——后端虽然会
/// 跳过以 `****` 开头的值，但那是一道兜底，不是许可。
pub async fn set_app_config(patch: serde_json::Value) -> Result<SaveConfigReply, String> {
    invoke("set_app_config", serde_json::json!({ "patch": patch })).await
}

/// 保存抓取筛选面板的默认条件（`scrape_defaults.*`，14 项）。
///
/// 薄转发：`args` 就是命令层的 `ScrapeDefaultsArgs`，**只提交变化过的字段**——
/// 缺席的键在 Rust 侧是 `None`，表示「不动」，不会被空值覆盖。
/// 键名一律驼峰（`maxPages` / `companyScale` …），与 `commands_config.rs` 的
/// `alias` 对齐；回读走 `get_app_config().scrape_defaults`。
pub async fn set_scrape_defaults(
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    invoke("set_scrape_defaults", serde_json::json!({ "args": args })).await
}

/// 写「有效 JD 上限」与「打分门槛」。门槛只能走这里（`set_app_config` 会忽略它）。
pub async fn set_limit(max_jd: Option<u32>, min_score: Option<u8>) -> Result<LimitReply, String> {
    let mut args = serde_json::Map::new();
    if let Some(v) = max_jd {
        args.insert("maxJd".into(), serde_json::json!(v));
    }
    if let Some(v) = min_score {
        args.insert("minScore".into(), serde_json::json!(v));
    }
    invoke("set_limit", serde_json::json!({ "args": args })).await
}

// ---------- 登录态 ----------

pub async fn login_status(platform: &str) -> Result<LoginStatus, String> {
    invoke("login_status", serde_json::json!({ "platform": platform })).await
}

/// 登录成功后的收尾：把「已登录」落到配置表。
///
/// 真正的 cookie 由 patchright 的持久化 profile 落盘，这里只记一个可见标记；
/// 未登录时回 `ok:false` + 一句可读的 `error`，**不会**抛异常。
pub async fn login_save(platform: &str) -> Result<LoginSaveReply, String> {
    invoke("login_save", serde_json::json!({ "platform": platform })).await
}

/// 退出登录：清掉登录标记并关掉 sidecar 浏览器会话。
pub async fn logout(platform: &str) -> Result<LogoutReply, String> {
    invoke("logout", serde_json::json!({ "platform": platform })).await
}

// ---------- 日志 ----------

/// 取 `since`（Unix 毫秒）之后的日志。首次调用传 `None`（= 全量）。
/// 回执的 `now` 是**本次游标**，下次必须带回来，否则每次都会拉全量。
pub async fn list_logs(since: Option<u64>) -> Result<LogsReply, String> {
    match since {
        Some(s) => invoke("list_logs", serde_json::json!({ "since": s })).await,
        None => invoke("list_logs", serde_json::json!({})).await,
    }
}

pub async fn clear_logs() -> Result<bool, String> {
    invoke("clear_logs", serde_json::json!({})).await
}

// ---------- 决策记录 ----------

/// 读决策记录（新→旧）。`after_id` 传上一页最小的 id 继续往前翻。
pub async fn list_decisions(limit: Option<u32>, after_id: Option<i64>) -> Result<Vec<DecisionRecord>, String> {
    let mut args = serde_json::Map::new();
    if let Some(l) = limit {
        args.insert("limit".into(), serde_json::json!(l));
    }
    if let Some(a) = after_id {
        args.insert("afterId".into(), serde_json::json!(a));
    }
    invoke("list_decisions", serde_json::json!({ "args": args })).await
}

pub async fn chat_send_resume(uid: &str, resume_id: &str) -> Result<(), String> {
    invoke::<serde_json::Value>(
        "chat_send_resume",
        serde_json::json!({ "uid": uid, "resumeId": resume_id }),
    )
    .await
    .map(|_| ())
}

pub async fn chat_recall(uid: &str) -> Result<(), String> {
    invoke::<serde_json::Value>("chat_recall", serde_json::json!({ "uid": uid }))
        .await
        .map(|_| ())
}

pub async fn greet_draft(job_id: &str, title: &str, company: &str, salary: &str, experience: &str, jd: &str) -> Result<GreetReply, String> {
    invoke(
        "greet_draft",
        serde_json::json!({
            "args": {
                "jobId": job_id,
                "title": title,
                "company": company,
                "salary": salary,
                "experience": experience,
                "jd": jd,
                "useLlm": true,
            }
        }),
    )
    .await
}

pub async fn greet_one(uid: &str, text: &str) -> Result<String, String> {
    invoke("greet_one", serde_json::json!({ "uid": uid, "text": text })).await
}

pub async fn list_expectations() -> Result<Vec<Expectation>, String> {
    invoke("list_expectations", serde_json::json!({})).await
}
