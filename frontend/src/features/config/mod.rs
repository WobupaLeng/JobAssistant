//! 配置页：大模型 / 抓取默认参数 / 打分口径 / 数据目录。
//!
//! `state` 拉数并保存，`draft` 是「草稿 → 保存计划」的纯规则，
//! `form` 提供分组外壳，`panel` 组装视图。

pub mod draft;
pub mod form;
pub mod panel;
pub mod state;

pub use panel::ConfigPanel;
pub use state::ConfigState;
