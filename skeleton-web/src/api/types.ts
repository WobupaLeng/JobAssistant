/**
 * Tauri 命令的**类型契约**（只有类型，没有任何运行时代码）。
 *
 * **真源是 Rust**（AGENTS.md 3.1）：这里的每个字段都对照过
 * `app/src-tauri/src/commands_db.rs`、`app/src-tauri/src/commands_scrape.rs`
 * 与 `core/src/domain/job.rs`，改 Rust 侧签名必须同一次提交里改这里。
 * 本文件只有 `type` 声明：`invoke` 的调用全部在 `./index.ts`，
 * 聊天与求职期望的类型在文件末尾。
 */

// ---------- 环境与口径 ----------

export type AppInfo = { name: string; version: string; shell: string; stage: string };

/** 打分口径，由 `get_scoring_spec` 下发。前端**禁止**硬编码满分与门槛。 */
export type ScoringSpec = { score_max: number; score_min_default: number };

// ---------- 岗位 ----------

/** 与 `core/src/domain/job.rs::Job` 对齐的展示字段。 */
export type Job = {
  job_id: string;
  platform: string;
  title: string | null;
  company: string | null;
  salary: string | null;
  status: string;
  created_at: number;
  job_url: string | null;
  jd_text: string | null;
  location: string | null;
  experience: string | null;
  degree: string | null;
  hr_name: string | null;
  hr_position: string | null;
  hr_active_status: string | null;
  company_industry: string | null;
  company_scale: string | null;
  company_stage: string | null;
  company_address: string | null;
  company_intro: string | null;
  hiring_count: number | null;
  founded_years: number | null;
  match_score: number | null;
  /** `1` 表示被排除规则否决 */
  rejected: number;
  score_source: string | null;
  score_reason: string | null;
  updated_at: number | null;
  score_scale: string | null;
};

export type SortField = 'score' | 'created' | 'updated';

export type ListJobsArgs = {
  status?: string;
  platform?: string;
  min_score?: number;
  include_rejected?: boolean;
  limit?: number;
  offset?: number;
  sort?: SortField;
  ascending?: boolean;
};

export type DbOverview = {
  total: number;
  /** `[["100", 21], ["10", 0]]`：打分制标记 → 条数 */
  by_scale: [string, number][];
  has_legacy: boolean;
};

// ---------- 打分 ----------

export type ScoreArgs = {
  company?: string;
  title?: string;
  salary?: string;
  experience?: string;
  jd?: string;
  useLlm?: boolean;
  llmPrefilter?: number;
  filters?: string;
};

export type ScoreReply = {
  score: number;
  reason: string;
  rejected_no_jd: boolean;
  /** `rule` / `llm` */
  source: string;
  tokens: number;
};

export type LlmStatus = {
  configured: boolean;
  /** 脱敏尾号，如 `****abcd`；绝不返回明文 */
  key_hint: string | null;
  model: string | null;
  prefilter_default: number;
};

// ---------- 全量重算 ----------

export type RescoreChange = {
  job_id: string;
  title: string | null;
  company: string | null;
  old_score: number | null;
  new_score: number;
  source: string;
  reason: string;
};

export type RescoreReply = {
  scanned: number;
  updated: number;
  failed: number;
  next_cursor: string | null;
  has_more: boolean;
  changes: RescoreChange[];
};

export type RescoreArgs = {
  afterJobId?: string;
  limit?: number;
  useLlm?: boolean;
  llmPrefilter?: number;
  filters?: string;
  logChanges?: boolean;
};

// ---------- 抓取 ----------

export type ScrapeReply = {
  pid: number;
  min_score: number;
  keywords: string[];
  max_jd: number | null;
};

export type ProgressReply = {
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
};

/** 增量事件，对应 Rust 侧 `UpdateReply`（按 `kind` 打标签）。 */
export type ScrapeUpdate =
  | { kind: 'job'; summary: string; accepted: boolean; score: number | null }
  | { kind: 'log'; level: string; msg: string }
  | { kind: 'login'; state: string; msg: string }
  | { kind: 'malformed'; line: string }
  | { kind: 'exited'; code: number | null }
  | { kind: 'restarted'; pid: number };

// ---------- 配置（骨架期，后端命令尚未落地） ----------
//
// 字段语义对照旧版 `backend/app/main.py` 的 `GET/POST /api/config`
// 与 `web/src/api.ts` 的 `Config` / `ExcludeRules`。重写版把这些键统一收进
// `app_config` 表，前端只认下面这一份结构，不再区分旧版的嵌套形式。

/** 筛选条件（下单关键词，值是站点编码或原文，留空 = 不限）。 */
export type AppFilters = {
  salary: string;
  experience: string;
  degree: string;
  scale: string;
  stage: string;
  industry: string;
};

/**
 * 排除规则。`null` / 空串 / 空数组一律表示「该项不排除」。
 * 键名对齐旧版 `ExcludeRules`：`veto_words` / `exclude_salary_min` / …，
 * 其中 `weekend` 旧版取 `double` 或 `null`，重写版补了 `no_bigweek`。
 */
export type AppExcludes = {
  veto_words: string[];
  exclude_salary_min: number | null;
  exclude_salary_max: number | null;
  exclude_degree: string[];
  /** 成立年份早于此值的公司排除 */
  exclude_founded_below: number | null;
  /** 简历年限超过此值 = 岗位要求太高，排除 */
  exclude_resume_max_years: number | null;
  /** HR 最近 N 天内没活跃过就排除；`null` = 不按活跃度排除 */
  exclude_hr_active_days: number | null;
  /** `null` = 不限；`double` = 只投双休 */
  weekend: string | null;
};

export type AppConfig = {
  /** 永远为空串：密钥绝不回明文，只回 `api_key_hint` */
  api_key: string;
  /** 脱敏提示，如 `****abcd`；未配置为 `null` */
  api_key_hint: string | null;
  base_url: string;
  model: string;
  /** 每次抓取上限 */
  limit: number;
  keywords: string[];
  min_score: number;
  use_llm: boolean;
  llm_prefilter: number;
  greet_prompt: string;
  skills: string;
  filters: AppFilters;
  excludes: AppExcludes;
};

/** 只发实际改动的键；`api_key` 留空串表示「不改密钥」。 */
export type AppConfigPatch = Partial<Omit<AppConfig, 'api_key_hint' | 'filters' | 'excludes'>> & {
  filters?: Partial<AppFilters>;
  excludes?: Partial<AppExcludes>;
};

// ---------- 决策日志 / 敏感库 ----------

/** 一条判定记录：入库还是排除。`source` 形如 `rule` / `llm` / `sensitive`。 */
export type DecisionLog = {
  id: number;
  created_at: number;
  platform: string;
  job_id: string | null;
  company: string | null;
  title: string | null;
  /** `stored` = 入库，`rejected` = 排除 */
  kind: string;
  reason: string | null;
  score: number | null;
  source: string | null;
};

/** 敏感库条目：抓过的岗位（`stored` / `rejected`）下一轮直接跳过。 */
export type SensitiveRow = {
  id: number;
  created_at: number;
  platform: string;
  job_id: string | null;
  company: string | null;
  title: string | null;
  salary: string | null;
  /** `stored` = 已入库，`rejected` = 已排除 */
  kind: string;
  reason: string | null;
};

/** 脱敏展示用：后 4 位以外一律打码；空值给占位。 */
export function keyHint(hint: string | null): string {
  if (!hint) return '未配置';
  const tail = hint.slice(-4);
  return tail ? `****${tail}` : '已配置';
}

export const emptyProgress: ProgressReply = {
  scraped: 0,
  inserted: 0,
  duplicates: 0,
  below_threshold: 0,
  rejected: 0,
  login_state: 'unknown',
  last_log: null,
  last_title: null,
  running: false,
  finished_reason: null,
};


// ---------- Boss 聊天 ----------
//
// 真源：`core/src/sidecar/protocol.rs` 的 `Friend` / `ChatMessage` / `ResumeItem`
// 与 `app/src-tauri/src/commands_chat.rs` 的六个命令。全字段可缺省——
// 抓不到就留空，不能让一条脏数据显示成整块空白。

export type Friend = {
  uid: string;
  name: string | null;
  company: string | null;
  title: string | null;
  last_msg: string | null;
  last_time: string | null;
  unread: number | null;
  avatar: string | null;
};

export type ChatMessage = {
  /** 只可能是 `me` / `hr` / `system`；决定气泡靠左还是靠右 */
  sender: string;
  content: string;
  time: string | null;
  msg_id: string | null;
  recalled: boolean | null;
};

export type ResumeItem = { resume_id: string; name: string | null; updated_at: string | null };

export type FriendsReply = { items: Friend[] };
export type MessagesReply = { items: ChatMessage[] };
export type ResumesReply = { items: ResumeItem[] };
export type SendReply = { text: string };
export type OkReply = { ok: boolean };

/** 打招呼草稿（`greet_draft`）。`source` 为 `rule` 时 `note` 一定有值。 */
export type GreetReply = { text: string; tokens: number; source: string; note: string };

export type GreetArgs = {
  jobId?: string;
  title?: string;
  company?: string;
  salary?: string;
  experience?: string;
  jd?: string;
  useLlm?: boolean;
};

/**
 * 账号里配置的一条求职期望（`list_expectations`）。
 *
 * 与现行版 `/api/expectations` 的字段一致；`source` 目前恒为 `boss_expect`。
 */
export type Expectation = {
  name: string;
  code: string;
  city: string;
  cityCode: string;
  salary: string;
  source: string;
};
