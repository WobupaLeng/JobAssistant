//! 会话启动编排：解析 sidecar 目录、抓取任务快照、起一个 sidecar 会话。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::infra::SqliteStore;
use crate::core::sidecar::ingest::IngestConfig;
use crate::core::sidecar::process::SidecarConfig;
use crate::core::sidecar::protocol::Command;
use crate::core::sidecar::session::SidecarSession;

use super::sink::{make_lookup, make_sink};

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
/// 重启时由 [`super::restart::restart_session`] 把旧计数搬到新会话上，
/// 计数因此不会被清零。
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
