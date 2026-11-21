//! 崩溃重启判定与执行：什么情况下重启、放弃时写什么原因、重启后怎么收尾。

use crate::core::sidecar::session::{ScrapeProgress, SidecarSession, STOPPED_BY_USER};

use super::launch::ScrapeJobSpec;

/// 允许的自动重启次数（SPEC-002 第 7 节：崩溃后自动重启**一次**）。
pub const MAX_AUTO_RESTARTS: u32 = 1;

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
