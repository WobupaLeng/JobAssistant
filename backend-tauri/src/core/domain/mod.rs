//! 领域层：纯业务规则，不依赖任何具体技术（DB / HTTP / Tauri）。
//!
//! 分层铁律见 SPEC-002 第 4 节：`domain` 不得依赖 `rusqlite` / `reqwest`。
//! 这样规则可以用 `cargo test` 毫秒级验证。

pub mod app_config;
pub mod auto_greet;
pub mod greeting;
pub mod job;
pub mod logbuf;
pub mod migration;
pub mod parse;
pub mod profile;
pub mod rescore;
pub mod rules;
pub mod schema;
pub mod scoring;
pub mod scoring_llm;
pub mod scoring_service;
pub mod store_ports;

pub use auto_greet::{
    next_gap_ms, AutoGreetRuntime, AUTO_GAP_MIN_MS, AUTO_GAP_MS, AUTO_MAX_FAILURES,
};
pub use app_config::{
    has_api_key, mask_api_key, resolve_limit, LimitArgs, LimitValues,
};
pub use job::{
    Job, JobQuery, JobStatus, JobStore, JobTabCounts, ScoreSource, SensitiveEntry, SensitiveHit,
    SensitiveKind, SensitiveStore, SortField,
};
pub use scoring_llm::{
    LlmClient, LlmError, LlmScore, ScoreDecision, SYSTEM_PROMPT, LLM_PREFILTER_DEFAULT,
};
pub use scoring_service::{score_job, ScoreConfig, ScoreOutcome};
pub use greeting::{
    build_prompt, matched_skills, rule_greeting, write_greeting, Greeting, GreetingProfile,
    DEFAULT_PROMPT,
};
pub use rescore::{rescore_batch, RescoreChange, RescoreOptions, RescoreOutcome};
pub use store_ports::{
    parse_config_json, ConfigStore, DecisionRecord, DecisionStore, JobAdminStore, JobStats,
    SensitiveAdminStore, SensitiveFilter, SensitiveQuery, SensitiveRecord,
};
