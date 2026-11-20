//! 契约类型：与 `app/src-tauri/src/commands_*.rs` 的结构体逐一对应。
//!
//! 这里的每个字段名都必须与 Rust 侧 serde 的输出**逐字一致**（含 snake_case）。
//! 与原来的 `skeleton-web/src/api/types.ts` 相比只多了一样东西：`serde` 派生，
//! 让反序列化在**编译期**校验字段与类型，而不是等运行时拿到 `undefined`。
//!
//! 存在的理由（与 TS 版相同的教训）：命令签名改动必须同步更新这里，
//! 否则界面会静默显示空白——serde 的 `default` 会让缺字段变成默认值而非报错。

use serde::{Deserialize, Serialize};

/// `app_info()` 的返回。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub shell: String,
    pub stage: String,
    /// 数据目录**绝对路径**（配置页「数据目录」一栏直接显示它）。
    /// 旧版恒为空串，因为那时后端还没下发；现在由 setup 阶段捕获的真实路径填。
    #[serde(default)]
    pub data_dir: String,
}

/// `get_scoring_spec()` 的返回。
///
/// **前端禁止硬编码满分与门槛**：该常量历史上散落 5 处，漏改任一处会产生
/// 「门槛 60 分却筛出 6 分岗位」的静默错误（AGENTS.md 第 3.1 节）。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ScoringSpec {
    pub score_max: u8,
    pub score_min_default: u8,
}

impl Default for ScoringSpec {
    fn default() -> Self {
        // 仅为占位：真实值必须来自后端。90 分制读数的位置在拿到 spec 前显示「…」。
        Self { score_max: 0, score_min_default: 0 }
    }
}

/// 岗位实体，对齐 `core/src/domain/job.rs` 的 `Job`。
///
/// 定义已下沉到 [`crate::lib_ui::job::Job`]（`lib_ui` 不得反向依赖 `api`），
/// 此处只做 re-export：`use crate::api::types::Job;` 这条既有路径保持不变，
/// 反序列化实现也仍然落在本模块（`api` 负责契约校验）。
pub use crate::lib_ui::job::Job;

/// `Job` 的反序列化在 `api` 侧实现：字段名与 Rust serde 输出逐字对应。
impl<'de> Deserialize<'de> for Job {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename = "Job")]
        struct Raw {
            job_id: String,
            platform: String,
            title: String,
            company: String,
            salary: String,
            status: String,
            created_at: i64,
            job_url: String,
            jd_text: String,
            location: String,
            experience: String,
            degree: String,
            hr_name: String,
            hr_position: String,
            hr_active_status: String,
            company_industry: String,
            company_scale: String,
            company_stage: String,
            company_address: String,
            company_intro: String,
            hiring_count: i64,
            founded_years: i64,
            match_score: u8,
            rejected: i64,
            score_source: String,
            score_reason: String,
            updated_at: i64,
            score_scale: i64,
        }

        let r = Raw::deserialize(deserializer)?;
        Ok(Job {
            job_id: r.job_id,
            platform: r.platform,
            title: r.title,
            company: r.company,
            salary: r.salary,
            status: r.status,
            created_at: r.created_at,
            job_url: r.job_url,
            jd_text: r.jd_text,
            location: r.location,
            experience: r.experience,
            degree: r.degree,
            hr_name: r.hr_name,
            hr_position: r.hr_position,
            hr_active_status: r.hr_active_status,
            company_industry: r.company_industry,
            company_scale: r.company_scale,
            company_stage: r.company_stage,
            company_address: r.company_address,
            company_intro: r.company_intro,
            hiring_count: r.hiring_count,
            founded_years: r.founded_years,
            match_score: r.match_score,
            rejected: r.rejected,
            score_source: r.score_source,
            score_reason: r.score_reason,
            updated_at: r.updated_at,
            score_scale: r.score_scale,
        })
    }
}

/// `list_jobs` 的参数。
///
/// 页签 `tab` 一给，就覆盖 `status` + `include_rejected` + `rejected_only`：
/// 三个页签的语义互斥，拼在一起会得到「未投递里混着否决」这种看不懂的列表。
#[derive(Debug, Clone, Serialize)]
pub struct ListJobsArgs {
    pub status: Option<String>,
    pub platform: Option<String>,
    pub min_score: Option<u8>,
    pub include_rejected: bool,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub sort: Option<String>,
    pub ascending: bool,
    /// 页签：`pending` / `delivered` / `rejected`
    pub tab: Option<String>,
    /// 只看被否决的岗位；`tab = rejected` 时自动等价
    pub rejected_only: bool,
    /// 排序方向 `asc` / `desc`；给了它就覆盖 `ascending`
    pub order: Option<String>,
}

impl Default for ListJobsArgs {
    fn default() -> Self {
        Self {
            status: None,
            platform: None,
            min_score: None,
            include_rejected: false,
            limit: None,
            offset: None,
            sort: None,
            ascending: false,
            tab: None,
            rejected_only: false,
            order: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DbOverview {
    pub total: usize,
    pub by_scale: Vec<(String, i64)>,
    pub has_legacy: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct LlmStatus {
    pub configured: bool,
    pub key_hint: String,
    pub model: String,
    pub prefilter_default: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoreArgs {
    pub company: String,
    pub title: String,
    pub salary: String,
    pub experience: String,
    pub jd: String,
    pub use_llm: bool,
    pub llm_prefilter: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScoreReply {
    pub score: u8,
    pub reason: String,
    pub rejected_no_jd: bool,
    pub source: String,
    pub tokens: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScrapeProgress {
    pub scraped: u32,
    pub inserted: u32,
    pub duplicates: u32,
    pub below_threshold: u32,
    pub rejected: u32,
    pub login_state: String,
    pub last_log: String,
    pub last_title: String,
    pub running: bool,
    pub finished_reason: String,
}

/// 抓取事件流的一帧。Rust 侧是 `#[serde(tag = "kind")]` 的判别联合，
/// 等价于 TS 版的 `ScrapeUpdate = { kind: 'job', ... } | { kind: 'log', ... } | ...`。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum ScrapeUpdate {
    #[serde(rename = "job")]
    Job { summary: String, accepted: bool, score: u8 },
    #[serde(rename = "log")]
    Log { level: String, msg: String },
    #[serde(rename = "login")]
    Login { state: String, msg: String },
    #[serde(rename = "malformed")]
    Malformed { line: String },
    #[serde(rename = "exited")]
    Exited { code: i64 },
    #[serde(rename = "restarted")]
    Restarted { pid: u32 },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScrapeReply {
    pub pid: u32,
    pub min_score: u8,
    pub keywords: Vec<String>,
    pub max_jd: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Friend {
    pub uid: String,
    pub name: String,
    pub company: String,
    pub title: String,
    pub last_msg: String,
    pub last_time: String,
    pub unread: u32,
    pub avatar: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub time: String,
    pub msg_id: String,
    pub recalled: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResumeItem {
    pub resume_id: String,
    pub name: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FriendsReply {
    pub items: Vec<Friend>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessagesReply {
    pub items: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResumesReply {
    pub items: Vec<ResumeItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GreetReply {
    pub text: String,
    pub tokens: u32,
    pub source: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Expectation {
    pub name: String,
    pub code: String,
    pub city: String,
    pub city_code: String,
    pub salary: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionLog {
    pub id: i64,
    pub created_at: String,
    pub platform: String,
    pub job_id: String,
    pub company: String,
    pub title: String,
    pub kind: String,
    pub reason: String,
    pub score: u8,
    pub source: String,
}

/// 敏感库一行，对齐 core::domain::store_ports::SensitiveRecord。
///
/// 旧称 SensitiveRow；platform 之外全是 Option——旧库里确实存在没有
/// 岗位 id / 薪水的行（按公司名命中的排除项），用 String 接会让整页解析失败。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SensitiveRecord {
    pub id: i64,
    /// 落库时间，Unix 秒
    pub created_at: i64,
    pub platform: String,
    pub job_id: Option<String>,
    pub company: Option<String>,
    pub title: Option<String>,
    pub salary: Option<String>,
    pub location: Option<String>,
    pub reason: Option<String>,
    pub job_url: Option<String>,
    /// `stored`（已入库）/ `rejected`（已排除）
    #[serde(default)]
    pub kind: String,
}

/// 敏感库一页。`total` 是**按当前筛选条件**的总数，不是全库总数——
/// 抽屉标题「敏感库 N 条」与「已选中 M 条」用的就是它。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SensitivePage {
    pub items: Vec<SensitiveRecord>,
    pub total: i64,
}

/// `get_app_config()` 的回执，对齐 `commands_config::AppConfigReply`。
///
/// **`api_key` 恒为空串**：密钥只落库、只以 `api_key_hint`（`****尾4位`）回传。
/// 保存时若用户没动密钥框，patch 里就不要带 `api_key`；带了掩码也会被后端
/// 识别并跳过（见 `commands_config::apply_patch`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfigReply {
    /// 恒为空串；界面输入框用它占位
    #[serde(default)]
    pub api_key: String,
    /// `****abcd`；未配置时为空串
    #[serde(default)]
    pub api_key_hint: String,
    #[serde(default)]
    pub has_key: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub theme: String,
    /// `rule` / `llm`
    #[serde(default)]
    pub score_mode: String,
    #[serde(default)]
    pub llm_prefilter: u8,
    /// 有效 JD 上限（抓取时最多补多少个岗位的详情）
    #[serde(default)]
    pub max_jd: u32,
    /// 打分门槛（百分制；满分在 `score_max`，不得硬编码）
    #[serde(default)]
    pub min_score: u8,
    #[serde(default)]
    pub score_max: u8,
    #[serde(default)]
    pub greet_prompt: String,
    /// 出厂提示词，供「恢复默认」按钮使用
    #[serde(default)]
    pub greet_prompt_default: String,
    #[serde(default)]
    pub skills: String,
    #[serde(default)]
    pub exclude_keywords: String,
    #[serde(default)]
    pub exclude_rules: String,
    #[serde(default)]
    pub resume_path: String,
    /// 数据目录**绝对路径**
    #[serde(default)]
    pub data_dir: String,
    #[serde(default)]
    pub last_filter: String,
    #[serde(default)]
    pub tokens_total: i64,
    #[serde(default)]
    pub tokens_greet: i64,
    #[serde(default)]
    pub tokens_score: i64,
    /// 抓取筛选面板上次用过的条件（`scrape_defaults.*` 聚合，14 项）。
    ///
    /// 对齐命令层 `commands_config::AppConfigReply::scrape_defaults`（`serde_json::Value`）。
    /// 一个都没配过时是 `{}`，**不是 `null`**——调用方可直接当对象展开。
    /// 缺省（旧 exe / 字段被裁掉）时 `serde_json::Value::default()` 即 `Null`，
    /// 所以读取方一律走 `as_object()`，不要直接 `unwrap`。
    #[serde(default)]
    pub scrape_defaults: serde_json::Value,
}

/// `set_app_config()` 的回执。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SaveConfigReply {
    pub ok: bool,
    /// 保存后大模型客户端是否可用
    #[serde(default)]
    pub llm_enabled: bool,
    /// 实际写入的键数（提示「已保存（N 项）」）
    #[serde(default)]
    pub updated: usize,
}

/// `set_limit()` 的回执：**后端裁决后的值**，不是前端输入的值。
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct LimitReply {
    pub ok: bool,
    pub max_jd: u32,
    pub min_score: u8,
}

/// `list_decisions()` 的一行，对齐 `core::domain::store_ports::DecisionRecord`。
///
/// `old_score` / `new_score` 是后加的补列：更早的行两列为 `null`，
/// 界面按「无分数变化」渲染（不要显示 `0 → 0`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DecisionRecord {
    pub id: i64,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub company_job: String,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub old_score: Option<u8>,
    #[serde(default)]
    pub new_score: Option<u8>,
}

/// 一条运行日志（`list_logs`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogRow {
    /// Unix 毫秒
    pub ts: u64,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub message: String,
}

/// `list_logs(since)` 的回执：增量条目 + **本次游标**。
///
/// 调用方必须把 `now` 存下来，下次带 `since = now`；
/// 否则每次都会把整段历史重新拉一遍。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogsReply {
    pub items: Vec<LogRow>,
    pub now: u64,
}

/// 三个页签的条数（`job_tab_counts`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct JobTabCounts {
    pub pending: i64,
    pub delivered: i64,
    pub rejected: i64,
}

/// 登录态（`login_status`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LoginStatus {
    pub platform: String,
    /// `logged_in` / `no_login` / `unknown`
    #[serde(default)]
    pub state: String,
    /// sidecar 是否在跑
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub message: String,
    /// 本次会话是否已把登录态存过
    #[serde(default)]
    pub saved: bool,
}

/// `login_save()` 的回执。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LoginSaveReply {
    pub ok: bool,
    pub saved: bool,
    #[serde(default)]
    pub error: String,
}

/// `logout()` 的回执。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogoutReply {
    pub ok: bool,
    /// 是否清掉了登录标记
    pub cleared: bool,
    /// 是否顺带关掉了 sidecar 浏览器会话
    pub closed: bool,
}

/// `chat_send()` 的回执。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SendReply {
    /// 实际发出去的文本（过长时后端已截断）
    pub text: String,
    /// 顺带标记为已投递的岗位 id；没传 `jobId` 或标记失败时为空串
    #[serde(default)]
    pub job_id: String,
}

/// 自动投递：一次 `auto_greet_next` 的结果。
///
/// 后端是**无定时器的状态机**：每调一次最多发一条，回执里带 `next_at_ms`，
/// 前端据此安排下一次调用（旧版 `AutoGreetOne` 的对齐版）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AutoNextReply {
    /// 本次是否真的发出去了
    pub ok: bool,
    /// `pick` / `greet` / `send` / `mark` / `done`——区分「没岗位了」与「发失败」
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub skip: bool,
    #[serde(default)]
    pub job_id: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub title: String,
    /// 发出去的正文
    #[serde(default)]
    pub text: String,
    /// `rule` / `llm`
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tokens: u32,
    /// 下一次该在什么时刻（Unix 毫秒）再调；0 表示不必再调
    #[serde(default)]
    pub next_at_ms: u64,
    /// 距离下次调用的剩余毫秒（可直接做倒计时）
    #[serde(default)]
    pub remaining_ms: u64,
    #[serde(default)]
    pub sent: u32,
}

/// 自动投递运行状态（`auto_greet_status`）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AutoStatusReply {
    pub running: bool,
    pub paused: bool,
    #[serde(default)]
    pub sent: u32,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub next_at_ms: u64,
    #[serde(default)]
    pub remaining_ms: u64,
    /// 停止原因（「待投递列表已经处理完了」/「连续失败 3 次，已自动暂停」…）
    #[serde(default)]
    pub stop_reason: String,
    /// 待投递队列里还剩多少条
    #[serde(default)]
    pub pending: i64,
}

/// `rescore_jobs` 的参数。
///
/// 分页由调用方驱动：后端一次只处理 `limit` 条并回 `next_cursor`，
/// 调用方必须循环到 `has_more == false`。`after_job_id` 为 `None` 表示从头开始。
#[derive(Debug, Clone, Default, Serialize)]
pub struct RescoreArgs {
    pub after_job_id: Option<String>,
    pub limit: Option<u32>,
    pub use_llm: bool,
    pub llm_prefilter: Option<u32>,
    pub log_changes: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RescoreReply {
    pub scanned: u32,
    pub updated: u32,
    pub failed: u32,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    #[serde(default)]
    pub changes: Vec<RescoreChange>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RescoreChange {
    pub job_id: String,
    pub title: String,
    pub company: String,
    pub old_score: u8,
    pub new_score: u8,
    pub source: String,
    pub reason: String,
}

/// sensitive_remove() 的回执（pi 内部用来取 deleted）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoveReply {
    pub ok: bool,
    pub deleted: usize,
}
