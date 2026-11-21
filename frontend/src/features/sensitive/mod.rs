//! 敏感库（图 1）：居中浮层，列出「已抓过的岗位」并支持手动移出。
//!
//! 只负责**展示 + 移出**：真正的「下一轮抓取直接跳过」是抓取链路里
//! `SensitiveEntry` 的三道预筛在做（`app/src-tauri/src/core/sidecar/ingest.rs`），
//! 这里不做任何业务判定。
//!
//! 与岗位详情抽屉（`features/jobs/drawer.rs`）**互斥**：同一时刻只开一个，
//! 由页面层持有一个 `sensitive_open: RwSignal<bool>` 并把详情抽屉关掉。
//!
//! 集成方式见 `drawer_entry.rs` 的模块注释（两选一）。

mod drawer_entry;
mod list;
mod panel;
mod state;

pub use drawer_entry::{SensitiveDrawer, SensitiveEntry, SensitiveOverlay};
pub use panel::SensitivePanel;
pub use state::{Filter, SensitiveState};
