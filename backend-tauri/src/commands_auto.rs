//! 自动投递命令层：**前端驱动**的状态机（一次 next = 一个岗位）。
//!
//! **为什么没有后台定时线程**：投递节奏必须能被用户随时暂停，而「暂停」
//! 与「正在等 20 秒」是同一个竞态的两端。后台线程要么 sleep 到点才发现被暂停
//! （用户感觉按钮慢了一拍），要么需要额外一套取消信号；而这个应用的界面
//! 本来就每 5 秒刷一次状态，让前端按 `next_at_ms` 到点再调一次 `next`
//! 更简单、更可观察，也更容易在报告里说清「上一轮为什么停」。
//!
//! **节奏规则在 core**（[`crate::core::domain::auto_greet`]）：20 秒基准 ± 5 秒
//! 随机且下限 15 秒，失败 3 次自动暂停，待投递列表空了就停。命令层只做
//! 「取岗位 → 生成话术 → 发送 → 标记已投递 → 记账」，一行规则都不写。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commands_db::{store_err, store_handle, with_store, CmdResult, LlmState, StoreState};
use crate::commands_scrape::ScrapeState;
use crate::core::domain::auto_greet::AutoGreetRuntime;
use crate::core::domain::greeting::{write_greeting, Greeting, GreetingProfile};
use crate::core::domain::job::{Job, JobQuery};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::domain::scoring_llm::LlmClient;
use crate::core::domain::job::JobStore;
use crate::core::domain::store_ports::{ConfigStore, JobAdminStore};
use crate::core::domain::token_meter::{TokenKind, TokenMeter};
use crate::core::sidecar::protocol::Command;
use serde::{Deserialize, Serialize};

/// 自动投递的进程级运行态。
pub struct AutoGreetState(pub Mutex<AutoGreetRuntime>);

impl AutoGreetState {
    pub fn empty() -> Self {
        Self(Mutex::new(AutoGreetRuntime::default()))
    }
}

impl Default for AutoGreetState {
    fn default() -> Self {
        Self::empty()
    }
}

/// 随机源：xorshift64*，用 `AtomicU64` 而不是 `rand` crate。
///
/// 不能新增依赖（AGENTS.md / 本 lane 的写权限：`Cargo.toml` 只读），
/// 而「20 秒 ±5 秒」对随机质量的要求极低——只要不是每次都同一个数就行。
static RNG: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// 返回 [0, 1) 的伪随机数。
pub fn random_roll() -> f64 {
    let mut x = RNG.load(Ordering::Relaxed);
    // 种子固定也能跑，但连续调用必须给出不同的值：先推进再取高 53 位
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    RNG.store(x, Ordering::Relaxed);
    let bits = x >> 11; // 53 位
    (bits as f64) / ((1u64 << 53) as f64)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 一次投递的入参。`jobId` 省略时由 Rust 自己挑下一个待投递岗位。
#[derive(Debug, Default, Deserialize)]
pub struct AutoNextArgs {
    #[serde(default, alias = "jobId")]
    pub job_id: Option<String>,
}

/// 一个待投递岗位的摘要（旧版 `AutoGreetJob` 的对齐版）。
#[derive(Debug, Clone, Serialize)]
pub struct AutoJob {
    pub job_id: String,
    pub title: String,
    pub company: String,
    pub salary: String,
    pub score: Option<u8>,
    pub hr_name: String,
    pub job_url: String,
}

impl From<&Job> for AutoJob {
    fn from(j: &Job) -> Self {
        Self {
            job_id: j.job_id.clone(),
            title: j.title.clone().unwrap_or_default(),
            company: j.company.clone().unwrap_or_default(),
            salary: j.salary.clone().unwrap_or_default(),
            score: j.match_score,
            hr_name: j.hr_name.clone().unwrap_or_default(),
            job_url: j.job_url.clone().unwrap_or_default(),
        }
    }
}

/// 一次投递的结果。
///
/// `stage` 让前端能区分「没岗位了」「没找到聊天入口」「发出去被拒」，
/// 而不是只看到一个 `ok:false` 就去猜。
#[derive(Debug, Serialize)]
pub struct AutoNextReply {
    /// 本次是否真的发出去了
    pub ok: bool,
    /// `pick` / `greet` / `send` / `mark` / `done`
    pub stage: String,
    pub error: String,
    /// 是否跳过（例如岗位已被别人聊过）；跳过不算失败
    pub skip: bool,
    pub job_id: String,
    pub company: String,
    pub title: String,
    /// 发出去的正文
    pub text: String,
    /// 生成话术的来源：`rule` / `llm`
    pub source: String,
    pub note: String,
    pub tokens: u32,
    /// 下一次该在什么时刻（Unix 毫秒）再调本命令；0 表示不必再调
    pub next_at_ms: u64,
    /// 距离下次调用的剩余毫秒（前端可直接拿去做倒计时）
    pub remaining_ms: u64,
    /// 已发条数（累计）
    pub sent: u32,
}

/// 自动投递运行状态（前端每 5 秒拉一次）。
#[derive(Debug, Serialize)]
pub struct AutoStatusReply {
    pub running: bool,
    pub paused: bool,
    pub sent: u32,
    pub failures: u32,
    pub next_at_ms: u64,
    pub remaining_ms: u64,
    pub stop_reason: String,
    /// 待投递队列里还剩多少条（用于界面上「还剩 N 个」）
    pub pending: i64,
}

/// 挑出下一个待投递岗位：`status=pending`、未被否决、按分数降序。
///
/// 排序键必须是 `score`：用户配好门槛后才开始自动投递，先投分最高的那几个。
pub fn pick_next_pending(store: &dyn JobStore<Error = crate::core::infra::StoreError>) -> Option<Job> {
    let mut query = JobQuery::for_tab("pending");
    query.sort = crate::core::domain::job::SortField::Score;
    query.ascending = false;
    query.limit = 1;
    store
        .list_jobs(&query)
        .ok()
        .and_then(|mut v| v.pop())
}

/// 话术生成：与 `greet_draft` 同源（同一份提示词 + 同一份画像）。
pub fn draft_for(
    store: &dyn ConfigStore<Error = crate::core::infra::StoreError>,
    client: Option<&dyn LlmClient>,
    job: &Job,
) -> Greeting {
    let prompt_raw = store.get_config("greet_prompt", "").unwrap_or_default();
    let prompt = if prompt_raw.trim().is_empty() {
        crate::core::domain::greeting::DEFAULT_PROMPT.to_string()
    } else {
        prompt_raw
    };
    let profile = crate::commands_greet::profile_from_store(store);
    write_greeting(client, Some(&prompt), job, &profile)
}

/// 取一个**已经活着**的 sidecar 会话的句柄；没有就直接报错。
///
/// **刻意不调用 `ensure_session_locked`**：自动投递是高频循环（前端每 1 秒一拍），
/// 由它负责拉起会话意味着「没有 sidecar 就顺手起一个」——那会连带启动 Chrome。
/// 用户在「自动投递」里点开始，期待的是往已有会话里发消息，而不是眼前突然
/// 弹出一个浏览器窗口。浏览器只由主动入口拉起：`login_start` / `scrape_start` /
/// `session_open`。
fn ensure_handle(
    scrape: &ScrapeState,
    store_state: &StoreState,
) -> CmdResult<crate::core::sidecar::process::SidecarHandle> {
    let _ = store_state;
    let guard = scrape
        .0
        .lock()
        .map_err(|_| "抓取会话锁已中毒".to_string())?;
    let session = guard.as_ref().ok_or_else(|| {
        "浏览器尚未启动：请先扫码登录或开始抓取，再开启自动投递".to_string()
    })?;
    let handle = session.handle_clone();
    if !handle.is_alive() {
        return Err("浏览器进程已退出：请先重新扫码登录，再开启自动投递".to_string());
    }
    Ok(handle)
}

/// 当前会话报告的登录态；没有会话视为未登录。
///
/// 自动投递的前置条件：**登录有效**。用 sidecar 自己探测出的 `login_state`
/// （`login.state` 事件或 `login_status` 写回）作为唯一真源，不在命令层另判。
fn logged_in_now(scrape: &ScrapeState) -> bool {
    let Ok(guard) = scrape.0.lock() else {
        return false;
    };
    match guard.as_ref() {
        Some(s) => s.progress().login_state == "logged_in",
        None => false,
    }
}

/// 找岗位对应的聊天入口：`ChatOpen{query}` → 回执里的 uid。
fn open_chat(handle: &crate::core::sidecar::process::SidecarHandle, job: &Job) -> Result<String, String> {
    let query = job
        .company
        .clone()
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| job.title.clone().unwrap_or_default());
    if query.trim().is_empty() {
        return Err("岗位缺少公司名与标题，无法定位聊天会话".to_string());
    }
    let reply = crate::commands_chat::request_chat(handle, &Command::ChatOpen {
        query: query.clone(),
    })?;
    for key in ["uid", "encrypt_uid", "encryptUid"] {
        if let Some(uid) = reply.get(key).and_then(|v| v.as_str()) {
            if !uid.trim().is_empty() {
                return Ok(uid.to_string());
            }
        }
    }
    Err(format!("没有在 Boss 消息列表里找到「{query}」的会话"))
}

/// 执行一次自动投递。
#[tauri::command]
pub fn auto_greet_next(
    state: tauri::State<'_, AutoGreetState>,
    scrape: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    llm: tauri::State<'_, LlmState>,
    args: AutoNextArgs,
) -> CmdResult<AutoNextReply> {
    let now = now_ms();
    // 0) 登录前置：**登录无效就不投**。
    //
    // 放在认领之前，是为了不留半截状态：未登录时连 `rt.start()` 都不该发生，
    // 否则界面会显示「正在投递」却一条也发不出去。也不在这里去拉起浏览器——
    // 那是 `login_start` / `scrape_start` 的职责，自动投递只消费已有会话。
    if !logged_in_now(&scrape) {
        let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
        rt.pause("登录已失效，自动投递已暂停");
        return Ok(AutoNextReply {
            ok: false,
            stage: "pick".into(),
            error: "登录无效或已过期，请先扫码登录再开启自动投递".into(),
            skip: false,
            job_id: String::new(),
            company: String::new(),
            title: String::new(),
            text: String::new(),
            source: String::new(),
            note: String::new(),
            tokens: 0,
            next_at_ms: 0,
            remaining_ms: rt.remaining_ms(now),
            sent: rt.sent,
        });
    }
    // 1) 认领一轮：暂停中直接拒绝，避免前端定时器在暂停后仍然投
    {
        let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
        if rt.paused {
            return Ok(AutoNextReply {
                ok: false,
                stage: "pick".into(),
                error: "自动投递已暂停".into(),
                skip: false,
                job_id: String::new(),
                company: String::new(),
                title: String::new(),
                text: String::new(),
                source: String::new(),
                note: String::new(),
                tokens: 0,
                next_at_ms: 0,
                remaining_ms: 0,
                sent: rt.sent,
            });
        }
        if !rt.is_active() {
            rt.start(now);
        }
    }

    // 2) 取岗位：显式 jobId 优先，否则按分数挑下一个
    let store_arc = store_handle(&store)?;
    let job = match args.job_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => store_arc.get_job(id).map_err(store_err)?.ok_or_else(|| {
            format!("岗位 {id} 不在库里")
        })?,
        None => match pick_next_pending(&*store_arc) {
            Some(j) => j,
            None => {
                let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
                rt.finish_exhausted();
                return Ok(AutoNextReply {
                    ok: false,
                    stage: "done".into(),
                    error: rt.stop_reason.clone(),
                    skip: false,
                    job_id: String::new(),
                    company: String::new(),
                    title: String::new(),
                    text: String::new(),
                    source: String::new(),
                    note: String::new(),
                    tokens: 0,
                    next_at_ms: 0,
                    remaining_ms: 0,
                    sent: rt.sent,
                });
            }
        },
    };
    let summary = AutoJob::from(&job);

    // 3) 生成话术
    let greeting = {
        let guard = llm.0.lock().ok();
        let bare: Option<&dyn LlmClient> = guard.as_ref().and_then(|g| g.as_ref()).map(|c| c as &dyn LlmClient);
        // 计费口径的累计装在客户端边界（ADR-005），`draft_for` 本体不动。
        let warn = |m: &str| crate::commands_logs::push("warn", m);
        let meter = bare.map(|c| TokenMeter::new(c, &*store_arc, TokenKind::Greet, &warn));
        let client: Option<&dyn LlmClient> = match meter.as_ref() {
            Some(m) => Some(m as &dyn LlmClient),
            None => bare,
        };
        draft_for(&*store_arc, client, &job)
    };

    // 4) 找聊天入口并发出去；任何失败都记一次 failure，满 3 次自动暂停
    let sent_text = match ensure_handle(&scrape, &store).and_then(|handle: crate::core::sidecar::process::SidecarHandle| {
        let uid = open_chat(&handle, &job)?;
        send_greeting_handle(&handle, &uid, &greeting.text)
    }) {
        Ok(text) => text,
        Err(e) => {
            let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
            let gave_up = rt.record_failure();
            crate::commands_logs::push("warn", format!("自动投递失败（{}）：{e}", summary.company));
            if gave_up {
                rt.pause("连续失败 3 次，已自动暂停");
            }
            return Ok(AutoNextReply {
                ok: false,
                stage: "send".into(),
                error: e,
                skip: false,
                job_id: summary.job_id,
                company: summary.company,
                title: summary.title,
                text: greeting.text,
                source: greeting.source.clone(),
                note: greeting.note.clone(),
                tokens: greeting.tokens,
                next_at_ms: 0,
                remaining_ms: rt.remaining_ms(now),
                sent: rt.sent,
            });
        }
    };

    // 5) 标记已投递
    with_store(&store, |s| {
        s.set_delivered(&[summary.job_id.clone()], true)
            .map_err(store_err)
    })?;

    // 6) 记账 + 预算下次节奏
    let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
    rt.record_sent(now, random_roll());
    crate::commands_logs::push(
        "info",
        format!("自动投递已发送：{} · {}", summary.company, summary.title),
    );
    Ok(AutoNextReply {
        ok: true,
        stage: "mark".into(),
        error: String::new(),
        skip: false,
        job_id: summary.job_id,
        company: summary.company,
        title: summary.title,
        text: sent_text,
        source: greeting.source,
        note: greeting.note,
        tokens: greeting.tokens,
        next_at_ms: rt.next_at_ms,
        remaining_ms: rt.remaining_ms(now),
        sent: rt.sent,
    })
}

fn send_greeting_handle(
    handle: &crate::core::sidecar::process::SidecarHandle,
    uid: &str,
    text: &str,
) -> Result<String, String> {
    let reply = crate::commands_chat::request_chat(
        handle,
        &Command::ChatSend {
            uid: uid.to_string(),
            text: text.to_string(),
        },
    )?;
    Ok(reply
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or(text)
        .to_string())
}

/// 暂停自动投递。
#[tauri::command]
pub fn auto_greet_pause(state: tauri::State<'_, AutoGreetState>, reason: Option<String>) -> CmdResult<bool> {
    let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
    rt.pause(reason.as_deref().unwrap_or("用户已暂停"));
    Ok(true)
}

/// 继续自动投递：会清空失败计数，并重新起算一次间隔。
#[tauri::command]
pub fn auto_greet_resume(state: tauri::State<'_, AutoGreetState>) -> CmdResult<u64> {
    let now = now_ms();
    let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
    rt.resume(now);
    Ok(rt.next_at_ms)
}

/// 读状态。`pending` 现查一次，供界面显示「还剩 N 个」。
#[tauri::command]
pub fn auto_greet_status(
    state: tauri::State<'_, AutoGreetState>,
    store: tauri::State<'_, StoreState>,
) -> CmdResult<AutoStatusReply> {
    let now = now_ms();
    let pending = with_store(&store, |s| {
        s.count_jobs(&JobQuery::for_tab("pending")).map_err(store_err)
    })
    .unwrap_or(0);
    let rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
    Ok(AutoStatusReply {
        running: rt.is_active(),
        paused: rt.paused,
        sent: rt.sent,
        failures: rt.failures,
        next_at_ms: rt.next_at_ms,
        remaining_ms: rt.remaining_ms(now),
        stop_reason: rt.stop_reason.clone(),
        pending,
    })
}

/// 重置运行态（一轮投递结束后用户点「重新开始」）。
#[tauri::command]
pub fn auto_greet_reset(state: tauri::State<'_, AutoGreetState>) -> CmdResult<bool> {
    let mut rt = state.0.lock().map_err(|_| "自动投递状态锁已中毒".to_string())?;
    *rt = AutoGreetRuntime::default();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::auto_greet::{
        next_gap_ms, AUTO_GAP_MIN_MS, AUTO_GAP_MS, AUTO_MAX_FAILURES,
    };
    use crate::core::infra::SqliteStore;

    #[test]
    fn roll_is_in_unit_interval_and_varies() {
        let a = random_roll();
        let b = random_roll();
        assert!((0.0..1.0).contains(&a));
        assert!((0.0..1.0).contains(&b));
        assert!(a != b, "两次随机不应相同，否则间隔会恒定");
    }

    #[test]
    fn gap_never_below_fifteen_seconds() {
        // 旧版 AUTO_GAP_MS=20000 ±5s 的下沿即 15s；这是反爬红线，不得下调
        for roll in [0.0, 0.001, 0.5, 0.999, 1.0] {
            let gap = next_gap_ms(roll);
            assert!(gap >= AUTO_GAP_MIN_MS, "间隔 {gap}ms 低于下限 {AUTO_GAP_MIN_MS}ms");
            assert!(gap <= AUTO_GAP_MS + 5_000, "间隔 {gap}ms 超过上限");
        }
        assert!(next_gap_ms(f64::NAN) >= AUTO_GAP_MIN_MS);
    }

    #[test]
    fn auto_next_args_accept_camel_job_id() {
        let camel: AutoNextArgs = serde_json::from_value(serde_json::json!({"jobId": "abc"}))
            .expect("驼峰 jobId 应可解析");
        assert_eq!(camel.job_id.as_deref(), Some("abc"));
        let snake: AutoNextArgs = serde_json::from_value(serde_json::json!({"job_id": "abc"}))
            .expect("蛇形也应可解析");
        assert_eq!(snake.job_id, camel.job_id);
        let empty: AutoNextArgs = serde_json::from_value(serde_json::json!({}))
            .expect("全空应取默认值");
        assert!(empty.job_id.is_none(), "省略 jobId 表示由后端挑下一个");
    }

    #[test]
    fn three_failures_pause_the_loop() {
        let mut rt = AutoGreetRuntime::default();
        rt.start(1_000);
        for i in 1..=AUTO_MAX_FAILURES {
            let gave_up = rt.record_failure();
            assert_eq!(gave_up, i == AUTO_MAX_FAILURES, "只在第 {AUTO_MAX_FAILURES} 次失败时放弃");
        }
        rt.pause("连续失败 3 次，已自动暂停");
        assert!(rt.paused);
        assert!(!rt.is_active());
    }

    #[test]
    fn pick_next_pending_prefers_highest_score_and_skips_rejected() {
        let store = SqliteStore::open_in_memory().expect("内存库应可打开");
        let mk = |id: &str, score: Option<u8>, rejected: bool, status: &str| Job {
            job_id: id.to_string(),
            platform: "boss".to_string(),
            title: Some(format!("岗位{id}")),
            company: Some("某公司".to_string()),
            match_score: score,
            rejected: if rejected { 1 } else { 0 },
            status: status.to_string(),
            created_at: now_ms() as i64,
            ..Default::default()
        };
        for job in [
            mk("low", Some(60), false, "pending"),
            mk("high", Some(95), false, "pending"),
            mk("rejected", Some(99), true, "pending"),
            mk("done", Some(98), false, "delivered"),
        ] {
            store.insert_job(&job).expect("写入应成功");
        }
        let next = pick_next_pending(&store).expect("应挑出一个岗位");
        assert_eq!(next.job_id, "high", "要挑分最高且未被否决、未投递的那个");
    }

    #[test]
    fn pick_next_pending_returns_none_when_queue_empty() {
        let store = SqliteStore::open_in_memory().expect("内存库应可打开");
        assert!(pick_next_pending(&store).is_none(), "空队列必须返回 None 而不是报错");
    }

    #[test]
    fn exhausted_queue_stops_with_readable_reason() {
        let mut rt = AutoGreetRuntime::default();
        rt.start(1_000);
        rt.finish_exhausted();
        assert!(!rt.is_active(), "队列空了就该停，不要空转到天亮");
        assert!(rt.stop_reason.contains("待投递"), "停止原因要能给用户看：{}", rt.stop_reason);
    }

    #[test]
    fn draft_for_falls_back_to_rule_without_client() {
        let store = SqliteStore::open_in_memory().expect("内存库应可打开");
        let job = Job {
            job_id: "j1".into(),
            platform: "boss".into(),
            title: Some("Java 后端开发".into()),
            company: Some("某科技".into()),
            ..Default::default()
        };
        let g = draft_for(&store, None, &job);
        assert_eq!(g.source, "rule", "没配密钥时必须走规则话术");
        assert!(!g.text.trim().is_empty());
    }
}
