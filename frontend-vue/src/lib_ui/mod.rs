//! 界面层的纯逻辑：导航、主题、文本、时间、分数着色。
//!
//! 对应 TS 版的 `src/lib/` 目录。这一层**不碰 invoke、不持有页面状态**，
//! 因此可以被单独推理和测试。

pub mod job;
pub mod logfmt;
pub mod motion;
pub mod nav;
pub mod session;
pub mod text;
pub mod theme;
pub mod time;
/// 时钟读口。其余模块都是纯函数，只有它碰 `js_sys::Date`——
/// 单测因此永远不经过 wasm 胶水（见文件头）。
pub mod clock;
