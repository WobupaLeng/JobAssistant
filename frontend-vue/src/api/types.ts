/**
 * 契约类型：与 `backend-tauri/src/commands_*.rs` 的结构体逐一对应。
 *
 * 这里的每个字段名都必须与 Rust 侧 serde 的输出**逐字一致（蛇形）**。
 * 只有 **`invoke` 的入参**用驼峰（AGENTS.md C3），DTO 不做任何大小写转换——
 * 后端 `#[serde(rename_all = "snake_case")]` 的 struct 甚至连入参都可能只认蛇形，
 * 转换层多一步就多一处静默失效。
 *
 * 与命令层对齐的修正（以 Rust 侧命令层为准，见 ARCH-003 §5.3）：
 * - `LlmStatus.key_hint` / `model` 是 `Option<String>` → `string | null`；
 * - `ScrapeProgress.last_log` / `last_title` / `finished_reason` 同为可空；
 * - `ScrapeUpdate` 的 `job.score` 是 `Option<u8>` → `number | null`；
 * - `AppConfigReply` **没有** `api_key` 字段（后端只回 `api_key_hint` 掩码）。
 */

import type { Job } from '../lib_ui/job';

export type { Job } from '../lib_ui/job';

/** `app_info()` 的返回。 */
export interface AppInfo {
  name: string;
  version: string;
  shell: string;
  stage: string;
  /** 数据目录**绝对路径**（配置页「数据目录」一栏直接显示它）。 */
  data_dir: string;
}

/**
 * `get_scoring_spec()` 的返回。
 *
 * **前端禁止硬编码满分与门槛**（AGENTS.md C10）：该常量历史上散落 5 处，
 * 漏改任一处会产生「门槛 60 分却筛出 6 分岗位」的静默错误。
 * 真实值必须来自本命令；`0 / 0` 只是拿到 spec 之前的占位。
 */
export interface ScoringSpec {
  score_max: number;
  score_min_default: number;
}

/**
 * `list_jobs` 的参数。
 *
 * 页签 `tab` 一给，就覆盖 `status` + `include_rejected` + `rejected_only`：
 * 三个页签的语义互斥，拼在一起会得到「未投递里混着否决」这种看不懂的列表。
 *
 * 注意：命令层是 `list_jobs(state, args: ListJobsArgs)`，载荷要包一层 `args`；
 * 且**为 `undefined` 的字段必须被丢掉**（命令层 `Option<T>` 没有 `#[serde(default)]`，
 * 显式 `null` 会被判成缺 key，见 ARCH-003 §5.2），由 `api/invoke.ts` 的 `prune` 负责。
 */
export interface ListJobsArgs {
  /** 状态：`pending` / `delivered` / `rejected`（旧版 `listTab`） */
  status?: string | null;
  platform?: string | null;
  min_score?: number | null;
  include_rejected?: boolean;
  limit?: number | null;
  offset?: number | null;
  /** `score` | `time` | `salary` | `created` | `updated` */
  sort?: string | null;
  ascending?: boolean;
  /** 页签：`pending` / `delivered` / `rejected` */
  tab?: string | null;
  /** 只看被否决的岗位；`tab = rejected` 时自动等价 */
  rejected_only?: boolean;
  /** 排序方向 `asc` / `desc`；给了它就覆盖 `ascending` */
  order?: string | null;
}

/** `db_overview()` 的返回。`by_scale` 是 `(规模名, 条数)` 二元组数组。 */
export interface DbOverview {
  total: number;
  by_scale: Array<[string, number]>;
  has_legacy: boolean;
}

/** `llm_status()` 的返回。未配置时 `key_hint` / `model` 为 `null`。 */
export interface LlmStatus {
  configured: boolean;
  key_hint: string | null;
  model: string | null;
  prefilter_default: number;
}

/**
 * `score_job` / `passes_threshold` 的入参。
 *
 * 命令层 `ScoreArgs` 带 `#[serde(rename_all = "snake_case")]`，所以**键名一律蛇形**；
 * `useLlm` / `llmPrefilter` 两个驼峰写法由 `alias` 兜住，但不必依赖它。
 */
export interface ScoreArgs {
  company: string;
  title: string;
  salary: string;
  experience: string;
  jd: string;
  use_llm?: boolean | null;
  llm_prefilter?: number | null;
  filters?: string | null;
}

/** `score_job()` 的返回。 */
export interface ScoreReply {
  score: number;
  reason: string;
  rejected_no_jd: boolean;
  source: string;
  tokens: number;
}

/** `scrape_progress()` 的返回（前端轮询）。 */
export interface ScrapeProgress {
  scraped: number;
  inserted: number;
  duplicates: number;
  below_threshold: number;
  rejected: number;
  login_state: string;
  last_log: string | null;
  last_title: string | null;
  running: boolean;
  finished_reason: string | null;
}

/**
 * 抓取事件流的一帧（`scrape_drain` 的单条增量）。
 *
 * Rust 侧是 `#[serde(tag = "kind", rename_all = "snake_case")]` 的判别联合，
 * 判别值即 `kind` 字段的小写形式。`job.score` 是 `Option<u8>` →
 * 未打分 / 被否决的岗位给的是 `null`，不是 0。
 */
export type ScrapeUpdate =
  | { kind: 'job'; summary: string; accepted: boolean; score: number | null }
  | { kind: 'log'; level: string; msg: string }
  | { kind: 'login'; state: string; msg: string }
  | { kind: 'risk'; platform: string; reason: string; url: string }
  | { kind: 'malformed'; line: string }
  | { kind: 'exited'; code: number }
  | { kind: 'restarted'; pid: number };

/** `scrape_start()` 的返回。 */
export interface ScrapeReply {
  pid: number;
  min_score: number;
  keywords: string[];
  max_jd: number | null;
}

/** Boss 聊天好友列表的一行。 */
export interface Friend {
  uid: string;
  name: string;
  company: string;
  title: string;
  last_msg: string;
  last_time: string;
  unread: number;
  avatar: string;
}

export interface ChatMessage {
  sender: string;
  content: string;
  time: string;
  msg_id: string;
  recalled: boolean;
}

export interface ResumeItem {
  resume_id: string;
  name: string;
  updated_at: string;
}

export interface FriendsReply {
  items: Friend[];
}

export interface MessagesReply {
  items: ChatMessage[];
}

export interface ResumesReply {
  items: ResumeItem[];
}

/** `greet_draft()` 的返回。 */
export interface GreetReply {
  text: string;
  tokens: number;
  source: string;
  note: string;
  /** 本次用掉的库内岗位 id；空表示只按标题/公司写。 */
  job_id?: string;
  /** 提示词里是否真的带上了 JD 正文（聊天页据此提示「这句是拿整段 JD 写的」）。 */
  jd_used?: boolean;
  /** 是否用了 HR 的回复（「生成回复」路径）。 */
  reply_used?: boolean;
}

/** 期望职位（`list_expectations`）的一行。 */
export interface Expectation {
  name: string;
  code: string;
  city: string;
  city_code: string;
  salary: string;
  source: string;
}

/** 敏感库一行，对齐 `core::domain::store_ports::SensitiveRecord`。 */
export interface SensitiveRecord {
  id: number;
  /** 落库时间，Unix 秒 */
  created_at: number;
  platform: string;
  job_id: string | null;
  company: string | null;
  title: string | null;
  salary: string | null;
  location: string | null;
  reason: string | null;
  job_url: string | null;
  /** `stored`（已入库）/ `rejected`（已排除） */
  kind: string;
}

/**
 * 敏感库一页。`total` 是**按当前筛选条件**的总数，不是全库总数——
 * 抽屉标题「敏感库 N 条」与「已选中 M 条」用的就是它。
 */
export interface SensitivePage {
  items: SensitiveRecord[];
  total: number;
}

/**
 * 抓取筛选面板的默认条件入参（`set_scrape_defaults` 的 `args`，14 项）。
 *
 * **只提交变化过的字段**：缺席的键在 Rust 侧是 `None`（= 不动），
 * 不会被空值覆盖。键名一律驼峰，对齐 `commands_config.rs` 的 `alias`。
 * 回读走 `get_app_config().scrape_defaults`。
 */
export interface ScrapeDefaultsArgs {
  city?: string;
  salary?: unknown;
  experience?: unknown;
  degree?: unknown;
  companyScale?: unknown;
  financeStage?: unknown;
  industry?: unknown;
  jobType?: unknown;
  maxPages?: unknown;
  excludeKeywords?: unknown;
  hrActive?: unknown;
  /** 休息制度三档＋不限；字面量逐字同源 SPEC-004（后端 rules.rs 枚举），错一个字母＝筛选静默丢失 */
  workWeekend?: 'any' | 'double' | 'no_bigweek' | 'single';
  jdDegree?: unknown;
  minYears?: unknown;
}

/**
 * `get_app_config()` 的返回，对齐 `commands_config::AppConfigReply`。
 *
 * **没有 `api_key` 字段**：密钥只落库、只以 `api_key_hint`（`****尾4位`）回传。
 * 保存时若用户没动密钥框，patch 里就不要带 `api_key`；带了掩码也会被后端
 * 识别并跳过（见 `commands_config::apply_patch`）。
 */
export interface AppConfigReply {
  base_url: string;
  /** `****abcd`；未配置时为空串 */
  api_key_hint: string;
  has_key: boolean;
  model: string;
  /** 打分温度（0~2，缺省 0.2）：低 = 同一条 JD 每次分数稳定 */
  temperature: number;
  /** 话术温度（0~2，缺省 0.8）：高 = 招呼语更有变化 */
  greet_temperature: number;
  score_mode: string;
  theme: string;
  llm_prefilter: number;
  /** 有效 JD 上限（抓取时最多补多少个岗位的详情） */
  max_jd: number;
  /** 打分门槛（不可硬编码，随 `score_max` 一起来自后端） */
  min_score: number;
  score_max: number;
  greet_prompt: string;
  /** 出厂提示词，供「恢复默认」按钮使用 */
  greet_prompt_default: string;
  skills: string;
  exclude_keywords: string;
  exclude_rules: string;
  resume_path: string;
  data_dir: string;
  last_filter: string;
  tokens_total: number;
  tokens_greet: number;
  tokens_score: number;
  /**
   * 抓取筛选面板上次用过的条件（`scrape_defaults.*` 聚合，14 项）。
   * 一个都没配过时是 `{}`，**不是 `null`**；缺省字段时可能是 `null`，
   * 所以读取方一律走「当对象展开，非对象就当空」，不要直接点属性。
   */
  scrape_defaults: Record<string, unknown> | null;
}

/** `set_app_config()` 的返回。 */
export interface SaveConfigReply {
  ok: boolean;
  /** 保存后大模型客户端是否可用 */
  llm_enabled: boolean;
  /** 实际写入的键数（提示「已保存（N 项）」） */
  updated: number;
}

/** `set_scrape_defaults()` 的返回。`saved` = 实际写入的键数（`null` 字段不计）。 */
export interface ScrapeDefaultsReply {
  ok: boolean;
  saved: number;
}

/** `set_limit()` 的返回：**后端裁决后的值**，不是前端输入的值。 */
export interface LimitReply {
  ok: boolean;
  max_jd: number;
  min_score: number;
}

/**
 * `list_decisions()` 的一行，对齐 `core::domain::store_ports::DecisionRecord`。
 *
 * `old_score` / `new_score` 是后加的补列：更早的行两列为 `null`，
 * 界面按「无分数变化」渲染（不要显示 `0 → 0`）。
 */
export interface DecisionRecord {
  id: number;
  platform: string;
  /** `公司/岗位`，按最后一个斜杠切（见 `lib_ui/logfmt.ts` 的 `split_company_job`） */
  company_job: string;
  decision: string;
  reason: string;
  /** Unix 秒 */
  created_at: number;
  old_score: number | null;
  new_score: number | null;
}

/** 一条运行日志（`list_logs`）。 */
export interface LogRow {
  /** Unix 毫秒 */
  ts: number;
  /** 后端分配的单调序号（从 1 开始）。同一毫秒多条时靠它区分列表 key。 */
  seq: number;
  level: string;
  message: string;
}

/**
 * `list_logs(since)` 的返回：条目 + **本次游标**。
 *
 * 调用方必须把 `now` 存下来，下次带 `since = now`；
 * 否则每次都会把整段历史重新拉一遍。后端按 `>=` 比较（闭区间），
 * 游标那一毫秒的条目会重送一次，由界面多重集去重消化。
 */
export interface LogsReply {
  items: LogRow[];
  now: number;
}

/** 三个页签的条数（`job_tab_counts`）。 */
export interface JobTabCounts {
  pending: number;
  delivered: number;
  rejected: number;
}

/** 登录态（`login_status`）。 */
export interface LoginStatus {
  platform: string;
  /** `logged_in` / `no_login` / `unknown` */
  state: string;
  /** sidecar 是否在跑 */
  running: boolean;
  message: string;
  /** 本次会话是否已把登录态存过 */
  saved: boolean;
}

/** `login_save()` 的返回。 */
export interface LoginSaveReply {
  ok: boolean;
  saved: boolean;
  error: string;
}

/** `logout()` 的返回。 */
export interface LogoutReply {
  ok: boolean;
  /** 是否清掉了登录标记 */
  cleared: boolean;
  /** 是否顺带关掉了 sidecar 浏览器会话 */
  closed: boolean;
}

/** `chat_send()` 的返回。 */
export interface SendReply {
  /** 实际发出去的文本（过长时后端已截断） */
  text: string;
  /** 顺带标记为已投递的岗位 id；没传 `jobId` 或标记失败时为空串 */
  job_id: string;
}

/** 无载荷命令的通用回执（`chat_send_resume` / `chat_recall` / 敏感库删除等）。 */
export interface OkReply {
  ok: boolean;
}

/** `sensitive_remove()` 的返回。`deleted` = 实际删除条数。 */
export interface RemoveReply {
  ok: boolean;
  deleted: number;
}

/**
 * 自动投递：一次 `auto_greet_next` 的结果。
 *
 * 后端是**无定时器的状态机**：每调一次最多发一条，回执里带 `next_at_ms`，
 * 前端据此安排下一次调用（节奏与随机间隔属业务规则，在前端**不得**自己算）。
 */
export interface AutoNextReply {
  /** 本次是否真的发出去了 */
  ok: boolean;
  /** `pick` / `greet` / `send` / `mark` / `done`——区分「没岗位了」与「发失败」 */
  stage: string;
  error: string;
  skip: boolean;
  job_id: string;
  company: string;
  title: string;
  /** 发出去的正文 */
  text: string;
  /** `rule` / `llm` */
  source: string;
  note: string;
  tokens: number;
  /** 下一次该在什么时刻（Unix 毫秒）再调；0 表示不必再调 */
  next_at_ms: number;
  /** 距离下次调用的剩余毫秒（可直接做倒计时） */
  remaining_ms: number;
  sent: number;
}

/** 自动投递运行状态（`auto_greet_status`）。 */
export interface AutoStatusReply {
  running: boolean;
  paused: boolean;
  sent: number;
  failures: number;
  next_at_ms: number;
  remaining_ms: number;
  /** 停止原因（「待投递列表已经处理完了」/「连续失败 3 次，已自动暂停」…） */
  stop_reason: string;
  /** 待投递队列里还剩多少条 */
  pending: number;
}

/**
 * `rescore_jobs` 的参数。
 *
 * 分页由调用方驱动：后端一次只处理 `limit` 条并回 `next_cursor`，
 * 调用方必须循环到 `has_more === false`。`after_job_id` 为 `null` 表示从头开始，
 * 且这个 `null` 必须**真的发出去**（不 prune），否则会与「缺 key」混淆。
 */
export interface RescoreArgs {
  after_job_id?: string | null;
  limit?: number | null;
  use_llm?: boolean;
  llm_prefilter?: number | null;
  filters?: string | null;
  log_changes?: boolean;
}

export interface RescoreReply {
  scanned: number;
  updated: number;
  failed: number;
  next_cursor: string | null;
  has_more: boolean;
  changes: RescoreChange[];
}

export interface RescoreChange {
  job_id: string;
  title: string | null;
  company: string | null;
  old_score: number | null;
  new_score: number;
  source: string;
  reason: string;
}

/** 空 `Job`：列表占位用（避免各处各写一份 28 字段字面量）。 */
export type { Job as JobDto };
