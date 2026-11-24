//! 基础设施层：具体技术实现。
//!
//! 只被 `app` 引用，`domain` 不依赖本层（SPEC-002 第 4 节）。

pub mod llm;
pub mod sqlite;

pub use llm::{
    HttpLlmClient, GREET_TEMPERATURE, SCORE_TEMPERATURE, TEMPERATURE_MAX, TEMPERATURE_MIN,
    DEFAULT_MODEL,
};
pub use sqlite::{SqliteStore, StoreError};
