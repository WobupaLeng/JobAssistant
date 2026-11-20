//! 抓取编排（2026-11-21 从 `commands_scrape.rs` 下沉）。
//!
//! **为什么在 core 而不是命令层**（AGENTS.md 2.2）：命令层只做「收参数 → 调 core
//! → 错误转字符串」。此前 `ensure_session_locked` / `make_sink` / `make_lookup` /
//! `launch_session` / 崩溃重启的执行逻辑全写在 `commands_scrape.rs` 里，等于把
//! 「会话怎么起、重启几次、敏感库怎么查」当成了视图层细节——换一个入口（登录、
//! 聊天页、求职期望）就得复制一遍，历史上正是因为这样，只有「开始抓取」能拉起
//! sidecar，想看一眼 Boss 消息也被人被迫先跑一次抓取。
//!
//! **本模块不依赖 Tauri**：它只认 [`SqliteStore`]（`infra` 的具体实现）与
//! [`SidecarSession`]（`sidecar` 的会话句柄）。窗口、`State` 抽取留在外壳层。
//!
//! 边界说明：纯判定（[`restart_decision`] / [`tail_hint`] / [`finalize_idle_progress`]
//! / [`plan_restart`]）是零副作用的函数，可单独钉在单测里；只有真正要 spawn
//! python 的函数才会碰进程。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::domain::job::{
    Job, JobStore, SensitiveEntry, SensitiveKind, SensitiveStore,
};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::infra::SqliteStore;
use crate::core::sidecar::ingest::{DecisionNote, IngestConfig};
use crate::core::sidecar::process::SidecarConfig;
use crate::core::sidecar::protocol::Command;
use crate::core::sidecar::session::{
    IngestPort, Ports, ScrapeProgress, SidecarSession, STOPPED_BY_USER,
};

/// 允许的自动重启次数（SPEC-002 第 7 节：崩溃后自动重启**一次**）。
pub const MAX_AUTO_RESTARTS: u32 = 1;

/// 默认平台标识（`scrape_start` / `login_start` / `session_open` 的缺省值）。
pub const DEFAULT_PLATFORM: &str = "boss";

/// 解析 sidecar 目录：exe 同级优先，其次仓库开发树。
///
/// `manifest_dir` 传 [`crate::core::sidecar::process::locate_script_dir`] 要用的
/// `CARGO_MANIFEST_DIR`——真实调用方用编译期常量，单测可以传假的。
pub fn resolve_script_dir(
    exe_dir: Option<PathBuf>,
    manifest_dir: &Path,
) -> Result<PathBuf, String> {
    // 起不来时必须留下痕迹：界面只会显示一句「启动抓取进程失败」，而真正的原因
    // （找不到 sidecar.py / python 不在 PATH / ready 超时）全在这行里。
    // 历史上 sidecar 静默死亡就是因为没人把这行打出来。
    eprintln!("[sidecar] 解析目录 exe_dir={exe_dir:?} manifest={}", manifest_dir.display());
    crate::core::sidecar::process::locate_script_dir(exe_dir.as_deref(), manifest_dir)
        .ok_or_else(|| {
            format!(
                "找不到 scraper/sidecar.py（已查 exe 同级与 {} 的上级目录）。\
                 开发态请在仓库根启动；打包态请确认 scraper/ 随应用发布。",
                manifest_dir.display()
            )
        })
}

/// 一次抓取任务的快照：崩溃重启时要照着原样再下发一遍。
///
/// 只存「用户下达过的指令」，不存任何抓取进度——进度在 [`SidecarSession`] 里，
/// 重启时由 [`restart_session`] 把旧计数搬到新会话上，计数因此不会被清零。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapeJobSpec {
    pub script_dir: PathBuf,
    pub platform: String,
    pub keywords: Vec<String>,
    /// `0` = 交给 sidecar 的默认上限
    pub max_jd: u32,
    pub min_score: u8,
}

impl ScrapeJobSpec {
    /// 本次任务要下发的 `scrape.start` 命令。
    pub fn start_command(&self) -> Command {
        Command::ScrapeStart {
            platform: self.platform.clone(),
            keywords: self.keywords.clone(),
            max_jd: self.max_jd,
            min_score: self.min_score,
        }
    }
}

/// 入库端口：写达标岗位 + 把结论记进敏感库 + 写决策日志。
///
/// `Arc<SqliteStore>` 被移进消费线程，因此不占用外壳层 `StoreState` 的锁——
/// 用户在抓取过程中打开列表不会被写入阻塞。
pub fn make_sink(store: Arc<SqliteStore>) -> impl IngestPort {
    let put_store = Arc::clone(&store);
    let remember_store = Arc::clone(&store);
    let record_store = Arc::clone(&store);
    Ports {
        put: move |job: &Job| {
            let inserted = match put_store.insert_job(job) {
                Ok(v) => v,
                Err(e) => return Err(format!("入库失败：{e}")),
            };
            // 入库（或此前已入库）都登记为 stored，下次抓取能在卡片阶段直接跳过，
            // 省掉一整轮详情页（3~6 秒/条）。写失败不影响主流程，仅记日志。
            if let Err(e) = put_store.add_sensitive(&SensitiveEntry {
                platform: &job.platform,
                company: job.company.as_deref(),
                title: job.title.as_deref(),
                reason: "已入库",
                kind: SensitiveKind::Stored,
                job_id: Some(job.job_id.as_str()),
                salary: job.salary.as_deref(),
                location: job.location.as_deref(),
                job_url: job.job_url.as_deref(),
            }) {
                eprintln!("[scrape] 写入敏感库失败（stored）：{e}");
            }
            Ok(inserted)
        },
        remember: move |entry: &SensitiveEntry<'_>| {
            if let Err(e) = remember_store.add_sensitive(entry) {
                eprintln!("[scrape] 写入敏感库失败（rejected）：{e}");
            }
        },
        // 决策日志写失败不打断抓取：它是审计线索，不是主数据。但失败必须出声，
        // 否则「决策日志页面恒空」这个 bug 会再次悄无声息地回来。
        record: move |note: &DecisionNote| {
            if let Err(e) = record_store.log_score_change(
                &note.platform,
                &note.company_job,
                note.decision,
                &note.reason,
                None,
                note.new_score,
            ) {
                eprintln!("[scrape] 写决策日志失败（{}）：{e}", note.decision);
            }
        },
    }
}

/// 敏感库查询器：卡片阶段先查重 / 查敏感库，命中就不再开详情页。
///
/// 命中即跳过，**不看 kind**：`stored` 是「库里已经有了」，
/// `rejected` 是「之前用户不要」，两者都不需要再花 3~6 秒拉详情。
pub fn make_lookup(
    store: Arc<SqliteStore>,
) -> impl FnMut(&str, Option<&str>, Option<&str>) -> Option<(String, bool)> + Send + 'static {
    move |platform: &str, company: Option<&str>, title: Option<&str>| {
        // 先精确查（公司+岗位），不中再按公司粗查（任一曾判定过的公司整体回避）
        let hit = store
            .sensitive_hit(platform, company, title)
            .ok()
            .flatten()
            .or_else(|| store.sensitive_company_hit(platform, company).ok().flatten());
        hit.map(|h| {
            let stored = h.is_stored();
            let reason = h
                .reason
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "此前已判定过".to_string());
            (reason, stored)
        })
    }
}

/// 拉起一个 sidecar 会话（首次启动与崩溃重启走同一条路径）。
///
/// 首次启动与崩溃重启共用此处，避免「重启版少配了某个参数」的分叉。
pub fn launch_session(
    spec: &ScrapeJobSpec,
    store: Arc<SqliteStore>,
) -> Result<SidecarSession, String> {
    let scfg = SidecarConfig {
        program: None,
        script_dir: spec.script_dir.clone(),
        // data_dir 让 sidecar 用默认值（与现行版共用 %LOCALAPPDATA%\ai-job-assistant）。
        // 改成别处会让用户被迫重新扫码登录。
        data_dir: None,
    };
    let icfg = IngestConfig {
        min_score: spec.min_score,
        ..Default::default()
    };
    SidecarSession::start(
        scfg,
        DEFAULT_PROFILE.clone(),
        icfg,
        make_sink(Arc::clone(&store)),
        make_lookup(store),
    )
    .map_err(|e| {
        eprintln!("[sidecar] 启动失败：{e}");
        format!("启动抓取进程失败：{e}")
    })
}

/// 确保 `guard` 里有一个活着的 sidecar 会话，返回它的 pid。
///
/// 抽出来的理由（不只是「少复制一段」）：**此前只有「开始抓取」和「扫码登录」
/// 能启动 sidecar**，于是想看一眼 Boss 消息的人被迫先跑一次抓取——把实现顺序
/// 当成了操作顺序。更坑的是「扫码登录」会一直占住 sidecar 的 `_worker` 直到
/// 登录成功或超时，期间聊天一律被「正在抓取或登录」挡回去。
///
/// 调用方须持有 `state.0` 的锁（这里不再加锁，否则会自锁）。
pub fn ensure_session_locked(
    guard: &mut Option<SidecarSession>,
    script_dir: PathBuf,
    store: Arc<SqliteStore>,
) -> Result<u32, String> {
    if guard.is_none() {
        let spec = ScrapeJobSpec {
            script_dir,
            // 只起浏览器、不下发抓取：平台/关键词/门槛在这里无意义，
            // 但快照仍要有个合法值，登录与 session_open 不读它们。
            platform: DEFAULT_PLATFORM.to_string(),
            keywords: Vec::new(),
            max_jd: 0,
            min_score: crate::core::scoring::SCORE_MIN_DEFAULT,
        };
        let session = launch_session(&spec, store)?;
        eprintln!("[sidecar] 已启动 pid={}", session.pid());
        *guard = Some(session);
    }
    Ok(guard.as_ref().expect("刚刚已保证存在").pid())
}

/// 面对一次退出，应当怎么处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartAction {
    /// 正常收工或用户主动停止：什么都不做
    Skip,
    /// 崩了，起一个新进程
    Restart,
    /// 已经重启过一次又崩：别再来一次，把原因写进进度
    GiveUp,
}

/// 崩溃重启的判定。纯函数，便于把四种情形都钉死在单测里。
///
/// - 用户点过「停止」→ `Skip`：那是我们要的结束，复活它只会让用户以为按钮没生效
/// - 退出码 0 → `Skip`：关键词跑完是 sidecar 的正常收工
/// - 没重启过 → `Restart`
/// - 已重启过 → `GiveUp`：再崩就是稳定失败（缺依赖 / 认证失效），
///   无限重启只会反复起浏览器、反复弹验证
pub fn restart_decision(
    prev: &ScrapeProgress,
    restarts: u64,
    code: Option<i32>,
) -> RestartAction {
    if prev.finished_reason.as_deref() == Some(STOPPED_BY_USER) {
        return RestartAction::Skip;
    }
    if matches!(code, Some(0)) {
        return RestartAction::Skip;
    }
    if restarts >= u64::from(MAX_AUTO_RESTARTS) {
        return RestartAction::GiveUp;
    }
    RestartAction::Restart
}

/// 崩溃时附一句 stderr 尾巴，否则用户只知道「崩了」，不知道崩在哪。
pub fn tail_hint(stderr: &str, code: Option<i32>) -> String {
    let last = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let code_txt = match code {
        Some(c) => format!("退出码 {c}"),
        None => "退出码未知".to_string(),
    };
    if last.is_empty() {
        format!("（{code_txt}）")
    } else {
        format!("（{code_txt}：{}）", clamp_hint(last))
    }
}

fn clamp_hint(s: &str) -> String {
    const MAX: usize = 160;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push('…');
    out
}

/// 崩溃时写进进度的「放弃」原因。
pub fn give_up_reason(stderr: &str, code: Option<i32>) -> String {
    format!(
        "抓取进程连续崩溃，已自动重启 {} 次后停止{}",
        MAX_AUTO_RESTARTS,
        tail_hint(stderr, code)
    )
}

/// 重启成功后写进进度的 `last_log`。
pub fn restart_log_line(stderr: &str, code: Option<i32>) -> String {
    format!(
        "[warn] 抓取进程异常退出（{}），已自动重启一次{}",
        code.map(|c| c.to_string())
            .unwrap_or_else(|| "退出码未知".into()),
        tail_hint(stderr, code)
    )
}

/// 一次退出之后的动作计划。纯值，便于单测覆盖四种情形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartPlan {
    /// 什么都不做
    None,
    /// 放弃：把这段原因写进进度
    GiveUp(String),
    /// 重启：旧会话已 shutdown，新会话还没起
    Restart(ScrapeJobSpec),
}

/// 只做判定、不碰进程：给定旧会话的进度与退出码，返回该怎么做。
pub fn plan_restart(
    prev: &ScrapeProgress,
    restarts: u64,
    code: Option<i32>,
    stderr: &str,
    spec: Option<&ScrapeJobSpec>,
) -> RestartPlan {
    match restart_decision(prev, restarts, code) {
        RestartAction::Skip => RestartPlan::None,
        RestartAction::GiveUp => RestartPlan::GiveUp(give_up_reason(stderr, code)),
        RestartAction::Restart => match spec {
            // 没有快照（本次启动不是「开始抓取」触发的）就不重启
            None => RestartPlan::None,
            Some(s) => RestartPlan::Restart(s.clone()),
        },
    }
}

/// 重启后把新会话的进度收拾干净：清掉旧原因、写明重启日志、计数 +1。
///
/// 计数必须接着上一次，否则界面上刚抓的几十条会「凭空消失」——
/// 所以 `adopt_progress` 在 `send` 之后调用（`send` 会把计数清零）。
pub fn finish_restart(
    fresh: &SidecarSession,
    old: &ScrapeProgress,
    stderr: &str,
    code: Option<i32>,
) {
    fresh.adopt_progress(old);
    if let Ok(mut p) = fresh.progress_handle().lock() {
        p.finished_reason = None;
        p.last_log = Some(restart_log_line(stderr, code));
    }
    fresh.bump_restarts();
}

/// 在「已经拿到新会话」之后补一次重启：下发命令失败时把原因写回进度。
///
/// 返回新 pid；失败返回 `Err(())`——调用方只关心「有没有重启成功」，
/// 失败原因已写进进度快照，界面会显示出来。
pub fn restart_session(
    fresh: &mut SidecarSession,
    spec: &ScrapeJobSpec,
    old: &ScrapeProgress,
    stderr: &str,
    code: Option<i32>,
    progress: &std::sync::Mutex<ScrapeProgress>,
) -> Result<u32, ()> {
    let pid = fresh.pid();
    if let Err(e) = fresh.send(&spec.start_command()) {
        fresh.shutdown();
        if let Ok(mut p) = progress.lock() {
            p.running = false;
            p.finished_reason = Some(format!("自动重启失败：下发抓取命令出错（{e}）"));
        }
        return Err(());
    }
    finish_restart(fresh, old, stderr, code);
    Ok(pid)
}

/// 会话已从退出处收工、但进度线程还没来得及写 `finished_reason` 时，补一个原因。
///
/// 消费线程写完进度会主动再刷一次；这里兜住「轮询恰好插在那两行之间」的窗口，
/// 否则界面会一直停在 `running=false` + 原因为空的半截状态。
pub const FALLBACK_STOP_REASON: &str = "抓取进程已退出";

pub fn finalize_idle_progress(mut p: ScrapeProgress) -> ScrapeProgress {
    if !p.running && p.finished_reason.is_none() {
        p.finished_reason = Some(FALLBACK_STOP_REASON.to_string());
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::job::Job;

    #[test]
    fn idle_progress_without_reason_gets_one() {
        let mut p = ScrapeProgress::new();
        p.running = false;
        let p = finalize_idle_progress(p);
        assert_eq!(p.finished_reason.as_deref(), Some(FALLBACK_STOP_REASON));

        // 真原因必须原样保留，不能被兜底覆盖
        let mut q = ScrapeProgress::new();
        q.running = false;
        q.finished_reason = Some("已按用户请求停止".to_string());
        let q = finalize_idle_progress(q);
        assert_eq!(q.finished_reason.as_deref(), Some("已按用户请求停止"));
    }

    // ---------- 崩溃重启的四种情形（SPEC-002 第 7 节） ----------

    fn exited(reason: &str) -> ScrapeProgress {
        ScrapeProgress {
            running: false,
            finished_reason: Some(reason.to_string()),
            ..ScrapeProgress::new()
        }
    }

    fn spec() -> ScrapeJobSpec {
        ScrapeJobSpec {
            script_dir: PathBuf::from("scraper"),
            platform: "boss".into(),
            keywords: vec!["Java".into()],
            max_jd: 10,
            min_score: 70,
        }
    }

    #[test]
    fn restart_skips_when_user_stopped() {
        // 用户点了停止：退出码即使非 0 也不能复活
        let p = exited(STOPPED_BY_USER);
        assert_eq!(restart_decision(&p, 0, Some(1)), RestartAction::Skip);
        assert_eq!(restart_decision(&p, 0, None), RestartAction::Skip);
    }

    #[test]
    fn restart_skips_normal_exit() {
        // 退出码 0 = 关键词跑完，这是正常收工
        let p = exited("抓取进程正常退出");
        assert_eq!(restart_decision(&p, 0, Some(0)), RestartAction::Skip);
    }

    #[test]
    fn restart_happens_once_on_crash() {
        let p = exited("抓取进程异常退出，退出码 1");
        assert_eq!(restart_decision(&p, 0, Some(1)), RestartAction::Restart);
        // 退出码未知（被外部杀掉 / 断管道）同样算崩溃
        assert_eq!(restart_decision(&p, 0, None), RestartAction::Restart);
    }

    #[test]
    fn restart_gives_up_after_one_retry() {
        // 已经重启过一次又崩：再重启只会反复起浏览器、反复弹验证
        let p = exited("抓取进程异常退出，退出码 1");
        assert_eq!(restart_decision(&p, 1, Some(1)), RestartAction::GiveUp);
        assert_eq!(restart_decision(&p, 9, Some(1)), RestartAction::GiveUp);
    }

    #[test]
    fn tail_hint_names_code_and_last_stderr_line() {
        let hint = tail_hint("Traceback...\nModuleNotFoundError: patchright\n\n", Some(1));
        assert!(hint.contains("退出码 1"), "{hint}");
        assert!(hint.contains("patchright"), "{hint}");
        // 没有 stderr 时也不能出现空括号
        assert_eq!(tail_hint("   ", None), "（退出码未知）");
    }

    #[test]
    fn tail_hint_truncates_on_char_boundary() {
        let long = "错".repeat(400);
        let hint = tail_hint(&long, Some(2));
        assert!(hint.contains('…'), "超长 stderr 必须截断");
        assert!(hint.chars().count() < 200);
    }

    #[test]
    fn plan_replays_recorded_spec_on_crash() {
        let p = exited("抓取进程异常退出，退出码 1");
        let s = spec();
        assert_eq!(
            plan_restart(&p, 0, Some(1), "boom", Some(&s)),
            RestartPlan::Restart(s.clone()),
            "崩溃且没重启过，必须照原样重放"
        );
        let cmd = s.start_command();
        match cmd {
            Command::ScrapeStart {
                platform,
                keywords,
                max_jd,
                min_score,
            } => {
                assert_eq!(platform, "boss");
                assert_eq!(keywords, vec!["Java".to_string()]);
                assert_eq!(max_jd, 10);
                assert_eq!(min_score, 70);
            }
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn plan_gives_up_with_reason_after_one_retry() {
        let p = exited("抓取进程异常退出，退出码 1");
        let plan = plan_restart(&p, 1, Some(1), "ModuleNotFoundError: patchright", Some(&spec()));
        match plan {
            RestartPlan::GiveUp(reason) => {
                assert!(reason.contains("已自动重启 1 次后停止"), "{reason}");
                assert!(reason.contains("退出码 1"), "{reason}");
                assert!(reason.contains("patchright"), "{reason}");
            }
            other => panic!("实际 {other:?}"),
        }
    }

    #[test]
    fn plan_does_nothing_without_recorded_spec() {
        let p = exited("抓取进程异常退出，退出码 1");
        // 登录 / session_open 起出来的会话没有抓取快照，崩了也不该复活
        assert_eq!(plan_restart(&p, 0, Some(1), "boom", None), RestartPlan::None);
        assert_eq!(
            plan_restart(&p, 0, Some(0), "boom", Some(&spec())),
            RestartPlan::None
        );
    }

    // 敏感库端口用的是真 `SqliteStore`，这里只核对「写库失败不影响主流程」的
    // 契约：写入即命中，且第二次命中能看到 stored 标记。
    #[test]
    fn sink_and_lookup_share_the_same_store() {
        let store = Arc::new(SqliteStore::open_in_memory().expect("内存库"));
        let job = Job {
            job_id: "boss:sink-1".into(),
            platform: "boss".into(),
            title: Some("Java 后端".into()),
            company: Some("某公司".into()),
            ..Default::default()
        };
        let mut sink = make_sink(Arc::clone(&store));
        sink.put(&job).expect("入库成功");

        let mut lookup = make_lookup(Arc::clone(&store));
        let hit = lookup("boss", Some("某公司"), Some("Java 后端"));
        let (reason, stored) = hit.expect("已入库的岗位必须命中敏感库");
        assert!(stored, "stored 标记必须为真");
        assert_eq!(reason, "已入库");

        // 同一条岗位重复 put：返回 false（已存在），但仍然登记 stored
        assert!(!sink.put(&job).expect("重复入库不报错"));
    }

    /// 生产端口必须把决策真的写进 `jd_decisions`。
    ///
    /// 这是本次修复的落点：`make_sink` 早先只实现了 `put` / `remember`，
    /// `IngestPort::record` 走默认空实现，于是决策日志在生产上**一条都没有**，
    /// 界面只剩 `rescore` 那一档有内容。
    #[test]
    fn sink_writes_decisions_into_the_store() {
        use crate::core::domain::store_ports::DecisionStore;
        use crate::core::sidecar::ingest::DecisionNote;

        let store = Arc::new(SqliteStore::open_in_memory().expect("内存库"));
        let mut sink = make_sink(Arc::clone(&store));
        sink.record(&DecisionNote {
            platform: "boss".into(),
            company_job: "某公司/Java 后端".into(),
            decision: "reject",
            reason: "exclude：外包".into(),
            new_score: None,
        });
        let rows = store.list_decisions(0, 0).expect("读决策日志");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision.as_deref(), Some("reject"));
        assert_eq!(rows[0].company_job.as_deref(), Some("某公司/Java 后端"));
        assert_eq!(rows[0].reason.as_deref(), Some("exclude：外包"));
    }
}
