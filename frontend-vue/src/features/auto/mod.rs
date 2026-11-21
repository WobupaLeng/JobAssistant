//! 自动投递控制条：开始 / 暂停 / 继续 + 状态文字 + 重置。
//!
//! 行为契约（**不可改动**，见 `panel.rs` 的模块注释）：
//! - 每调一次 `auto_greet_next(None)` 最多发一条；
//! - 下一次调用时刻**只由后端回执的 `next_at_ms` 决定**，前端不自己算间隔；
//! - 定时器只负责「每秒问一次后端状态 + 到点发一条」，
//!   窗口最小化或 WebView 挂起时循环自然停止——这是已知且接受的行为。

mod panel;
mod queue;
mod state;
mod timer;

pub use panel::AutoBar;
pub use queue::AutoQueue;
