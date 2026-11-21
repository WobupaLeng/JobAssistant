//! 业务核心 crate。
//!
//! 分层铁律（SPEC-002 第 4 节）：
//! - 本 crate **不得依赖 `tauri`**；
//! - [`domain`] 不得依赖 `rusqlite` / `reqwest` 等具体技术。
//!
//! 这样领域规则可以用纯 `cargo test` 毫秒级验证，不必启动窗口或浏览器。

pub mod domain;
pub mod infra;
/// 抓取编排（会话启停 / 重启策略 / 敏感库端口）。
///
/// 从命令层下沉而来（AGENTS.md 2.2：命令层只做「收参数 → 调 core → 转字符串」）。
/// 本模块只依赖 `infra` 与 `sidecar`，不依赖 `tauri`。
pub mod scrape;
pub mod sidecar;

/// 打分口径常量：唯一来源（SPEC-002 第 3.2 节）。
///
/// 前端**禁止硬编码**这两个值，必须通过 `get_scoring_spec` 命令获取，
/// 否则"门槛 60 分却筛出 6 分岗位"这类静默错误会再次出现。
pub mod scoring {
    /// 分数上限。100 分制（SPEC-002 D3 决策）。
    pub const SCORE_MAX: u8 = 100;
    /// 低于此分数的 JD 不入库。
    pub const SCORE_MIN_DEFAULT: u8 = 70;
}
