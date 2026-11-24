/**
 * `commands_db.rs` 的 15 条命令。
 *
 * 载荷形状**逐条照抄** `backend-tauri/src/commands_db.rs` 的签名与
 * `src/api/mod.rs`（Leptos 版）的构造方式，**不做统一化**（AGENTS.md C2）：
 * - 命令层是 `fn foo(state, args: FooArgs)` → 包一层 `{ args }`；
 * - 命令层是平铺参数 → 键名与 Rust 参数名同名（需要驼峰的由 `#[serde(alias)]` 兜）；
 * - `score_job(args: ScoreArgs)` 是**例外**：整个 struct 就是载荷，没有 `args` 层。
 *
 * 命令名唯一真源是 `backend-tauri/src/lib.rs` 的 `generate_handler!`。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import type {
  DbOverview,
  DecisionRecord,
  Job,
  JobTabCounts,
  ListJobsArgs,
  LlmStatus,
  RescoreArgs,
  RescoreReply,
  ScoreArgs,
  ScoreReply,
} from '../types';

/**
 * 列出岗位，带筛选、分页与排序。
 *
 * 载荷结构：`{ args: ListJobsArgs }`。
 *
 * 两处细节都不能省：
 * 1. 字段名是**蛇形**（`min_score` / `include_rejected`），因为结构体本身没有
 *    `rename_all`，只有这两个字段带驼峰 `alias`；照 Leptos 版发蛇形最稳。
 * 2. `include_rejected` / `rejected_only` / `ascending` 三个布尔**恒发**：
 *    Rust 侧它们是 `bool` 而不是 `Option<bool>`，前端不发就会退化成 `false`，
 *    而「页签给了但 include_rejected 被吞掉」会列出看不懂的混合列表。
 *    其余可选字段由 `prune` 丢掉 `undefined`（ARCH-003 §5.2）。
 */
export function listJobs(args: ListJobsArgs = {}): Promise<Job[]> {
  return invokeArgs<Job[]>('list_jobs', {
    status: args.status,
    platform: args.platform,
    sort: args.sort,
    tab: args.tab,
    order: args.order,
    min_score: args.min_score,
    limit: args.limit,
    offset: args.offset,
    include_rejected: args.include_rejected === true,
    rejected_only: args.rejected_only === true,
    ascending: args.ascending === true,
  });
}

/**
 * 取单个岗位。
 *
 * 平铺驼峰 `{ jobId }`；岗位不存在时后端回 `null`（不是报错），
 * 所以调用方必须处理 `null`。
 */
export function getJob(jobId: string): Promise<Job | null> {
  return invokeFlat<Job | null>('get_job', { jobId });
}

/** 库概览：总数 + 按公司规模分组 + 是否有历史遗留分数。 */
export function dbOverview(): Promise<DbOverview> {
  return invokeNone<DbOverview>('db_overview');
}

/** 把一个库内的分数列迁到当前量纲（旧版 8 分制 → 百分制）。返回是否发生过迁移。 */
export function migrateScores(): Promise<boolean> {
  return invokeNone<boolean>('migrate_scores');
}

/**
 * 对一份 JD 现场打分（不进库）。
 *
 * 后端签名是 `fn score_job(llm: State<..>, args: ScoreArgs)`——`args` 是
 * **struct 参数**，Tauri 2 的命令参数 key 恒取参数的 lowerCamelCase 名，
 * 所以要包一层 `{ args: {...} }`。此前这里平铺发送，缺 `args` 键会被
 * tauri-macros 直接拒掉（`command score_job missing required key args`）。
 * 结构体内的键名沿用 `ScoreArgs` 的 `rename_all = "snake_case"`，故内层仍是蛇形。
 */
export function scoreJob(args: ScoreArgs): Promise<ScoreReply> {
  return invokeArgs<ScoreReply>('score_job', args);
}

/**
 * 用**给定门槛**判断一份 JD 是否过线（不入库、不调用大模型）。
 *
 * 后端签名 `fn passes_threshold(args: ScoreArgs, min_score: u8)`：两个都是
 * **标量/结构体参数本身**（没有 `State` 参），Tauri 2 按参数名拆成平铺的
 * lowerCamelCase 键——结构体字段仍平铺在这一层（`company`/`title`/…），
 * 标量 `min_score` 的键是 `minScore`。**这里没有 `args` 包裹层。**
 * 门槛由调用方从 `get_scoring_spec` 取来（AGENTS.md C10：前端不得硬编码门槛）。
 */
export function passesThreshold(args: ScoreArgs, minScore: number): Promise<boolean> {
  return invokeFlat<boolean>('passes_threshold', { ...args, minScore });
}

/**
 * 人工改分并留痕（写 `decision` 记录）。
 *
 * 平铺驼峰：`{ jobId, score, source, reason }`。
 */
export function updateScore(
  jobId: string,
  score: number,
  source: string,
  reason: string,
): Promise<void> {
  return invokeFlat<void>('update_score', { jobId, score, source, reason });
}

/**
 * 全量重算分数（分页驱动）。
 *
 * 载荷结构：`{ args: {...} }`，键名驼峰（`RescoreArgs` 是 snake_case 结构体，
 * 靠 `alias` 接受驼峰）。
 *
 * `afterJobId` / `limit` / `llmPrefilter` 用**显式 `null`**：`null` 表示「从头开始 /
 * 用默认」，与「缺 key」在语义上不同（缺 key 会让调用方以为没传这轮参数）。
 * `useLlm` / `logChanges` 是 `bool`，恒发。
 * `filters` 只在有值时出现（`Option<String>` 且无 default 之外的特殊语义）。
 */
export function rescoreJobs(args: RescoreArgs = {}): Promise<RescoreReply> {
  return invokeArgs<RescoreReply>('rescore_jobs', {
    afterJobId: args.after_job_id ?? null,
    limit: args.limit ?? null,
    useLlm: args.use_llm === true,
    llmPrefilter: args.llm_prefilter ?? null,
    filters: args.filters ?? undefined,
    logChanges: args.log_changes === true,
  });
}

/** 大模型客户端状态（是否配了 key、模型名、默认预筛档位）。 */
export function llmStatus(): Promise<LlmStatus> {
  return invokeNone<LlmStatus>('llm_status');
}

/** 三个页签的条数。概览页按 5 秒轮询它。 */
export function jobTabCounts(): Promise<JobTabCounts> {
  return invokeNone<JobTabCounts>('job_tab_counts');
}

/**
 * 批量改投递标记（`reject_jobs` / `allow_jobs` / `mark_delivered` 三条共用）。
 *
 * 载荷结构：`{ args: { jobIds } }`。空数组**直接短路返回 0、不发 IPC**：
 * 后端也会回 0，但「什么都没选」本来就不该跑一次 SQL。
 */
async function setJobFlag(cmd: string, jobIds: readonly string[]): Promise<number> {
  if (jobIds.length === 0) return 0;
  return invokeArgs<number>(cmd, { jobIds: [...jobIds] });
}

/** 标为「不投递」。返回受影响条数。 */
export function rejectJobs(jobIds: readonly string[]): Promise<number> {
  return setJobFlag('reject_jobs', jobIds);
}

/** 从「不投递」放回待投递。返回受影响条数。 */
export function allowJobs(jobIds: readonly string[]): Promise<number> {
  return setJobFlag('allow_jobs', jobIds);
}

/** 标为「已投递」。返回受影响条数。 */
export function markDelivered(jobIds: readonly string[]): Promise<number> {
  return setJobFlag('mark_delivered', jobIds);
}

/**
 * 从「已投递」放回待投递（`status` 写回 `pending`）。
 *
 * 后端 `set_delivered(&ids, false)` 早已实现（`sqlite/admin.rs`），只差一条命令壳，
 * 与 `allow_jobs` 对称。
 */
export function unmarkDelivered(jobIds: readonly string[]): Promise<number> {
  return setJobFlag('unmark_delivered', jobIds);
}

/**
 * 读决策记录（新 → 旧）。
 *
 * 载荷结构：`{ args: { limit?, afterId?, beforeId? } }`——驼峰**是** `afterId` / `beforeId`，
 * 不是蛇形。两个游标互斥：`afterId` 追新（取 `id > afterId`），
 * `beforeId` 向更旧翻页（取 `id < beforeId`，传上一页最后一条的 id）。
 */
export function listDecisions(
  limit?: number | null,
  afterId?: number | null,
  beforeId?: number | null,
): Promise<DecisionRecord[]> {
  return invokeArgs<DecisionRecord[]>('list_decisions', { limit, afterId, beforeId });
}

/** 决策日志总条数（不受翻页条数影响，界面显示「共 N 条」用）。 */
export function countDecisions(): Promise<number> {
  return invokeArgs<number>('count_decisions', {});
}

/** 单条决策行为的条数（`decision` 是库里的原值）。 */
export interface DecisionCount {
  decision: string;
  count: number;
}

/**
 * 各决策行为的条数。
 *
 * 分档（排除/入库/其它）由调用方按 `decisionTone` 归属：后端只按原值
 * `GROUP BY`，两套口径不会漂移。
 */
export function decisionCounts(): Promise<DecisionCount[]> {
  return invokeArgs<DecisionCount[]>('decision_counts', {});
}
