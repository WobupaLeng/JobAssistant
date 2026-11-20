//! sidecar（Python 抓取进程）协议：stdin/stdout 上的 JSON Lines。
//!
//! SPEC-002 第 2.1 节。约束：
//! - **不使用 HTTP、不占端口**；
//! - sidecar 无状态：不打分、不落库、不读配置；
//! - 单行上限 1 MB，超限截断并报错，不静默丢弃。

pub mod protocol;

pub mod ingest;
pub mod process;
pub mod session;

pub use ingest::{prepare, to_job, IngestConfig, IngestDetail, IngestVerdict};
pub use process::{
    locate_script_dir, parse_line, protocol_line, SidecarConfig, SidecarError, SidecarEvent,
    SidecarHandle, DEFAULT_READY_TIMEOUT, PYTHON_ENV,
};
pub use protocol::{
    ChatMessage, Command, ErrorInfo, Event, Envelope, Expectation, Friend, ResumeItem, ScrapedJob,
    MAX_LINE_BYTES,
};
pub use session::{JobSink, ScrapeProgress, SessionUpdate, SidecarSession};
