//! Boss 消息：会话列表 + 线程 + 打招呼草稿 + 输入框。
//!
//! 分层：`state` 管状态与命令，`list` / `thread` / `composer` 只管渲染。
//! 页面层（`pages/chat.rs`）只做装配与首帧触发，不写业务规则。

pub mod composer;
pub mod list;
pub mod state;
pub mod thread;

pub use list::ChatList;
pub use state::ChatState;
pub use thread::ChatThread;
