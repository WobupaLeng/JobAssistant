/**
 * `commands_db.rs` 的命令，载荷形状逐条照抄后端签名，**不做统一化**（AGENTS.md C2）：
 * `fn foo(state, args: FooArgs)` 包一层 `{ args }`；平铺参数则键名与 Rust 参数名同名。
 * 命令名唯一真源是 `backend-tauri/src/lib.rs` 的 `generate_handler!`。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import { normalizeJob } from '../../lib_ui/job';
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
 * 列出岗位，带筛选、分页与排序。载荷 `{ args: ListJobsArgs }`，字段发**蛇形**。
 * `include_rejected` / `rejected_only` / `ascending` 三个布尔**恒发**：Rust 侧是 `bool`
 * 而非 `Option<bool>`，不发会退化成 `false`。其余可选字段由 `prune` 丢掉 `undefined`。
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
  }).then((rows) => rows.map((r) => normalizeJob(r)));
}

/** 取单个岗位。平铺 `{ jobId }`；岗位不存在时回 `null` 而不是报错，调用方须处理。 */
export function getJob(jobId: string): Promise<Job | null> {
  return invokeFlat<Job | null>('get_job', { jobId }).then((r) =>
    r === null ? null : normalizeJob(r),
  );
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
 * 对一份 JD 现场打分（不进库）。`args` 是 struct 参数，内层键名沿用 snake_case；
 * `{ args }` 这层由 `invokeArgs` 补。
 */
export function scoreJob(args: ScoreArgs): Promise<ScoreReply> {
  return invokeArgs<ScoreReply>('score_job', args);
}

/**
 * 用**给定门槛**判断一份 JD 是否过线（不入库、不调大模型）。参数平铺、没有 `args` 层，
 * 门槛键是 `minScore`，必须从 `get_scoring_spec` 取。
 */
export function passesThreshold(args: ScoreArgs, minScore: number): Promise<boolean> {
  return invokeFlat<boolean>('passes_threshold', { ...args, minScore });
}

/** 人工改分并留痕（写 `decision` 记录）。平铺驼峰 `{ jobId, score, source, reason }`。 */
export function updateScore(
  jobId: string,
  score: number,
  source: string,
  reason: string,
): Promise<void> {
  return invokeFlat<void>('update_score', { jobId, score, source, reason });
}

/**
 * 全量重算分数（分页驱动）。载荷 `{ args }`，键名驼峰（后端靠 `alias` 接受）。
 * `afterJobId` / `limit` / `llmPrefilter` 用显式 `null`（从头开始 / 用默认），与缺 key 语义
 * 不同；`useLlm` / `logChanges` 是 `bool`，恒发。
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
 * 批量改投递标记（三条命令共用）。空数组**短路返回 0、不发 IPC**，免得白跑一次 SQL。
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

/** 从「已投递」放回待投递（`status` 写回 `pending`），与 `allow_jobs` 对称。 */
export function unmarkDelivered(jobIds: readonly string[]): Promise<number> {
  return setJobFlag('unmark_delivered', jobIds);
}

/**
 * 读决策记录（新 → 旧）。两个游标互斥：`afterId` 追新，`beforeId` 向更旧翻页
 * （传上一页最后一条的 id）。
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

/** 各决策行为的条数。后端只按原值 `GROUP BY`，分档由调用方按 `decisionTone` 归属。 */
export function decisionCounts(): Promise<DecisionCount[]> {
  return invokeArgs<DecisionCount[]>('decision_counts', {});
}
