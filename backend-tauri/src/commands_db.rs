//! Tauri 命令层：前端与业务核心之间的唯一通道。
//!
//! **职责边界**：本模块只做三件事——接收参数、调用 core、把错误转成前端可读的字符串。
//! 任何业务规则都应落在 `jobassistant-core` 里，否则领域逻辑会重新散落到
//! 与 UI 耦合的位置（这正是重写前的病灶：`main.py` 里 40 处全局可变状态）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::core::domain::job::{Job, JobQuery, JobStore, ScoreSource, SortField};
use crate::core::domain::profile::DEFAULT_PROFILE;
use crate::core::domain::scoring::{rule_score_all, RuleScore};
use crate::core::domain::scoring_llm::{LlmClient, LLM_PREFILTER_DEFAULT};
use crate::core::domain::scoring_service::{score_job as score_job_entry, ScoreConfig, ScoreOutcome};
use crate::core::domain::token_meter::{TokenKind, TokenMeter};
use crate::core::infra::{HttpLlmClient, SqliteStore};
use serde::{Deserialize, Serialize};

/// 全局存储句柄。
///
/// 用 `Mutex<Option<...>>` 而不是直接持有：数据库路径要等 Tauri 解析出
/// `app_local_data_dir` 之后才知道，`setup` 阶段才能打开。
///
/// 内层是 `Arc` 而不是裸值：抓取会话的入库闭包必须 `Send + 'static`，
/// 它拿不到 `tauri::State`，只能克隆一份共享句柄带进消费线程。
pub struct StoreState(pub Mutex<Option<Arc<SqliteStore>>>);

/// 大模型客户端句柄。
///
/// 与 `StoreState` 一样用 `Mutex<Option<...>>`：密钥要从 `app_config` 表读，
/// 而那张表要等数据库打开之后才可读。**没配密钥不是错误**——应用照常以规则
/// 打分运行，只是 UI 上「启用大模型」的开关应保持不可用。
pub struct LlmState(pub Mutex<Option<HttpLlmClient>>);

impl LlmState {
    pub fn empty() -> Self {
        Self(Mutex::new(None))
    }
}

/// 从配置表构造大模型客户端。
///
/// 返回 `None` 表示「没有可用配置」——调用方据此把开关置灰，而不是报错。
/// 金额/模型等细节全部走配置，Rust 侧不写死任何密钥。
pub fn build_llm_client(
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
) -> Option<HttpLlmClient> {
    let key = api_key.filter(|k| !k.trim().is_empty())?;
    let base = base_url.filter(|u| !u.trim().is_empty());
    let name = model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| crate::core::infra::DEFAULT_MODEL.to_string());
    Some(HttpLlmClient::new(key, base, name))
}

/// 从配置键值对里取一个原文（去空白，空串视为没配）。
fn cfg_text<K: AsRef<str>, V: AsRef<str>>(pairs: &[(K, V)], want: &str) -> Option<String> {
    pairs
        .iter()
        .find(|(k, _)| k.as_ref() == want)
        .map(|(_, v)| v.as_ref().trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 连同温度一起构造客户端（启动与保存两条路径都用它）。
///
/// 温度必须跟密钥、模型在**同一次**读取里拿到，否则「保存了温度但要重启才
/// 生效」会变成只有用户能发现的问题。
pub fn build_llm_client_with_temperature<K: AsRef<str>, V: AsRef<str>>(
    pairs: &[(K, V)],
) -> Option<HttpLlmClient> {
    let (key, base, model) = pick_llm_config(pairs.iter().map(|(k, v)| (k, v)));
    let client = build_llm_client(key, base, model)?;
    Some(client.with_temperature_text(
        cfg_text(pairs, CFG_TEMPERATURE).as_deref(),
        cfg_text(pairs, CFG_GREET_TEMPERATURE).as_deref(),
    ))
}

/// 把内部错误转成前端字符串。
///
/// Tauri 命令的 `Err` 类型必须是 `Serialize`；用 `String` 最直接，
/// 但**不要**把带路径的底层错误原样抛给 UI——下面统一加前缀便于排查。
pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn store_err(e: impl std::fmt::Display) -> String {
    format!("存储错误：{e}")
}

/// 取存储句柄；未初始化时给出明确提示而不是 panic。
///
/// 克隆 `Arc` 后在锁外调用：抓取入库是唯一会让 SQLite 写入与列表查询
/// 抢锁的路径，把锁持有时间压到最短。
pub fn with_store<T>(
    state: &StoreState,
    f: impl FnOnce(&SqliteStore) -> CmdResult<T>,
) -> CmdResult<T> {
    let store = {
        let guard = state.0.lock().map_err(|_| "存储锁已中毒".to_string())?;
        guard.as_ref().map(Arc::clone)
    };
    let store = store.ok_or_else(|| "数据库尚未初始化".to_string())?;
    f(&store)
}

/// 取一份可带进后台线程的存储句柄（抓取会话入库用）。
///
/// 与 `with_store` 的区别：这里把 `Arc` 交出去，调用方在**自己的线程**里
/// 长期持有，不占用 `StoreState` 的锁。
pub fn store_handle(state: &StoreState) -> CmdResult<Arc<SqliteStore>> {
    let guard = state.0.lock().map_err(|_| "存储锁已中毒".to_string())?;
    guard
        .as_ref()
        .map(Arc::clone)
        .ok_or_else(|| "数据库尚未初始化".to_string())
}

// ---------- 数据层 ----------

/// 列表查询入参。字段全部可选，缺省即「不限制」。
///
/// **`alias` 不能删**（AGENTS.md 3.2）：前端发驼峰（`minScore` / `includeRejected`
/// / `rejectedOnly`），Rust 字段是蛇形，serde 对未知字段不报错——少了 alias，
/// 界面上勾的「包含不投递」「只看不投递」会被静默丢弃。
#[derive(Debug, Deserialize)]
pub struct ListJobsArgs {
    /// 状态页签：`pending` | `delivered` | `rejected`（旧版 `web/` 的 listTab）。
    ///
    /// 有值时**覆盖** `status` / `include_rejected` / `rejected_only` 三个字段，
    /// 页签的完整语义见 `JobQuery::for_tab`。
    #[serde(default)]
    pub tab: Option<String>,
    pub status: Option<String>,
    pub platform: Option<String>,
    #[serde(default, alias = "minScore")]
    pub min_score: Option<u8>,
    /// 是否包含被排除的岗位，默认 false
    #[serde(default, alias = "includeRejected")]
    pub include_rejected: bool,
    /// 只看被排除的岗位（旧版 `rejected_only`）
    #[serde(default, alias = "rejectedOnly")]
    pub rejected_only: bool,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    /// `score` | `time` | `salary` | `created` | `updated`
    pub sort: Option<String>,
    /// 显式次序：`asc` | `desc`。给了就以它为准，否则用 `ascending`。
    pub order: Option<String>,
    #[serde(default)]
    pub ascending: bool,
}

impl ListJobsArgs {
    fn into_query(self) -> JobQuery {
        // 页签优先：一个 `tab` 就决定 status 与两个否决开关，避免前端三处拼参数
        // 拼出「未投递却带否决位」这种自相矛盾的组合。
        let tabbed = self.tab.as_deref().map(JobQuery::for_tab);
        let ascending = match self.order.as_deref() {
            Some("asc") => true,
            Some("desc") => false,
            _ => self.ascending,
        };
        let base = tabbed.unwrap_or(JobQuery {
            status: self.status,
            platform: self.platform,
            min_score: self.min_score,
            include_rejected: self.include_rejected,
            rejected_only: self.rejected_only,
            // limit / offset / sort / ascending 统一在下面就位，这里只给默认值
            ..Default::default()
        });
        JobQuery {
            limit: self.limit.unwrap_or(100),
            offset: self.offset.unwrap_or(0),
            sort: SortField::parse(self.sort.as_deref().unwrap_or("score")),
            ascending,
            ..base
        }
    }
}

/// 读取岗位列表。
#[tauri::command]
pub fn list_jobs(state: tauri::State<'_, StoreState>, args: ListJobsArgs) -> CmdResult<Vec<Job>> {
    with_store(&state, |s| s.list_jobs(&args.into_query()).map_err(store_err))
}

/// 读取单条岗位。
#[tauri::command]
pub fn get_job(state: tauri::State<'_, StoreState>, job_id: String) -> CmdResult<Option<Job>> {
    with_store(&state, |s| s.get_job(&job_id).map_err(store_err))
}

// ---------- 三个状态页签：批量改判 + 计数 ----------

use crate::core::domain::store_ports::{DecisionRecord, DecisionStore, JobAdminStore};

/// 三个页签的条数（图 2 的「未投递 / 已投递 / 不投递」）。
///
/// 三条 COUNT 各自复用 `push_job_filters`，与列表用的是同一套 WHERE——
/// 否则会出现「页签说 18 条，点进去列表只有 17 条」这种无法解释的漂移。
#[tauri::command]
pub fn job_tab_counts(state: tauri::State<'_, StoreState>) -> CmdResult<crate::core::domain::JobTabCounts> {
    with_store(&state, |s| count_tabs(s).map_err(store_err))
}

fn count_tabs(s: &SqliteStore) -> Result<crate::core::domain::JobTabCounts, crate::core::infra::StoreError> {
    Ok(crate::core::domain::JobTabCounts {
        pending: s.count_jobs(&JobQuery::for_tab("pending"))?,
        delivered: s.count_jobs(&JobQuery::for_tab("delivered"))?,
        rejected: s.count_jobs(&JobQuery::for_tab("rejected"))?,
    })
}

/// 批量改判的统一入参。
///
/// `jobIds` 是前端驼峰写法；漏了 alias 会让「批量标记」按钮点了没反应
/// ——正是 AGENTS.md 3.2 记录的那种静默失效。
#[derive(Debug, Deserialize)]
pub struct JobIdsArgs {
    #[serde(default, alias = "jobIds")]
    pub job_ids: Vec<String>,
}

impl JobIdsArgs {
    /// 清洗：去空串、去重，但**保留顺序**（日志与回执按用户选择顺序更可读）。
    fn clean(self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in self.job_ids {
            let id = id.trim().to_string();
            if !id.is_empty() && !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }
}

/// 批量标记「不投递」。
#[tauri::command]
pub fn reject_jobs(state: tauri::State<'_, StoreState>, args: JobIdsArgs) -> CmdResult<usize> {
    let ids = args.clean();
    if ids.is_empty() {
        return Ok(0);
    }
    with_store(&state, |s| s.set_rejected(&ids, true).map_err(store_err))
}

/// 批量取消「不投递」。
#[tauri::command]
pub fn allow_jobs(state: tauri::State<'_, StoreState>, args: JobIdsArgs) -> CmdResult<usize> {
    let ids = args.clean();
    if ids.is_empty() {
        return Ok(0);
    }
    with_store(&state, |s| s.set_rejected(&ids, false).map_err(store_err))
}

/// 批量标记「已投递」。
#[tauri::command]
pub fn mark_delivered(state: tauri::State<'_, StoreState>, args: JobIdsArgs) -> CmdResult<usize> {
    let ids = args.clean();
    if ids.is_empty() {
        return Ok(0);
    }
    with_store(&state, |s| s.set_delivered(&ids, true).map_err(store_err))
}

/// 批量取消「已投递」（放回待投递）。与 `allow_jobs` 同一层壳，领域逻辑在 store。
#[tauri::command]
pub fn unmark_delivered(state: tauri::State<'_, StoreState>, args: JobIdsArgs) -> CmdResult<usize> {
    let ids = args.clean();
    if ids.is_empty() {
        return Ok(0);
    }
    with_store(&state, |s| s.set_delivered(&ids, false).map_err(store_err))
}

// ---------- 决策记录 ----------

/// 决策记录入参。
///
/// 两种翻页方向互斥：`afterId` 取「比它更新」的记录（追新），
/// `beforeId` 取「比它更旧」的记录（决策日志的无限滚动，传上一页最后一条的 id）。
#[derive(Debug, Deserialize)]
pub struct DecisionsArgs {
    pub limit: Option<u32>,
    #[serde(default, alias = "afterId")]
    pub after_id: Option<i64>,
    #[serde(default, alias = "beforeId")]
    pub before_id: Option<i64>,
}

/// 决策记录列表（分数变化优先取 old/new 列；旧行两列为 NULL）。
#[tauri::command]
pub fn list_decisions(
    state: tauri::State<'_, StoreState>,
    args: DecisionsArgs,
) -> CmdResult<Vec<DecisionRecord>> {
    let limit = args.limit.unwrap_or(0);
    match args.before_id {
        Some(before) => {
            with_store(&state, |s| s.list_decisions_before(limit, before).map_err(store_err))
        }
        None => {
            let after = args.after_id.unwrap_or(0);
            with_store(&state, |s| s.list_decisions(limit, after).map_err(store_err))
        }
    }
}

/// 决策日志总条数（界面「共 N 条」用，不受翻页条数影响）。
#[tauri::command]
pub fn count_decisions(state: tauri::State<'_, StoreState>) -> CmdResult<i64> {
    with_store(&state, |s| s.count_decisions().map_err(store_err))
}

/// 单条决策行为的条数（`decision` 是库里的原值，未做中文换算）。
#[derive(Debug, Serialize)]
pub struct DecisionCount {
    pub decision: String,
    pub count: i64,
}

/// 各决策行为的条数。分档（排除/入库/其它）由前端按 `decisionTone` 归属，
/// 后端只负责按原值 `GROUP BY`，避免两套口径漂移。
#[tauri::command]
pub fn decision_counts(state: tauri::State<'_, StoreState>) -> CmdResult<Vec<DecisionCount>> {
    let rows = with_store(&state, |s| s.decision_counts().map_err(store_err))?;
    Ok(rows
        .into_iter()
        .map(|(decision, count)| DecisionCount { decision, count })
        .collect())
}

/// 数据库概览：总条数、各打分制条数。用于 UI 顶部的数据健康提示。
#[derive(Debug, Serialize)]
pub struct DbOverview {
    pub total: usize,
    /// `("100", 21)` 形式：打分制标记 → 条数
    pub by_scale: Vec<(String, i64)>,
    /// 是否存在未迁移的旧制数据（UI 应提示用户执行迁移）
    pub has_legacy: bool,
}

#[tauri::command]
pub fn db_overview(state: tauri::State<'_, StoreState>) -> CmdResult<DbOverview> {
    with_store(&state, |s| {
        let all = s
            .list_jobs(&JobQuery {
                include_rejected: true,
                limit: 100_000,
                ..Default::default()
            })
            .map_err(store_err)?;
        let by_scale = s.count_by_scale().map_err(store_err)?;
        let has_legacy = by_scale
            .iter()
            .any(|(k, n)| *n > 0 && (k == "10" || k == "null"));
        Ok(DbOverview {
            total: all.len(),
            by_scale,
            has_legacy,
        })
    })
}

/// 执行十分制→百分制迁移。幂等，可安全重复调用。
#[tauri::command]
pub fn migrate_scores(state: tauri::State<'_, StoreState>) -> CmdResult<bool> {
    with_store(&state, |s| s.migrate().map_err(store_err))
}

// ---------- 打分 ----------

/// 打分入参。字段名与 Boss 列表页字段保持一致，便于抓取层直接透传。
///
/// `use_llm` / `llm_prefilter` / `filters` 三者都可省略，省略时按
/// 「纯规则、初筛线取默认值」处理，旧调用方无需改动。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScoreArgs {
    pub company: Option<String>,
    pub title: Option<String>,
    pub salary: Option<String>,
    pub experience: Option<String>,
    pub jd: Option<String>,
    /// 是否调用大模型。前端传 `useLlm` 或 `use_llm` 都能反序列化。
    #[serde(default, alias = "useLlm")]
    pub use_llm: Option<bool>,
    /// 规则初筛线；低于它的岗位不花 token
    #[serde(default, alias = "llmPrefilter")]
    pub llm_prefilter: Option<u8>,
    /// 用户自定义筛选条件，会拼进提示词
    #[serde(default)]
    pub filters: Option<String>,
}

impl ScoreArgs {
    /// 转成领域层的 `Job`。演示场景只填与打分相关的字段。
    fn to_job(&self) -> Job {
        Job {
            job_id: String::new(),
            platform: "boss".to_string(),
            status: "pending".to_string(),
            title: self.title.clone(),
            company: self.company.clone(),
            salary: self.salary.clone(),
            experience: self.experience.clone(),
            jd_text: self.jd.clone(),
            ..Default::default()
        }
    }

    fn to_config(&self) -> ScoreConfig {
        ScoreConfig {
            use_llm: self.use_llm.unwrap_or(false),
            llm_prefilter: self.llm_prefilter.unwrap_or(LLM_PREFILTER_DEFAULT),
            filters: self.filters.clone().unwrap_or_default(),
        }
    }
}

/// 打分结果，直接给前端展示。
#[derive(Debug, Serialize)]
pub struct ScoreReply {
    /// 0-100 分
    pub score: u8,
    pub reason: String,
    /// 无 JD 时被直接判不入库（SPEC-002 第 3.1 节）
    pub rejected_no_jd: bool,
    /// `rule` 或 `llm`——前端据此显示这一分是怎么来的
    pub source: String,
    /// 本次消耗的 token（规则打分为 0）
    pub tokens: u32,
}

impl From<RuleScore> for ScoreReply {
    fn from(r: RuleScore) -> Self {
        Self {
            score: r.score,
            reason: r.reason,
            rejected_no_jd: r.rejected_no_jd,
            source: "rule".to_string(),
            tokens: 0,
        }
    }
}

impl From<ScoreOutcome> for ScoreReply {
    fn from(o: ScoreOutcome) -> Self {
        Self {
            score: o.score,
            reason: o.reason,
            rejected_no_jd: o.rejected_no_jd,
            source: o.source.to_string(),
            tokens: o.tokens,
        }
    }
}

/// 统一打分入口：规则 → （可选）规则初筛 → （可选）大模型。
///
/// 未配置密钥时即使 `use_llm=true` 也自动退回规则分，不会报错，
/// 因为「没配密钥」是正常状态而不是故障。
#[tauri::command]
pub fn score_job(
    store_state: tauri::State<'_, StoreState>,
    llm: tauri::State<'_, LlmState>,
    args: ScoreArgs,
) -> ScoreReply {
    let guard = llm.0.lock().ok();
    // 显式标注成 trait 对象，否则 `Option<&HttpLlmClient>` 不会自动收窄
    let bare: Option<&dyn LlmClient> = guard
        .as_ref()
        .and_then(|g| g.as_ref())
        .map(|c| c as &dyn LlmClient);
    // 计费口径的累计装在客户端边界（ADR-005），打分决策与回传语义零改动。
    // 本命令签名不可失败：库还没初始化时降级为裸 client，只记一条 warn。
    let warn = |m: &str| crate::commands_logs::push("warn", m);
    let store = store_handle(&store_state).ok();
    let meter = match (bare, store.as_deref()) {
        (Some(c), Some(s)) => Some(TokenMeter::new(c, s, TokenKind::Score, &warn)),
        (Some(_), None) => {
            crate::commands_logs::push(
                "warn",
                "词元计数未计入：数据库尚未初始化（打分本身不受影响）",
            );
            None
        }
        (None, _) => None,
    };
    let client: Option<&dyn LlmClient> = match meter.as_ref() {
        Some(m) => Some(m as &dyn LlmClient),
        None => bare,
    };
    score_job_entry(&args.to_job(), &DEFAULT_PROFILE, &args.to_config(), client).into()
}

/// 是否达到门槛。门槛由调用方传入，避免前后端各存一份默认值。
#[tauri::command]
pub fn passes_threshold(args: ScoreArgs, min_score: u8) -> bool {
    let r = rule_score_all(
        args.company.as_deref(),
        args.title.as_deref(),
        args.salary.as_deref(),
        args.experience.as_deref(),
        args.jd.as_deref(),
        &DEFAULT_PROFILE,
    );
    r.passes(min_score)
}

/// 手动写入分数（前端「重新打分」按钮用）。
#[tauri::command]
pub fn update_score(
    state: tauri::State<'_, StoreState>,
    job_id: String,
    score: u8,
    source: String,
    reason: String,
) -> CmdResult<()> {
    // 越界分数一旦落库就再也分不清是「旧制没迁移」还是「写错了」，
    // 所以入口处直接拒绝，而不是静默 clamp。
    let max = crate::core::scoring::SCORE_MAX;
    if score > max {
        return Err(format!("分数 {score} 超出 0-{max} 范围"));
    }
    let src = ScoreSource::parse(&source)
        .ok_or_else(|| format!("未知打分来源：{source}（只接受 rule / llm）"))?;
    with_store(&state, |s| {
        s.update_score(&job_id, score, src, &reason).map_err(store_err)
    })
}

// ---------- 全量重算（SPEC-002 第 3.3 节） ----------

/// 重算入参。
///
/// 分页是**调用方驱动**的：界面上一批一批地算，每批返回一个游标，
/// 下一批带着游标继续。这样长时间重算不会让一个命令卡住事件循环，
/// 用户也能中途停下。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RescoreArgs {
    /// 上一批的 `next_cursor`；首批不传
    #[serde(default, alias = "afterJobId")]
    pub after_job_id: Option<String>,
    /// 本批最多算几条，默认 50，夹在 1..=500
    #[serde(default)]
    pub limit: Option<u32>,
    /// 是否走大模型。**默认 false**：全库走大模型是按条计费的操作，
    /// 必须由用户显式打开，不能因为上次的开关状态被静默继承。
    #[serde(default, alias = "useLlm")]
    pub use_llm: bool,
    #[serde(default, alias = "llmPrefilter")]
    pub llm_prefilter: Option<u8>,
    #[serde(default)]
    pub filters: Option<String>,
    /// 是否往 `jd_decisions` 写一条审计日志，默认 true
    #[serde(default = "default_true", alias = "logChanges")]
    pub log_changes: bool,
}

fn default_true() -> bool {
    true
}

impl RescoreArgs {
    fn into_options(self) -> crate::core::domain::rescore::RescoreOptions {
        crate::core::domain::rescore::RescoreOptions {
            after_job_id: self.after_job_id,
            limit: Some(self.limit.unwrap_or(50).clamp(1, 500)),
            use_llm: self.use_llm,
            llm_prefilter: self.llm_prefilter.unwrap_or(LLM_PREFILTER_DEFAULT),
            filters: self.filters.unwrap_or_default(),
            log_changes: self.log_changes,
        }
    }
}

/// 重算回复。字段与 `RescoreOutcome` 一一对应，前端不需要再做换算。
#[derive(Debug, Serialize)]
pub struct RescoreReply {
    /// 本批扫过多少条
    pub scanned: u32,
    /// 其中分数真的变了的条数
    pub updated: u32,
    /// 写库失败的条数（不中断批次）
    pub failed: u32,
    /// 下一批的游标；`None` 表示已到底
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub changes: Vec<RescoreChangeReply>,
}

#[derive(Debug, Serialize)]
pub struct RescoreChangeReply {
    pub job_id: String,
    pub title: Option<String>,
    pub company: Option<String>,
    /// 旧分数；旧制数据迁移前可能为空
    pub old_score: Option<u8>,
    pub new_score: u8,
    /// `rule` / `llm`
    pub source: String,
    pub reason: String,
}

/// 用当前口径重算一批岗位的分数。
///
/// 迁移到百分制之后，存量分数仍是旧口径算出来的，必须提供这个入口
/// 让用户主动重算（SPEC-002 第 3.3 节）。**只改分数，不改排除结论**——
/// 是否被排除由 `rejected` 列决定，重算不碰它，避免「重算一次把用户手动
/// 放行的岗位又踢掉」。
#[tauri::command]
pub fn rescore_jobs(
    state: tauri::State<'_, StoreState>,
    llm: tauri::State<'_, LlmState>,
    args: RescoreArgs,
) -> CmdResult<RescoreReply> {
    let opts = args.into_options();
    with_store(&state, |s| {
        let guard = llm.0.lock().ok();
        let bare: Option<&dyn LlmClient> = if opts.use_llm {
            guard
                .as_ref()
                .and_then(|g| g.as_ref())
                .map(|c| c as &dyn LlmClient)
        } else {
            None
        };
        // 计费口径的累计装在客户端边界（ADR-005）：全量重算的逐条调用每轮都计。
        // `with_store` 已保证库已初始化；计数落库失败只 warn，不失败业务。
        let warn = |m: &str| crate::commands_logs::push("warn", m);
        let meter = bare.map(|c| TokenMeter::new(c, s, TokenKind::Score, &warn));
        let client: Option<&dyn LlmClient> = match meter.as_ref() {
            Some(m) => Some(m as &dyn LlmClient),
            None => bare,
        };
        let out = crate::core::domain::rescore::rescore_batch(
            s,
            &DEFAULT_PROFILE,
            &opts,
            client,
        );
        Ok(out.into())
    })
}

impl From<crate::core::domain::rescore::RescoreOutcome> for RescoreReply {
    fn from(o: crate::core::domain::rescore::RescoreOutcome) -> Self {
        Self {
            scanned: o.scanned,
            updated: o.updated,
            failed: o.failed,
            has_more: o.has_more(),
            next_cursor: o.next_cursor,
            changes: o
                .changes
                .into_iter()
                .map(|c| RescoreChangeReply {
                    job_id: c.job_id,
                    title: c.title,
                    company: c.company,
                    old_score: c.old_score,
                    new_score: c.new_score,
                    source: c.source.to_string(),
                    reason: c.reason,
                })
                .collect(),
        }
    }
}

// ---------- 初始化 ----------

/// 打开数据库并完成补列。由 `setup` 阶段调用。
///
/// 路径由调用方给出，本函数不猜测目录——测试可以传临时路径。
pub fn open_store(db_path: PathBuf) -> Result<Arc<SqliteStore>, String> {
    if let Some(parent) = db_path.parent() {
        // 目录不存在时 SQLite 只会报「unable to open database file」，
        // 这里先建好并给出可读的错误
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("无法创建数据目录 {}：{e}", parent.display()))?;
    }
    SqliteStore::open(&db_path)
        .map(Arc::new)
        .map_err(|e| format!("无法打开数据库 {}：{e}", db_path.display()))
}

/// 大模型相关配置在 `app_config` 表里的键名。
pub const CFG_API_KEY: &str = "api_key";
pub const CFG_BASE_URL: &str = "base_url";
pub const CFG_MODEL: &str = "model";
/// 打分温度
pub const CFG_TEMPERATURE: &str = "temperature";
/// 话术温度
pub const CFG_GREET_TEMPERATURE: &str = "greet_temperature";

/// 从 `app_config` 的键值对里挑出大模型需要的三项。
///
/// 抽成不碰数据库的纯函数，是为了能在没有 SQLite 的环境下单测。
pub fn pick_llm_config<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> (Option<String>, Option<String>, Option<String>)
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut key = None;
    let mut base = None;
    let mut model = None;
    for (k, v) in pairs {
        let val = v.as_ref().trim().to_string();
        if val.is_empty() {
            continue;
        }
        match k.as_ref() {
            CFG_API_KEY => key = Some(val),
            CFG_BASE_URL => base = Some(val),
            CFG_MODEL => model = Some(val),
            _ => {}
        }
    }
    (key, base, model)
}

/// 前端用来展示「大模型是否已配置」。
#[derive(Debug, Serialize)]
pub struct LlmStatus {
    /// 是否已有可用客户端（有密钥才算）
    pub configured: bool,
    /// 脱敏后的密钥尾号，未配置时为 `None`；**绝不返回明文**
    pub key_hint: Option<String>,
    pub model: Option<String>,
    /// 默认初筛线，前端不该自己写死
    pub prefilter_default: u8,
}

#[tauri::command]
pub fn llm_status(state: tauri::State<'_, StoreState>) -> CmdResult<LlmStatus> {
    // 数据库没打开也算「未配置」，不报错——过早失败会让整块 UI 不可用
    let store = match state.0.lock() {
        Ok(g) => g.as_ref().map(|s| s.config_pairs().ok()).flatten(),
        Err(_) => None,
    };
    let (key, _base, model) = match store {
        Some(pairs) => pick_llm_config(pairs),
        None => (None, None, None),
    };
    let key_hint = key.as_ref().map(|k| {
        let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("****{tail}")
    });
    Ok(LlmStatus {
        configured: key.is_some(),
        key_hint,
        model,
        prefilter_default: LLM_PREFILTER_DEFAULT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的打分辅助：不带 `State`，直接走领域层，避免构造 Tauri 运行时。
    fn score_raw(args: ScoreArgs) -> ScoreReply {
        score_job_entry(&args.to_job(), &DEFAULT_PROFILE, &args.to_config(), None).into()
    }

    fn demo_args(jd: Option<&str>) -> ScoreArgs {
        ScoreArgs {
            company: Some("某科技有限公司".into()),
            title: Some("Java 后端开发".into()),
            salary: Some("20-40K".into()),
            experience: Some("5-10年".into()),
            jd: jd.map(|s| s.to_string()),
            use_llm: None,
            llm_prefilter: None,
            filters: None,
        }
    }

    #[test]
    fn list_args_default_to_score_sort() {
        let args = ListJobsArgs {
            tab: None,
            status: None,
            platform: None,
            min_score: None,
            include_rejected: false,
            rejected_only: false,
            limit: None,
            offset: None,
            sort: None,
            order: None,
            ascending: false,
        };
        let q = args.into_query();
        assert_eq!(q.sort, SortField::Score);
        assert_eq!(q.limit, 100);
        assert_eq!(q.offset, 0);
        assert!(!q.rejected_only, "默认绝不能带否决位");
    }

    #[test]
    fn list_args_reject_injection_in_sort() {
        // 排序字段只接受白名单，其余一律降级为 score
        let args = ListJobsArgs {
            tab: None,
            status: None,
            platform: None,
            min_score: None,
            include_rejected: false,
            rejected_only: false,
            limit: Some(10),
            offset: None,
            sort: Some("match_score; DROP TABLE job_queue".into()),
            order: None,
            ascending: false,
        };
        assert_eq!(args.into_query().sort, SortField::Score);
    }

    fn raw_args(v: serde_json::Value) -> ListJobsArgs {
        serde_json::from_value(v).expect("入参应可反序列化")
    }

    #[test]
    fn list_args_accept_camel_case_flags() {
        // 前端发驼峰：少了 alias 这几个开关会被 serde 静默丢弃
        let q = raw_args(serde_json::json!({
            "minScore": 70,
            "includeRejected": true,
            "rejectedOnly": true,
            "limit": 20
        }))
        .into_query();
        assert_eq!(q.min_score, Some(70));
        assert!(q.include_rejected);
        assert!(q.rejected_only);
        assert_eq!(q.limit, 20);
    }

    #[test]
    fn list_args_snake_and_camel_agree() {
        let camel = raw_args(serde_json::json!({"minScore": 60, "includeRejected": true})).into_query();
        let snake = raw_args(serde_json::json!({"min_score": 60, "include_rejected": true})).into_query();
        assert_eq!(camel.min_score, snake.min_score);
        assert_eq!(camel.include_rejected, snake.include_rejected);
    }

    #[test]
    fn tab_overrides_status_and_rejection_flags() {
        // 页签优先：即使前端同时发了 status/rejectedOnly，delivered 页签也必须
        // 是「已投递」而不是被判成「只看否决」
        let q = raw_args(serde_json::json!({
            "tab": "delivered",
            "status": "pending",
            "rejectedOnly": true,
            "includeRejected": false
        }))
        .into_query();
        assert_eq!(q.status.as_deref(), Some("delivered"));
        assert!(!q.rejected_only);
        assert!(q.include_rejected);

        let rejected = raw_args(serde_json::json!({"tab": "rejected"})).into_query();
        assert!(rejected.rejected_only, "不投递页签必须只取被判否决的");

        let pending = raw_args(serde_json::json!({"tab": "pending"})).into_query();
        assert_eq!(pending.status.as_deref(), Some("pending"));
        assert!(!pending.include_rejected);
    }

    #[test]
    fn order_string_wins_over_ascending_bool() {
        let asc = raw_args(serde_json::json!({"order": "asc", "ascending": false})).into_query();
        assert!(asc.ascending);
        let desc = raw_args(serde_json::json!({"order": "desc", "ascending": true})).into_query();
        assert!(!desc.ascending);
        // 没给 order 时退回布尔字段，保持旧调用方不变
        let legacy = raw_args(serde_json::json!({"ascending": true})).into_query();
        assert!(legacy.ascending);
    }

    #[test]
    fn sort_accepts_legacy_time_key() {
        assert_eq!(raw_args(serde_json::json!({"sort": "time"})).into_query().sort, SortField::Created);
        assert_eq!(raw_args(serde_json::json!({"sort": "salary"})).into_query().sort, SortField::Salary);
    }

    #[test]
    fn job_ids_accept_camel_and_clean_blanks() {
        let cleaned = serde_json::from_value::<JobIdsArgs>(serde_json::json!({
            "jobIds": ["a", " ", "b", "a"]
        }))
        .expect("驼峰 jobIds 应可解析")
        .clean();
        assert_eq!(cleaned, vec!["a".to_string(), "b".to_string()], "空串要去掉、重复要合并");
    }

    #[test]
    fn job_ids_empty_list_is_noop() {
        // 空列表是「用户还没选任何卡片」，不是错误；命令层直接返回 0 不去碰库
        let cleaned = JobIdsArgs { job_ids: Vec::new() }.clean();
        assert!(cleaned.is_empty());
        let missing = serde_json::from_value::<JobIdsArgs>(serde_json::json!({}))
            .expect("缺 jobIds 应按空列表处理")
            .clean();
        assert!(missing.is_empty(), "缺字段不能报错，否则前端首帧会弹红条");
    }

    #[test]
    fn decisions_args_accept_camel_after_id() {
        let args = serde_json::from_value::<DecisionsArgs>(serde_json::json!({
            "limit": 20,
            "afterId": 42
        }))
        .expect("驼峰 afterId 应可解析");
        assert_eq!(args.limit, Some(20));
        assert_eq!(args.after_id, Some(42));
        let dflt = serde_json::from_value::<DecisionsArgs>(serde_json::json!({}))
            .expect("全空应取默认值");
        assert_eq!(dflt.limit, None);
        assert_eq!(dflt.after_id, None);
        assert_eq!(dflt.before_id, None);
    }

    #[test]
    fn decisions_args_accept_camel_before_id() {
        let args = serde_json::from_value::<DecisionsArgs>(serde_json::json!({
            "limit": 10,
            "beforeId": 7
        }))
        .expect("驼峰 beforeId 应可解析");
        assert_eq!(args.limit, Some(10));
        assert_eq!(args.before_id, Some(7));
        assert_eq!(args.after_id, None);
    }

    #[test]
    fn tab_counts_start_at_zero_on_empty_db() {
        // 空库边界：三个页签全 0，且不因「库是空的」而报错
        let store = SqliteStore::open_in_memory().expect("内存库应可打开");
        let counts = count_tabs(&store).expect("计数不应失败");
        assert_eq!(counts, crate::core::domain::JobTabCounts::default());
    }

    #[test]
    fn score_job_returns_100_scale() {
        // JD 从画像动态拼，不写死技能名——画像由用户随时调整
        let jd = format!(
            "负责后端服务开发，熟悉 {}。",
            DEFAULT_PROFILE.core_skills.join("、")
        );
        let reply = score_raw(demo_args(Some(&jd)));
        assert!(reply.score <= 100, "分数不得超过 100：{}", reply.score);
        assert!(!reply.reason.is_empty(), "理由不应为空");
        assert_eq!(reply.source, "rule", "未启用大模型时应走规则");
        assert_eq!(reply.tokens, 0, "规则打分为不消耗 token");
    }

    #[test]
    fn no_jd_is_rejected_explicitly() {
        // 无 JD 时不应靠调低分数来否决，而是显式标记
        let reply = score_raw(demo_args(None));
        assert!(reply.rejected_no_jd, "无 JD 应被显式否决");
    }

    #[test]
    fn use_llm_without_client_falls_back_to_rule() {
        // 没配密钥却把开关打开，属于正常状态：退回规则分，不报错
        let mut args = demo_args(Some("负责后端服务开发，熟悉 Java、MySQL。"));
        args.use_llm = Some(true);
        let reply = score_raw(args);
        assert_eq!(reply.source, "rule");
    }

    #[test]
    fn score_args_accept_both_camel_and_snake_case() {
        let camel: ScoreArgs = serde_json::from_str(r#"{"useLlm":true,"llmPrefilter":30}"#)
            .expect("useLlm 应能被反序列化");
        assert_eq!(camel.use_llm, Some(true));
        assert_eq!(camel.llm_prefilter, Some(30));

        let snake: ScoreArgs = serde_json::from_str(r#"{"use_llm":true,"llm_prefilter":40}"#)
            .expect("use_llm 也应能被反序列化");
        assert_eq!(snake.use_llm, Some(true));
        assert_eq!(snake.llm_prefilter, Some(40));
    }

    #[test]
    fn score_config_defaults_when_args_omit_llm_fields() {
        let cfg = demo_args(None).to_config();
        assert!(!cfg.use_llm, "缺省应为纯规则");
        assert_eq!(cfg.llm_prefilter, LLM_PREFILTER_DEFAULT);
        assert!(cfg.filters.is_empty());
    }

    #[test]
    fn build_llm_client_skips_blank_key() {
        assert!(build_llm_client(None, None, None).is_none());
        assert!(build_llm_client(Some("   ".into()), None, None).is_none());
        assert!(build_llm_client(Some("sk-test".into()), None, None).is_some());
    }

    #[test]
    fn client_from_pairs_reads_both_temperatures() {
        let pairs = vec![
            (CFG_API_KEY, "sk-test"),
            (CFG_MODEL, "deepseek-chat"),
            (CFG_TEMPERATURE, "0.35"),
            (CFG_GREET_TEMPERATURE, "1.1"),
        ];
        let c = build_llm_client_with_temperature(&pairs).expect("有密钥就该建出客户端");
        assert_eq!(c.score_temperature(), 0.35);
        assert_eq!(c.greet_temperature(), 1.1);
    }

    #[test]
    fn client_from_pairs_without_temperature_uses_defaults() {
        let pairs = vec![(CFG_API_KEY, "sk-test"), (CFG_TEMPERATURE, "  ")];
        let c = build_llm_client_with_temperature(&pairs).unwrap();
        assert_eq!(c.score_temperature(), crate::core::infra::SCORE_TEMPERATURE);
        assert_eq!(c.greet_temperature(), crate::core::infra::GREET_TEMPERATURE);
        // 没密钥就没有客户端：温度不能凭空造出一个
        assert!(build_llm_client_with_temperature(&[(CFG_TEMPERATURE, "1.0")]).is_none());
    }

    #[test]
    fn update_score_rejects_unknown_source() {
        // 直接构造 state 较繁琐，这里只验证来源解析的分支
        assert!(ScoreSource::parse("magic").is_none());
    }

    // ---------- 全量重算三条铁律：默认不烧 token、分页夹紧、幂等 ----------

    #[test]
    fn rescore_args_default_to_rule_only() {
        // 全库走大模型是按条计费的操作，缺省必须是纯规则
        let opts = serde_json::from_str::<RescoreArgs>("{}")
            .expect("空对象应能反序列化")
            .into_options();
        assert!(!opts.use_llm, "缺省不得启用大模型");
        assert_eq!(opts.limit, Some(50));
        assert!(opts.log_changes, "缺省应写审计日志");
        assert!(opts.after_job_id.is_none());
    }

    #[test]
    fn rescore_args_clamp_limit() {
        let zero = serde_json::from_str::<RescoreArgs>(r#"{"limit":0}"#).unwrap();
        assert_eq!(zero.into_options().limit, Some(1), "0 会让续算原地打转");
        let huge = serde_json::from_str::<RescoreArgs>(r#"{"limit":99999}"#).unwrap();
        assert_eq!(huge.into_options().limit, Some(500));
    }

    #[test]
    fn rescore_args_accept_both_camel_and_snake_case() {
        // serde 对未知字段不报错：漏了 alias 会让界面设的开关被静默丢弃
        let camel: RescoreArgs =
            serde_json::from_str(r#"{"useLlm":true,"llmPrefilter":30,"afterJobId":"j9","logChanges":false}"#)
                .expect("驼峰应能被反序列化");
        let o = camel.into_options();
        assert!(o.use_llm);
        assert_eq!(o.llm_prefilter, 30);
        assert_eq!(o.after_job_id.as_deref(), Some("j9"));
        assert!(!o.log_changes, "logChanges:false 不得被静默丢弃成默认 true");
    }

    #[test]
    fn rescore_reply_maps_outcome_fields() {
        let store = SqliteStore::open_in_memory().expect("内存库");
        // 先塞一条旧口径（十分制）的岗位，重算后分数应被改写并打上新制标记
        let job = Job {
            job_id: "old1".into(),
            platform: "boss".into(),
            title: Some("Java 后端开发".into()),
            company: Some("某科技有限公司".into()),
            salary: Some("20-40K".into()),
            experience: Some("5-10年".into()),
            jd_text: Some(format!(
                "负责后端服务，熟悉 {}。",
                DEFAULT_PROFILE.core_skills.join("、")
            )),
            match_score: Some(7),
            ..Default::default()
        };
        assert!(store.insert_job(&job).unwrap());

        let opts = crate::core::domain::rescore::RescoreOptions::default();
        let out = crate::core::domain::rescore::rescore_batch(
            &store,
            &DEFAULT_PROFILE,
            &opts,
            None,
        );
        let reply: RescoreReply = out.into();
        assert_eq!(reply.scanned, 1);
        assert_eq!(reply.updated, 1, "7 分旧制应被重算成百分制新分");
        assert_eq!(reply.failed, 0);
        assert!(!reply.has_more);
        assert!(reply.next_cursor.is_none(), "扫完一屏应明确到底");
        let change = &reply.changes[0];
        assert_eq!(change.job_id, "old1");
        assert_eq!(change.old_score, Some(7));
        assert!(change.new_score > 7, "百分制分数应高于旧十分制");
        assert_eq!(change.source, "rule");

        let after = store.get_job("old1").unwrap().expect("应仍在库");
        assert_eq!(after.score_scale.as_deref(), Some("100"), "应打上新制标记");
    }

    #[test]
    fn open_store_creates_parent_dir() {
        let dir = std::env::temp_dir().join(format!("aja_cmd_{}", std::process::id()));
        let db = dir.join("nested").join("test.db");
        let store = open_store(db.clone()).expect("应能建目录并打开");
        drop(store);
        assert!(db.exists(), "数据库文件应被创建");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
