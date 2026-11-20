/**
 * Tauri 命令层：**唯一**允许调用 `invoke` 的地方。
 *
 * 分层约定（依赖单向：`features → api → lib`）：
 * - `types.ts` —— 契约类型，与 Rust 侧逐字段对齐，**不含任何运行时代码**；
 * - `index.ts` —— 本文件，只做「参数整形 → invoke」；
 * - `mock.ts` —— 命令缺失时的演示替身，**永不写回真库**。
 *
 * 契约真源：`core/src/domain/job.rs`（Job）、`core/src/sidecar/protocol.rs`
 * （聊天/期望）、`app/src-tauri/src/commands_*.rs`（命令签名）。
 * 改签名必须同一次提交内改本文件与该组件的调用处。
 */

import { invoke } from '@tauri-apps/api/core';
// 仅供本文件内部使用（`api` 对象的返回值标注）
import type {
  AppConfig,
  AppConfigPatch,
  AppInfo,
  DecisionLog,
  DbOverview,
  Expectation,
  FriendsReply,
  GreetArgs,
  GreetReply,
  Job,
  ListJobsArgs,
  LlmStatus,
  MessagesReply,
  OkReply,
  ProgressReply,
  RescoreArgs,
  RescoreReply,
  ResumesReply,
  ScoreArgs,
  ScoreReply,
  ScoringSpec,
  ScrapeUpdate,
  SendReply,
  SensitiveRow,
  SortField,
} from './types';

/**
 * 类型出境出口。
 *
 * 为什么要有这一段：`types.ts` 拆出去之后，各组件原本写的是
 * `import { type Job } from '../api'`。若要求它们改指向 `'../api/types'`，
 * 就泄漏了「契约分几个文件」这个内部细节，分层反而更脆。
 * 这里统一再导出，调用方永远只认识 `api` 这一个入口。
 */
export type {
  AppConfig,
  AppConfigPatch,
  AppExcludes,
  AppFilters,
  AppInfo,
  ChatMessage,
  DbOverview,
  DecisionLog,
  Expectation,
  Friend,
  FriendsReply,
  GreetArgs,
  GreetReply,
  Job,
  ListJobsArgs,
  LlmStatus,
  MessagesReply,
  OkReply,
  ProgressReply,
  RescoreArgs,
  RescoreChange,
  RescoreReply,
  ResumeItem,
  ResumesReply,
  ScoreArgs,
  ScoreReply,
  ScoringSpec,
  ScrapeReply,
  ScrapeUpdate,
  SendReply,
  SensitiveRow,
  SortField,
} from './types';

export const api = {
  // ---- 应用与口径 ----
  appInfo: () => invoke<AppInfo>('app_info'),
  scoringSpec: () => invoke<ScoringSpec>('get_scoring_spec'),
  llmStatus: () => invoke<LlmStatus>('llm_status'),

  // ---- 岗位 ----
  listJobs: (args: ListJobsArgs) => invoke<Job[]>('list_jobs', { args }),
  getJob: (jobId: string) => invoke<Job | null>('get_job', { jobId }),
  dbOverview: () => invoke<DbOverview>('db_overview'),
  migrateScores: () => invoke<boolean>('migrate_scores'),
  scoreJob: (args: ScoreArgs) => invoke<ScoreReply>('score_job', { args }),
  rescoreJobs: (args: RescoreArgs) => invoke<RescoreReply>('rescore_jobs', { args }),

  // ---- 抓取 ----
  scrapeStart: (args: {
    keywords?: string[];
    minScore?: number;
    maxJd?: number;
    platform?: string;
  }) => invoke<{ pid: number; min_score: number; keywords: string[]; max_jd: number | null }>('scrape_start', { args }),
  scrapeStop: () => invoke<void>('scrape_stop'),
  scrapeProgress: () => invoke<ProgressReply>('scrape_progress'),
  scrapeDrain: (limit = 50) => invoke<ScrapeUpdate[]>('scrape_drain', { limit }),
  scrapeClose: () => invoke<void>('scrape_close'),
  loginStart: (platform = 'boss') => invoke<number>('login_start', { platform }),
  /**
   * 只把 sidecar 的浏览器拉起来，不抓岗位、不导航。
   *
   * 聊天与「求职期望」都依赖活着的 sidecar，但它们不该强迫用户先跑一次抓取。
   * `page` 指定落地页（如 `'chat'`）——不传时浏览器停在 about:blank。
   */
  sessionOpen: (platform = 'boss', page?: string) =>
    invoke<number>('session_open', page ? { platform, page } : { platform }),

  // ---- 配置与决策记录 ----
  getAppConfig: () => invoke<AppConfig>('get_app_config'),
  setAppConfig: (patch: AppConfigPatch) => invoke<void>('set_app_config', { patch }),
  listDecisions: (limit = 200, afterId?: number) => invoke<DecisionLog[]>('list_decisions', { limit, afterId }),
  listSensitive: (limit = 200, offset = 0) => invoke<SensitiveRow[]>('list_sensitive', { limit, offset }),
  deleteSensitive: (ids: number[]) => invoke<number>('delete_sensitive', { ids }),
  clearSensitive: () => invoke<number>('clear_sensitive'),
  setDelivered: (jobIds: string[]) => invoke<number>('set_delivered', { jobIds }),
  setRejected: (jobIds: string[]) => invoke<number>('set_rejected', { jobIds }),

  // ---- Boss 聊天 ----
  chatFriends: () => invoke<FriendsReply>('chat_friends'),
  chatMessages: (uid: string) => invoke<MessagesReply>('chat_messages', { uid }),
  chatSend: (uid: string, text: string) => invoke<SendReply>('chat_send', { uid, text }),
  chatResumes: () => invoke<ResumesReply>('chat_resumes'),
  chatSendResume: (uid: string, resumeId: string) => invoke<OkReply>('chat_send_resume', { uid, resumeId }),
  chatRecall: (uid: string) => invoke<OkReply>('chat_recall', { uid }),

  // ---- 打招呼 ----
  greetDraft: (args: GreetArgs) => invoke<GreetReply>('greet_draft', { args }),
  greetOne: (uid: string, text: string) => invoke<string>('greet_one', { uid, text }),

  // ---- 账号里配置的求职期望（需浏览器已在运行且已登录） ----
  listExpectations: () => invoke<Expectation[]>('list_expectations'),
};

/** Rust 侧命令错误一律是字符串；非字符串（如 IPC 层异常）也统一成可读文本。 */
export function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** 尚未注册到 `invoke_handler` 的命令，Tauri 会回这种文案；比它更像业务报错的都不替换。 */
const MISSING_CMD_RE = /not found|not allowed|cannot find|undefined|unknown command|is not a function/i;

/**
 * 命令缺失判定。骨架期（另有 lane 在后端落地这些命令）用它让界面**优雅降级**：
 * 只有「不像业务错误」的失败才被判为未就绪，真实业务报错照常冒泡给错误条。
 */
export function isCommandMissing(e: unknown): boolean {
  return MISSING_CMD_RE.test(errText(e));
}

/** 界面统一的「未就绪」文案，各处共用一套措辞，别各写一句。 */
export const NOT_READY_TEXT = '该功能后端尚未就绪';

/** 排序字段的中文名，列表表头与大模型提示共用一套措辞。 */
export const SORT_LABEL: Record<SortField, string> = {
  score: '分数',
  created: '入库时间',
  updated: '更新时间',
};

/** 脱敏展示用：后 4 位以外一律打码；空值给占位。 */
export function keyHint(hint: string | null): string {
  if (!hint) return '未配置';
  const tail = hint.slice(-4);
  return tail ? `****${tail}` : '已配置';
}

/**
 * 抓取进度的初始值。
 *
 * 是**值**而不是类型，所以走 `export ... from` 而非 `export type`。
 * 放在这里而不是让调用方各自写一份字面量：`ProgressReply` 加字段时
 * 只改 `types.ts` 一处，所有初始值自动跟上。
 */
export { emptyProgress } from './types';
