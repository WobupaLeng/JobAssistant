//! 日志域：**运行日志**与**决策日志**两套数据并存（用户裁决第 ⑦ 条）。
//!
//! 目录分工：
//! - `state.rs` —— 纯逻辑 + 可单测函数（排序、增量合并、筛选、分数文案）
//! - `runtime.rs` —— 运行日志视图（`list_logs` / `clear_logs`）
//! - `decision.rs` —— 决策日志视图（`list_decisions`）
//! - `panel.rs`  —— 双视图外壳：分段控件、刷新、自动刷新循环与卸载收手
//!
//! 数据源一律是 Tauri 命令（`crate::api`），不是 HTTP，也不是旧版 `web/`
//! 里的前端内存数组。

pub mod decision;
pub mod panel;
pub mod runtime;
pub mod state;

pub use panel::LogPanel;
pub use state::{DecisionFilter, DecisionRow, LogState, LogView, RunRow};
