//! sidecar 进程管理：spawn / 写 stdin / 按行读事件 / 优雅退出。
//!
//! SPEC-002 第 2.2 节与第 7 节风险表。设计要点：
//!
//! - **一条写者**：stdin 与 stdout 各由一个专用线程持有（`SidecarHandle`）。
//!   若把 `ChildStdin` 放在共享锁里，抓取线程每条事件都要抢那把锁，
//!   一旦写端阻塞（管道满）就与读线程互相等待，表现为「界面卡死但进程还活着」。
//! - **读线程不会死锁在子进程上**：读到 EOF 立刻发 `SidecarEvent::Exited`，
//!   让上层知道「没法再发命令了」，而不是继续往死管道里写。
//! - **崩溃不自动重启**：SPEC-002 第 7 节的缓解手段是「监控退出码并通知前端」。
//!   本模块只做监控与上报；重启策略交给调用方决定——`python` 根本不存在时
//!   重启一万次也一样，必须让用户看到原因。
//! - **关停有上限**：先关 stdin（sidecar 主循环会退出），必要时 kill，
//!   最后等待，全程有超时，绝不让应用退出时挂住。
//!
//! 本模块是纯 `std` 实现，不依赖 Tauri，因此可以独立 `cargo test`。
//!
//! **按职责拆分**（2026-11-21，纯搬移 + 可见性调整，行为零变化）：
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | `config` | `SidecarConfig` / `locate_script_dir` / `PYTHON_ENV` / `DEFAULT_READY_TIMEOUT` |
//! | `handle` | `SidecarError` / `SidecarEvent` / `SidecarHandle`（启动、发命令、关停） |
//! | `codec` | `protocol_line` / `parse_line` / `LineParse` / 回执路由 / 三条读线程 |
//! | `tests` | 单元测试 |
//!
//! 对外路径 `core::sidecar::process::*` 保持不变：下面把每个公开项都
//! re-export 到本模块根，调用方一行 `use` 都不用改。

mod codec;
mod config;
mod handle;
#[cfg(test)]
mod tests;

pub use codec::{parse_line, protocol_line, LineParse};
pub use config::{locate_script_dir, SidecarConfig, DEFAULT_READY_TIMEOUT, PYTHON_ENV};
pub use handle::{SidecarError, SidecarEvent, SidecarHandle};

// 回执类型与读线程既被 `handle` 用、也被 `codec` 用，公开面只有
// `SidecarReply`（`request` 的返回值载体）与 `LineParse`。
pub use handle::SidecarReply;
