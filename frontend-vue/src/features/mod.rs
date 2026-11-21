//! 业务域：每个子模块负责一个功能块。
//!
//! 依赖方向：`pages → features → components → lib_ui`。
//! features 不得引用 pages（那会形成环）。

pub mod auto;
pub mod chat;
pub mod config;
pub mod jobs;
pub mod logs;

pub mod scrape;
pub mod score;
pub mod sensitive;
