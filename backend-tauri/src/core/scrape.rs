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
//!
//! **按职责拆分**（2026-11-21，纯搬移 + 可见性调整，行为零变化）：
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | `launch` | `resolve_script_dir` / `ScrapeJobSpec` / `launch_session` / `ensure_session_locked` / `DEFAULT_PLATFORM` |
//! | `filters` | `ScrapeFilters` / `filters_from_defaults` / `exclude_rules_from_defaults`（scrape_defaults → 筛选快照 + 排除规则，纯函数） |
//! | `sink` | `make_sink` / `make_lookup`（`IngestPort` 实现与敏感库查询器） |
//! | `restart` | `MAX_AUTO_RESTARTS` / `RestartAction` / `restart_decision` / `plan_restart` / `restart_session` / `FALLBACK_STOP_REASON` / `finalize_idle_progress` |
//! | `tests` | 单元测试（原样搬移，断言零改动） |
//!
//! 对外路径 `core::scrape::*` 保持不变：下面把每个公开项都 re-export 到本模块根，
//! `commands_scrape.rs` 等调用方一行 `use` 都不用改。

mod filters;
mod launch;
mod restart;
mod sink;
#[cfg(test)]
mod tests;

pub use filters::{exclude_rules_from_defaults, filters_from_defaults, ScrapeFilters};
pub use launch::{
    ensure_session_locked, launch_session, resolve_script_dir, ScrapeJobSpec, DEFAULT_PLATFORM,
};
pub use restart::{
    finalize_idle_progress, finish_restart, give_up_reason, plan_restart, restart_decision,
    restart_log_line, restart_session, tail_hint, RestartAction, RestartPlan,
    FALLBACK_STOP_REASON, MAX_AUTO_RESTARTS,
};
pub use sink::{make_lookup, make_sink};
