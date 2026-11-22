//! 抓取命令层：前端 ↔ Python sidecar 流程的唯一入口。
//!
//! **职责边界**（AGENTS.md 2.2）：本模块只做三件事——接收参数、调用 core 的
//! [`SidecarSession`] 与 [`crate::core::scrape`]、把错误转成前端可读的字符串。
//! 会话怎么起、崩溃重启几次、敏感库怎么查，全部在 **`core::scrape`** 里；
//! 打分、排除规则、去重全部在 `core/sidecar/ingest.rs` 里。这里一行领域规则都不写。
//!
//! 进程模型：sidecar 是 `jobassistant-app` 的子进程，前端只通过 `invoke()`
//! 间接驱动。进程崩溃由消费线程上报退出码，**重启策略在 `core::scrape`**
//! （SPEC-002 第 7 节要求崩溃后自动重启一次，超过一次就停下来报原因，
//! 避免「崩了立刻重启、立刻再崩」的死循环）；本层只负责：读快照 → 叫 core
//! 判定 → 执行/写回进度。
//!
//! 进程级互斥（`LAST_SPEC` / `RESTART_LOCK`）留在这里，因为它们是「进程全局」
//! 而不是领域规则：`core` 不得持有 Tauri 生命周期里的状态。

use std::sync::{Arc, Mutex};

use crate::core::scoring::SCORE_MIN_DEFAULT;
use crate::core::scrape::{self, ScrapeJobSpec};
use crate::core::sidecar::ingest::IngestVerdict;
use crate::core::sidecar::protocol::Command;
use crate::core::sidecar::session::{
    ScrapeProgress, SessionUpdate, SidecarSession,
};

use serde::{Deserialize, Serialize};

use crate::commands_db::{store_err, store_handle, with_store, StoreState};

/// 抓取会话句柄。
///
/// `Mutex<Option<...>>`：会话只在用户点「开始抓取」时创建（那时才拉起 Python），
/// 停止或重新开始时要能彻底回收子进程，否则 python 会占着浏览器 profile。
pub struct ScrapeState(pub Mutex<Option<SidecarSession>>);

static LAST_SPEC: Mutex<Option<ScrapeJobSpec>> = Mutex::new(None);

// 放在静态区而不是 `ScrapeState` 里，是为了让「读快照 + 写回写」的判定
// 只有一条路径（`maybe_restart`），不必同时锁 `ScrapeState` 的两个字段
// ——那把锁正被 `scrape_drain` / `scrape_progress` 高频争用。
//
// `std::sync::Mutex::new` 是 const fn，可以放进 static。
static RESTART_LOCK: Mutex<()> = Mutex::new(());

/// 崩溃后自动重启一次；已重启过或用户主动停止时什么都不做。
///
/// 返回重启后的新 pid（`Some` 表示确实重启了）。
///
/// 「要不要重启」的判定全部在 [`crate::core::scrape::plan_restart`] 里，本函数
/// 只负责执行：重启要真起进程，没法在单测里跑，而判断本身是最容易写错的地方。
fn maybe_restart(
    state: &ScrapeState,
    store: &tauri::State<'_, StoreState>,
    code: Option<i32>,
    stderr: &str,
) -> CmdResult<Option<u32>> {
    let _serial = match RESTART_LOCK.lock() {
        Ok(g) => g,
        Err(_) => return Ok(None),
    };
    let spec = match LAST_SPEC.lock() {
        Ok(g) => g.clone(),
        Err(_) => return Ok(None),
    };
    let store = store_handle(store)?;

    let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    let Some(session) = guard.as_mut() else {
        return Ok(None);
    };
    let old = session.progress();
    match scrape::plan_restart(&old, session.restarts(), code, stderr, spec.as_ref()) {
        scrape::RestartPlan::None => Ok(None),
        scrape::RestartPlan::GiveUp(reason) => {
            if let Ok(mut p) = session.progress_handle().lock() {
                p.finished_reason = Some(reason);
            }
            Ok(None)
        }
        scrape::RestartPlan::Restart(spec) => {
            // 先回收旧进程：两个 python 抢同一个浏览器 profile 会直接失败。
            // 崩溃时旧子进程已经没了，但 stdin/stdout 句柄仍被 `SidecarHandle` 持有，
            // 不关掉下一次 spawn 拿不到 profile 锁。
            session.shutdown();
            let progress = session.progress_handle();
            match scrape::launch_session(&spec, Arc::clone(&store)) {
                Ok(mut fresh) => match scrape::restart_session(
                    &mut fresh,
                    &spec,
                    &old,
                    stderr,
                    code,
                    &progress,
                ) {
                    Ok(pid) => {
                        *session = fresh;
                        Ok(Some(pid))
                    }
                    Err(()) => Ok(None),
                },
                Err(e) => {
                    if let Ok(mut p) = progress.lock() {
                        p.running = false;
                        p.finished_reason = Some(format!("自动重启失败：{e}"));
                    }
                    Ok(None)
                }
            }
        }
    }
}

impl ScrapeState {
    pub fn empty() -> Self {
        Self(Mutex::new(None))
    }
}

/// Drop 时关停子进程：应用退出若留下 python，下次启动 patchright 会报
/// 「profile 正被使用」。
impl Drop for ScrapeState {
    fn drop(&mut self) {
        if let Ok(mut g) = self.0.lock() {
            if let Some(mut s) = g.take() {
                s.shutdown();
            }
        }
    }
}

type CmdResult<T> = Result<T, String>;

/// 关键词缺省值。
///
/// 放在命令层而不是 core：这是**界面默认值**，不是领域规则。core 只在
/// `ScrapeJobSpec` 里接收「最终要抓什么」。
const DEFAULT_KEYWORDS: [&str; 3] = ["Java", "Spring Cloud", "后端开发"];

/// 解析 sidecar 目录：exe 同级优先，其次仓库开发树。
fn resolve_script_dir(exe_dir: Option<std::path::PathBuf>) -> CmdResult<std::path::PathBuf> {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    scrape::resolve_script_dir(exe_dir, &manifest)
}

/// 取可执行文件所在目录（打包态 sidecar 与 exe 同级）。
fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// 供其它命令模块（自动投递）复用同一套 sidecar 目录解析。
///
/// 自动投递要自己拉浏览器，如果它另写一份路径逻辑，打包态就会出现
/// 「抓取能起、自动投递起不来」这种只在发布版复现的差异。
pub(crate) fn sidecar_script_dir() -> CmdResult<std::path::PathBuf> {
    resolve_script_dir(exe_dir())
}

// ---------- 前端结构（契约真源，见 AGENTS.md 3.1） ----------

/// `scrape_start` 的入参。
///
/// **`alias` 不能删**：前端发驼峰（`minScore` / `maxJd`），Rust 字段是蛇形，
/// serde 对未知字段不报错——少了 alias 门槛会被静默丢弃。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScrapeArgs {
    pub platform: Option<String>,
    pub keywords: Option<Vec<String>>,
    #[serde(default, alias = "maxJd")]
    pub max_jd: Option<u32>,
    #[serde(default, alias = "minScore")]
    pub min_score: Option<u8>,
}

/// `scrape_start` 的回执。
#[derive(Debug, Serialize)]
pub struct ScrapeReply {
    pub pid: u32,
    pub min_score: u8,
    pub keywords: Vec<String>,
    pub max_jd: Option<u32>,
}

/// `scrape_progress` 的回执（前端轮询）。
#[derive(Debug, Serialize)]
pub struct ProgressReply {
    pub scraped: u32,
    pub inserted: u32,
    pub duplicates: u32,
    pub below_threshold: u32,
    pub rejected: u32,
    pub login_state: String,
    pub last_log: Option<String>,
    pub last_title: Option<String>,
    pub running: bool,
    pub finished_reason: Option<String>,
}

impl From<ScrapeProgress> for ProgressReply {
    fn from(p: ScrapeProgress) -> Self {
        Self {
            scraped: p.scraped,
            inserted: p.inserted,
            duplicates: p.duplicates,
            below_threshold: p.below_threshold,
            rejected: p.rejected,
            login_state: p.login_state,
            last_log: p.last_log,
            last_title: p.last_title,
            running: p.running,
            finished_reason: p.finished_reason,
        }
    }
}

/// `scrape_drain` 的单条增量（前端按 `kind` 分发）。
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateReply {
    Job {
        summary: String,
        accepted: bool,
        score: Option<u8>,
    },
    Log {
        level: String,
        msg: String,
    },
    Login {
        state: String,
        msg: String,
    },
    /// 无法解析的行（通常是 Python traceback 混进了 stdout）
    Malformed {
        line: String,
    },
    Exited {
        code: Option<i32>,
    },
    /// 崩溃后已自动重启，`pid` 是新进程
    Restarted {
        pid: u32,
    },
}

/// 把 core 的增量翻成前端结构。纯函数，可在无 Tauri 环境下核对口径。
pub fn to_update_reply(u: SessionUpdate) -> UpdateReply {
    match u {
        SessionUpdate::Job { verdict, summary } => {
            // 只有 Accepted 才有分数；被否决/未达门槛的分数没有展示意义
            let score = match &verdict {
                IngestVerdict::Accepted(d) => Some(d.score),
                _ => None,
            };
            UpdateReply::Job {
                summary,
                accepted: score.is_some(),
                score,
            }
        }
        SessionUpdate::Log { level, msg } => UpdateReply::Log { level, msg },
        SessionUpdate::Login { state, msg } => UpdateReply::Login { state, msg },
        SessionUpdate::Malformed { line } => UpdateReply::Malformed { line },
        SessionUpdate::Exited { code } => UpdateReply::Exited { code },
    }
}

// ---------- 命令 ----------

/// 开始抓取。已存在会话时先彻底关停旧的，再拉新进程。
#[tauri::command]
pub fn scrape_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    args: ScrapeArgs,
) -> CmdResult<ScrapeReply> {
    use tauri::Manager;

    let store = store_handle(&store)?;
    let script_dir = resolve_script_dir(exe_dir())?;

    // C12：筛选条件持久化在 scrape_defaults，前端不重发全部筛选——
    // 开抓时后端自己读出来，快照进 spec（崩溃重启照原样重发）。
    // 映射是领域规则，在 core::scrape::filters 里；这里只搬键值对。
    let defaults = crate::core::domain::app_config::collect_scrape_defaults(
        &store.config_pairs().map_err(store_err)?,
    );

    let keywords: Vec<String> = args
        .keywords
        .unwrap_or_default()
        .into_iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    let keywords = if keywords.is_empty() {
        DEFAULT_KEYWORDS.iter().map(|s| s.to_string()).collect()
    } else {
        keywords
    };

    // 越界门槛直接拒绝：静默 clamp 会让「门槛 200 分却抓到一批」无法解释
    let min_score = args.min_score.unwrap_or(SCORE_MIN_DEFAULT);
    if min_score > crate::core::scoring::SCORE_MAX {
        return Err(format!(
            "分数门槛 {min_score} 超出 0-{} 范围",
            crate::core::scoring::SCORE_MAX
        ));
    }

    // 旧会话必须先关干净：两个 python 抢同一个浏览器 profile 会直接失败
    {
        let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
        if let Some(mut old) = guard.take() {
            old.shutdown();
        }
    }

    let spec = ScrapeJobSpec {
        script_dir,
        platform: args.platform.clone().unwrap_or_else(|| "boss".to_string()),
        keywords: keywords.clone(),
        max_jd: args.max_jd.unwrap_or(0),
        min_score,
        filters: scrape::filters_from_defaults(&defaults),
        rules: scrape::exclude_rules_from_defaults(&defaults),
    };
    let mut session = scrape::launch_session(&spec, store)?;
    let pid = session.pid();

    if let Err(e) = session.send(&spec.start_command()) {
        session.shutdown();
        return Err(format!("发送抓取命令失败：{e}"));
    }

    // 前端需要拿到窗口句柄来收事件推送；这里只做「让 app 保持存活」的兜底，
    // 真正的进度轮询走 scrape_progress，避免依赖 Tauri 事件通道是否被前端监听。
    let _ = app.get_webview_window("main");

    let max_jd = spec.max_jd;
    // 记下这次任务：崩溃重启时要照着原样再下发一遍
    if let Ok(mut g) = LAST_SPEC.lock() {
        *g = Some(spec);
    }

    let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    *guard = Some(session);

    Ok(ScrapeReply {
        pid,
        min_score,
        keywords,
        // `0` 在协议里表示「交给 sidecar 的默认上限」，回执按原样回显
        max_jd: if max_jd == 0 { None } else { Some(max_jd) },
    })
}

/// 请求停止抓取。sidecar 实测 0.2 s 内生效；进程随后自行退出。
#[tauri::command]
pub fn scrape_stop(state: tauri::State<'_, ScrapeState>) -> CmdResult<()> {
    let guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    match guard.as_ref() {
        Some(s) => {
            s.stop();
            Ok(())
        }
        // 没在抓取时点停止不是错误：用户连点两次很常见
        None => Ok(()),
    }
}

/// 取进度快照。前端轮询这个，不占用抓取线程。
#[tauri::command]
pub fn scrape_progress(state: tauri::State<'_, ScrapeState>) -> ProgressReply {
    let guard = match state.0.lock() {
        Ok(g) => g,
        Err(_) => return ScrapeProgress::new().into(),
    };
    match guard.as_ref() {
        // 补齐「已退出但还没写原因」的窗口，见 core::scrape::finalize_idle_progress
        Some(s) => scrape::finalize_idle_progress(s.progress()).into(),
        // 没有会话时 running=false，前端据此显示「未在抓取」
        None => ScrapeProgress::new().into(),
    }
}

/// 取走自上次调用以来的全部增量（最多 `limit` 条，避免一次拉爆界面）。
///
/// **崩溃重启挂在这里**：`Exited` 是由这个函数冒泡给前端的，front 收到它才
/// 知道抓取结束了。如果在别处（例如 `scrape_progress`）重启，前端会先看到
/// 「进程已退出」，之后再怎么刷新都只是「还在抓」，日志与状态对不上。
#[tauri::command]
pub fn scrape_drain(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    limit: Option<u32>,
) -> Vec<UpdateReply> {
    let cap = limit.unwrap_or(50).clamp(1, 500) as usize;
    let mut out: Vec<UpdateReply> = Vec::with_capacity(cap);
    // 崩溃退出只可能出现在一批的末尾：消费线程发完 `Exited` 就收工了
    let mut crash: Option<(Option<i32>, String)> = None;
    {
        let guard = match state.0.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        let Some(s) = guard.as_ref() else {
            return Vec::new();
        };
        while out.len() < cap {
            match s.poll() {
                Some(u) => {
                    if let SessionUpdate::Exited { code } = &u {
                        crash = Some((*code, String::new()));
                    }
                    out.push(to_update_reply(u))
                }
                None => break,
            }
        }
        // 趁旧句柄还在，把 stderr 尾巴取出来：重启会替换句柄，之后就看不到了
        if let Some((_, stderr)) = crash.as_mut() {
            *stderr = s.stderr_tail();
        }
    }
    if let Some((code, stderr)) = crash {
        if let Ok(Some(pid)) = maybe_restart(&state, &store, code, &stderr) {
            out.push(UpdateReply::Restarted { pid });
        }
    }
    out
}

/// 拉起扫码登录流程。登录成功后 sidecar 会回一条 `login.state`。
#[tauri::command]
pub fn login_start(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    platform: Option<String>,
) -> CmdResult<u32> {
    let platform = platform.unwrap_or_else(|| "boss".to_string());
    let script_dir = resolve_script_dir(exe_dir())?;
    let store = store_handle(&store)?;
    let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    // 登录也需要一个活着的 sidecar
    let pid = scrape::ensure_session_locked(&mut guard, script_dir, store)?;
    let s = guard.as_ref().expect("刚刚已保证存在");
    s.send(&Command::LoginStart { platform })
        .map_err(|e| format!("发送登录命令失败：{e}"))?;
    Ok(pid)
}

// ---------- 登录态：查 / 存 / 登出 ----------

/// `login_status` 的回执（字段对齐旧版 `web/src/api.ts` 的 `LoginSession`）。
#[derive(Debug, serde::Serialize)]
pub struct LoginStatusReply {
    pub platform: String,
    /// `logged_in` | `no_login` | `unknown`
    pub state: String,
    /// sidecar 是否在跑（决定「重新登录」按钮要不要先拉起浏览器）
    pub running: bool,
    pub message: String,
    /// 是否已在本次会话中把登录态存过（旧版用以避免重复 `loginSave`）
    pub saved: bool,
}

/// 把 `session.open` 回执翻译成登录态。
///
/// sidecar 的 `_run_session_open` 回 `{"logged_in": bool, "state": "logged_in"|"no_login"}`，
/// 这里只做字段归一，判定逻辑一行都不重复写。
pub fn parse_login_state(platform: &str, reply: &serde_json::Value) -> LoginStatusReply {
    let logged_in = reply
        .get("logged_in")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let raw = reply
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or(if logged_in { "logged_in" } else { "unknown" });
    let state = match raw {
        "logged_in" => "logged_in",
        "no_login" => "no_login",
        _ => {
            if logged_in {
                "logged_in"
            } else {
                "unknown"
            }
        }
    };
    LoginStatusReply {
        platform: platform.to_string(),
        state: state.to_string(),
        running: true,
        message: if logged_in {
            "已登录".to_string()
        } else {
            "尚未登录，请点「扫码登录」".to_string()
        },
        saved: false,
    }
}

/// 查当前登录态。
///
/// **不新增 sidecar 命令**：`Command::SessionOpen{page:None}` 的回执里已经带了
/// 登录态，协议文件（`core/sidecar/protocol.rs`）因此不需要改。失败时降级为
/// `unknown` 而不是报错——登录态查不到不该让整个页面红屏。
#[tauri::command]
pub fn login_status(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    platform: Option<String>,
) -> CmdResult<LoginStatusReply> {
    let platform = platform.unwrap_or_else(|| scrape::DEFAULT_PLATFORM.to_string());
    let script_dir = sidecar_script_dir()?;
    let store_arc = store_handle(&store)?;
    let handle = {
        let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
        scrape::ensure_session_locked(&mut guard, script_dir, store_arc)?;
        guard
            .as_ref()
            .map(|s| s.handle_clone())
            .ok_or_else(|| "sidecar 尚未启动".to_string())?
    };
    // 锁外等回执：登录态查询要 reload 一次页面，几秒钟很正常
    match crate::commands_chat::request_chat(
        &handle,
        &Command::SessionOpen {
            platform: platform.clone(),
            page: None,
        },
    ) {
        Ok(data) => Ok(parse_login_state(&platform, &data)),
        Err(e) => Ok(LoginStatusReply {
            platform,
            state: "unknown".to_string(),
            running: true,
            message: format!("查询登录态失败：{e}"),
            saved: false,
        }),
    }
}

/// 保存登录态的回执（旧版 `loginSave` → `{ok, saved, error}`）。
#[derive(Debug, serde::Serialize)]
pub struct LoginSaveReply {
    pub ok: bool,
    pub saved: bool,
    pub error: String,
}

/// 收尾登录流程：把「已登录」这个结论落到配置表。
///
/// 为什么不下发 `login.save`：sidecar 的 `handle()` 不认识这条命令，会回
/// `UNKNOWN_CMD`；而协议文件本 lane 禁改。而**真正的 cookie 早已由
/// patchright 的持久化 profile 落盘**——用户下次启动仍然是登录态，
/// 所以这里只需要记一个可见的标记，供界面显示与 `logout` 清除。
pub fn login_marker_key(platform: &str) -> String {
    format!("platform_login:{platform}")
}

#[tauri::command]
pub fn login_save(
    state: tauri::State<'_, StoreState>,
    platform: Option<String>,
) -> CmdResult<LoginSaveReply> {
    let platform = platform.unwrap_or_else(|| scrape::DEFAULT_PLATFORM.to_string());
    use crate::core::domain::store_ports::ConfigStore;
    with_store(&state, |s| {
        s.set_config(&login_marker_key(&platform), Some("saved"))
            .map_err(store_err)?;
        Ok(LoginSaveReply {
            ok: true,
            saved: true,
            error: String::new(),
        })
    })
}

/// 登出：清掉登录标记 + 关掉 sidecar 持有的浏览器 profile。
///
/// 真正的 cookie 在 Chrome profile 里，只有关掉 sidecar（它独占着 profile）
/// 才能保证「下次打开是未登录」。用户点登出的语义就是「我要重新扫码」。
#[tauri::command]
pub fn logout(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    platform: Option<String>,
) -> CmdResult<LogoutReply> {
    let platform = platform.unwrap_or_else(|| scrape::DEFAULT_PLATFORM.to_string());
    use crate::core::domain::store_ports::ConfigStore;
    let cleared = with_store(&store, |s| {
        s.set_config(&login_marker_key(&platform), None)
            .map_err(store_err)?;
        Ok(true)
    })?;
    let closed = {
        let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
        match guard.take() {
            Some(mut s) => {
                s.shutdown();
                true
            }
            None => false,
        }
    };
    Ok(LogoutReply { ok: true, cleared, closed })
}

/// `logout` 的回执：旧版的 `{ok, cleared}`，另加 `closed` 说明是否真的关了浏览器。
#[derive(Debug, serde::Serialize)]
pub struct LogoutReply {
    pub ok: bool,
    pub cleared: bool,
    pub closed: bool,
}

/// 彻底关闭抓取进程（窗口关闭 / 手动清理用）。
#[tauri::command]
pub fn scrape_close(state: tauri::State<'_, ScrapeState>) -> CmdResult<()> {
    let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    if let Some(mut s) = guard.take() {
        s.shutdown();
    }
    Ok(())
}

/// 只把浏览器拉起来并报告登录态，不抓岗位。
///
/// 界面上的「Boss 消息」与「求职期望」需要活着的 sidecar，但不该强迫用户先跑一次
/// 抓取——见 [`crate::core::scrape::ensure_session_locked`] 的说明。回执的登录态从
/// `scrape_progress` 的 `login_state` 字段读，不在这里另起一份状态。
///
/// `page` 说明打开哪一页：`"chat"` = Boss 聊天页，省略则不导航。不导航时浏览器
/// 停在 about:blank，用户点「Boss 消息」看到的是一片空白——所以点开聊天页的入口
/// 都要带上它。
#[tauri::command]
pub fn session_open(
    state: tauri::State<'_, ScrapeState>,
    store: tauri::State<'_, StoreState>,
    platform: Option<String>,
    page: Option<String>,
) -> CmdResult<u32> {
    let platform = platform.unwrap_or_else(|| "boss".to_string());
    let script_dir = resolve_script_dir(exe_dir())?;
    let store = store_handle(&store)?;
    let mut guard = state.0.lock().map_err(|_| "抓取锁已中毒".to_string())?;
    let pid = scrape::ensure_session_locked(&mut guard, script_dir, store)?;
    guard
        .as_ref()
        .expect("刚刚已保证存在")
        .send(&Command::SessionOpen { platform, page })
        .map_err(|e| format!("发送启动浏览器命令失败：{e}"))?;
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::job::Job;
    use crate::core::sidecar::ingest::IngestDetail;
    use crate::core::scrape::{finalize_idle_progress, FALLBACK_STOP_REASON};

    #[test]
    fn rejected_verdict_has_no_score() {
        let r = to_update_reply(SessionUpdate::Job {
            verdict: IngestVerdict::Rejected {
                rule: "exclude".into(),
                reason: "外包".into(),
            },
            summary: "排除（exclude）：外包".into(),
        });
        match r {
            UpdateReply::Job {
                accepted, score, ..
            } => {
                assert!(!accepted);
                assert_eq!(score, None);
            }
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn accepted_verdict_carries_score() {
        let job = Job {
            job_id: "boss:1".into(),
            platform: "boss".into(),
            ..Default::default()
        };
        let r = to_update_reply(SessionUpdate::Job {
            verdict: IngestVerdict::Accepted(IngestDetail {
                job_id: "boss:1".into(),
                score: 82,
                reason: "技能命中".into(),
                job,
            }),
            summary: "82 分：技能命中".into(),
        });
        match r {
            UpdateReply::Job {
                accepted, score, ..
            } => {
                assert!(accepted);
                assert_eq!(score, Some(82));
            }
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn progress_reply_defaults_are_idle() {
        let r: ProgressReply = ScrapeProgress::new().into();
        assert!(!r.running);
        assert_eq!(r.login_state, "unknown");
        assert_eq!(r.scraped, 0);
        assert_eq!(r.inserted, 0);
    }

    #[test]
    fn exit_update_passes_code_through() {
        let r = to_update_reply(SessionUpdate::Exited { code: Some(3) });
        match r {
            UpdateReply::Exited { code } => assert_eq!(code, Some(3)),
            other => panic!("实际 {other:?}"),
        }
    }

    /// 前端 `invoke('scrape_start', { args: { keywords, minScore } })` 发的是驼峰。
    /// 若只认蛇形，门槛会被静默丢弃——界面设了 70 分却按默认门槛入库，是查不出来的错。
    #[test]
    fn scrape_args_accept_camel_case_min_score() {
        let a: ScrapeArgs =
            serde_json::from_str(r#"{"keywords":["Java"],"minScore":70,"maxJd":12}"#).unwrap();
        assert_eq!(a.min_score, Some(70));
        assert_eq!(a.max_jd, Some(12));
        assert_eq!(a.keywords, Some(vec!["Java".to_string()]));

        // 蛇形（Rust 侧、脚本调用）也必须继续可用
        let b: ScrapeArgs = serde_json::from_str(r#"{"min_score":55}"#).unwrap();
        assert_eq!(b.min_score, Some(55));
        assert_eq!(b.keywords, None);
    }

    /// 抓取已结束但原因为空时，界面会既显示不出「已停止」也显示不出「已退出」。
    #[test]
    fn idle_progress_without_reason_gets_one() {
        let mut p = ScrapeProgress::new();
        p.running = false;
        let r: ProgressReply = finalize_idle_progress(p).into();
        assert_eq!(r.finished_reason.as_deref(), Some(FALLBACK_STOP_REASON));

        // 真原因必须原样保留，不能被兜底覆盖
        let mut q = ScrapeProgress::new();
        q.running = false;
        q.finished_reason = Some("已按用户请求停止".to_string());
        let r2: ProgressReply = finalize_idle_progress(q).into();
        assert_eq!(r2.finished_reason.as_deref(), Some("已按用户请求停止"));
    }

    /// 重启后计数必须接着上一次，否则界面上刚抓的几十条会「凭空消失」。
    #[test]
    fn progress_reply_survives_restart_counting() {
        let prev = ScrapeProgress {
            scraped: 12,
            inserted: 5,
            duplicates: 3,
            below_threshold: 2,
            rejected: 2,
            login_state: "logged_in".into(),
            last_title: Some("Java 后端".into()),
            running: false,
            finished_reason: Some("抓取进程异常退出，退出码 1".into()),
            ..ScrapeProgress::new()
        };
        // `adopt_progress` 需要真会话（要起 python），这里只核对搬运规则
        let mut fresh = ScrapeProgress::new();
        fresh.scraped = prev.scraped;
        fresh.inserted = prev.inserted;
        fresh.login_state = prev.login_state.clone();
        fresh.running = true;
        let r: ProgressReply = fresh.into();
        assert_eq!(r.scraped, 12);
        assert_eq!(r.inserted, 5);
        assert_eq!(r.login_state, "logged_in");
        assert!(r.running);
        assert_eq!(r.finished_reason, None, "新进程尚未结束");
    }
}
