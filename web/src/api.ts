// 统一的后端访问层。
// Tauri 环境下端口由 Rust 侧通过 invoke 提供；浏览器调试时回退到固定端口。

import { invoke } from '@tauri-apps/api/core'

let cachedPort: number | null = null

export async function resolvePort(): Promise<number> {
  if (cachedPort) return cachedPort
  // 1) Tauri 注入
  try {
    const p = await invoke<number>('get_backend_port')
    if (p && p > 0) {
      cachedPort = p
      return p
    }
  } catch {
    /* 非 Tauri 环境 */
  }
  // 2) 环境变量（vite dev 用，便于浏览器里调试）
  const env = (import.meta as any).env?.VITE_AJA_PORT
  if (env) {
    cachedPort = Number(env)
    return cachedPort
  }
  // 3) 兜底
  cachedPort = 8791
  return cachedPort
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const port = await resolvePort()
  const res = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    let detail = ''
    try {
      const body = await res.json()
      detail = body?.detail || JSON.stringify(body)
    } catch {
      detail = await res.text().catch(() => '')
    }
    throw new Error(`${res.status} ${detail || res.statusText}`)
  }
  return (await res.json()) as T
}

// ---------- 类型 ----------
export interface JobRow {
  job_id: string
  platform: string
  title: string | null
  company: string | null
  salary: string | null
  status: string
  created_at: number
  job_url: string | null
  jd_text: string | null
  location: string | null
  experience: string | null
  degree: string | null
  hr_name: string | null
  hr_position: string | null
  hr_active_status: string | null
  company_industry: string | null
  company_scale: string | null
  company_stage: string | null
  company_address: string | null
  company_intro: string | null
  hiring_count: number | null
  founded_year?: number | null
  reg_capital?: string | null
  hr_active_time?: number | null
  token_used?: number | null
  founded_years: number | null
  match_score: number | null
  rejected?: number | null
  score_source: string | null
  score_reason: string | null
}

export interface LogRow {
  ts: number
  level: string
  message: string
}

export interface Stats {
  total: number
  scored: number
  avg_score: number | null
  by_status: Record<string, number>
  by_platform: Record<string, number>
}

export interface StatusResp {
  running: boolean
  platforms: string[]
  progress: { collected: number; scored: number; filtered: number; total: number; tokens?: number }
  stats: Stats
  logged_in?: Record<string, boolean>
  login_sessions?: Record<string, LoginSession>
  max_jd?: number
  progress_text?: string
  paused?: boolean
}

export interface LoginSession {
  platform: string
  state: 'idle' | 'opening' | 'waiting' | 'logged_in' | 'closed' | 'error'
  running: boolean
  message: string
  saved: boolean
}

export interface Config {
  base_url: string
  model: string
  api_key_masked: string
  has_key: boolean
  resume_path: string
  theme: string
  exclude_keywords: string[]
  exclude_rules?: ExcludeRules
  max_jd?: number
  exclude_rule_defaults?: ExcludeRules
  greet_prompt?: string
  greet_prompt_default?: string
  tokens_total?: number
  tokens_greet?: number
  profile: Record<string, any>
}

/** 排除规则：值为 null / 空数组 = 该项不排除 */
export const WEEKEND_OPTIONS = [
  { value: 'any', name: '不限' },
  { value: 'double', name: '只投双休' },
  { value: 'no_bigweek', name: '大小周' },
] as const

export interface ExcludeRules {
  name_words: string[]
  exclude_salary_min: number | null
  exclude_salary_max: number | null
  exclude_degree: string[]
  exclude_founded_below: number | null
  exclude_resume_max_years: number | null
  exclude_hr_active_days?: number | null
  weekend?: string | null
}

// HR 活跃度门槛：最近 N 天内活跃才算合格
export const HR_ACTIVE_OPTIONS = [
  { days: 1, name: '今日活跃' },
  { days: 3, name: '3 天内' },
  { days: 7, name: '一周内' },
] as const

export const DEGREE_VETO_OPTIONS = ['大专', '本科', '硕士', '博士'] as const

export interface StartPayload {
  platforms: string[]
  keywords: string[]
  city: string
  experience: string[]
  salary: string[]
  degree: string[]
  scale: string[]
  stage: string[]
  industry: string[]
  job_type: string
  exclude_keywords: string[]
  max_pages: number
  daily_limit: number
  max_jd?: number
}

// ---------- Boss 消息 ----------
export interface ChatFriend {
  uid: string
  security_id?: string
  encrypt_boss_id?: string
  encrypt_uid?: string
  name: string
  title?: string
  brand?: string
  avatar?: string
  last_msg?: string
  last_time?: string
  last_ts?: number
  unread?: number
  encrypt_job_id?: string
  job_id?: string
  from_id?: number
  to_id?: number
}

export interface ChatMessage {
  id: string
  mid: string
  from_me: boolean
  text: string
  time: number
  time_text: string
  kind?: string
  system?: boolean
  /** 自己发的才有：read=已读 delivered=送达 unread=未读 */
  status?: string
}

// ---------- API ----------
export interface ResumeItem {
  id: string
  name: string
  size: string
  upload_time: string
  suffix: string
}

export interface GreetResp {
  ok: boolean
  text: string
  tokens: number
  source: 'llm' | 'rule'
  note?: string
  tokens_total: number
  tokens_greet: number
}

export const apiClient = {
  status: () => api<StatusResp>('/api/status'),
  start: (p: StartPayload) =>
    api<{ ok: boolean; message: string }>('/api/start', {
      method: 'POST',
      body: JSON.stringify(p),
    }),
  stop: () => api<{ ok: boolean }>('/api/stop', { method: 'POST' }),
  expectations: (platform = 'boss') =>
    api<{ ok: boolean; status?: string; items: { name: string; code?: string; city?: string }[]; error?: string }>(
      `/api/expectations?platform=${encodeURIComponent(platform)}`
    ),
  sensitive: (params: { platform?: string; keyword?: string; limit?: number; offset?: number; kind?: string } = {}) => {
    const q = new URLSearchParams()
    if (params.platform) q.set('platform', params.platform)
    if (params.keyword) q.set('keyword', params.keyword)
    if (params.kind) q.set('kind', params.kind)
    q.set('limit', String(params.limit ?? 100))
    q.set('offset', String(params.offset ?? 0))
    return api<{ items: SensitiveRow[]; total: number }>(`/api/sensitive?${q.toString()}`)
  },
  deleteSensitive: (ids: number[]) =>
    api<{ ok: boolean; deleted: number }>('/api/sensitive/delete', {
      method: 'POST',
      body: JSON.stringify({ ids }),
    }),
  pause: () => api<{ ok: boolean; paused: boolean }>('/api/pause', { method: 'POST' }),
  resume: () => api<{ ok: boolean; paused: boolean }>('/api/resume', { method: 'POST' }),
  jobs: (params: {
    status?: string
    platform?: string
    min_score?: number
    limit?: number
    offset?: number
    sort?: 'score' | 'time' | 'salary'
    order?: 'asc' | 'desc'
    include_rejected?: boolean
    rejected_only?: boolean
  } = {}) => {
    const q = new URLSearchParams()
    if (params.status) q.set('status', params.status)
    if (params.platform) q.set('platform', params.platform)
    if (params.min_score != null) q.set('min_score', String(params.min_score))
    q.set('limit', String(params.limit ?? 10))
    q.set('offset', String(params.offset ?? 0))
    q.set('sort', params.sort ?? 'score')
    q.set('order', params.order ?? 'desc')
    if (params.include_rejected === false) q.set('include_rejected', 'false')
    if (params.rejected_only) q.set('rejected_only', 'true')
    return api<{ items: JobRow[]; total: number; offset: number; limit: number }>(`/api/jobs?${q}`)
  },
  rejectJobs: (job_ids: string[]) =>
    api<{ ok: boolean; updated: number }>('/api/jobs/reject', {
      method: 'POST',
      body: JSON.stringify({ job_ids }),
    }),
  markDelivered: (job_ids: string[]) =>
    api<{ ok: boolean; updated: number }>('/api/jobs/deliver', {
      method: 'POST',
      body: JSON.stringify({ job_ids }),
    }),
  allowJobs: (job_ids: string[]) =>
    api<{ ok: boolean; updated: number }>('/api/jobs/allow', {
      method: 'POST',
      body: JSON.stringify({ job_ids }),
    }),
  logs: (since = 0) => api<{ items: LogRow[]; now: number }>(`/api/logs?since=${since}`),
  getConfig: () => api<Config>('/api/config'),
  setLimit: (max_jd: number, min_score?: number) =>
    api<{ ok: boolean; max_jd: number; min_score: number }>('/api/limit', {
      method: 'POST',
      body: JSON.stringify(min_score === undefined ? { max_jd } : { max_jd, min_score }),
    }),
  setConfig: (patch: Record<string, any>) =>
    api<{ ok: boolean; llm_enabled: boolean }>('/api/config', {
      method: 'POST',
      body: JSON.stringify(patch),
    }),
  rescore: () => api<{ ok: boolean; rescored: number }>('/api/rescore', { method: 'POST' }),

  // ---- 平台登录 ----
  loginStart: (platform: string) =>
    api<{ ok: boolean; message: string; session: LoginSession }>('/api/login/start', {
      method: 'POST',
      body: JSON.stringify({ platform }),
    }),
  loginStatus: (platform: string) => api<LoginSession>(`/api/login/status?platform=${platform}`),
  loginSave: (platform: string) =>
    api<{ ok: boolean; saved?: number; error?: string }>(
      `/api/login/save?platform=${platform}`,
      { method: 'POST' },
    ),
  logout: (platform: string) =>
    api<{ ok: boolean; cleared: boolean }>('/api/login/logout', {
      method: 'POST',
      body: JSON.stringify({ platform }),
    }),

  // ---- Boss 消息 ----
  chatFriends: (force = false) =>
    api<{ ok: boolean; items: ChatFriend[]; error?: string }>(
      `/api/chat/friends?force=${force ? 'true' : 'false'}`,
    ),
  chatMessages: (uid: string, force = false) =>
    api<{ ok: boolean; items: ChatMessage[]; error?: string }>(
      `/api/chat/messages?uid=${encodeURIComponent(uid)}&force=${force ? 'true' : 'false'}`,
    ),
  chatSend: (uid: string, text: string, jobId?: string | null) =>
    api<{ ok: boolean; uid?: string; text?: string; error?: string }>('/api/chat/send', {
      method: 'POST',
      body: JSON.stringify({ uid, text, job_id: jobId ?? null }),
    }),
  chatResumes: (force = false) =>
    api<{ ok: boolean; items: ResumeItem[]; error?: string }>(`/api/chat/resumes?force=${force}`),
  chatSendResume: (uid: string, resumeId?: string) =>
    api<{ ok: boolean; error?: string }>('/api/chat/resume', {
      method: 'POST',
      body: JSON.stringify({ uid, resume_id: resumeId ?? null }),
    }),
  greet: (body: { job_id?: string; uid?: string; encrypt_job_id?: string }) =>
    api<GreetResp>('/api/greet', { method: 'POST', body: JSON.stringify(body) }),
  chatRecall: (uid: string) =>
    api<{ ok: boolean; error?: string }>('/api/chat/recall', {
      method: 'POST',
      body: JSON.stringify({ uid }),
    }),
  chatOpen: (p: { uid?: string; encrypt_job_id?: string; brand?: string }) =>
    api<{ ok: boolean; found?: boolean; friend?: ChatFriend; error?: string }>('/api/chat/open', {
      method: 'POST',
      body: JSON.stringify(p),
    }),
}

// ready: 是否已接入（未接入的平台在界面上置灰且不可勾选）
export const PLATFORMS = [
  { id: 'boss', name: 'Boss直聘', ready: true },
  { id: 'zhilian', name: '智联招聘', ready: false },
  { id: '51job', name: '51job', ready: false },
] as const

// ===== 与 Boss 直聘官网筛选项完全一致的编码表（值是 Boss 自己的 code） =====

export const CITIES = [
  { code: '101280600', name: '深圳' },
  { code: '101020100', name: '上海' },
  { code: '101010100', name: '北京' },
  { code: '101280100', name: '广州' },
  { code: '101210100', name: '杭州' },
  { code: '101270100', name: '成都' },
  { code: '101190400', name: '苏州' },
  { code: '101030100', name: '天津' },
] as const

// 与 Boss 直聘「薪资待遇」下拉一致：8K以下 / 8-12K / 12-16K / 16-20K / 20-30K / 30K以上，外加自定义档位
export const SALARY_OPTIONS = [
  { code: 'sal:0-8', name: '8K以下' },
  { code: 'sal:8-12', name: '8-12K' },
  { code: 'sal:12-16', name: '12-16K' },
  { code: 'sal:16-20', name: '16-20K' },
  { code: 'sal:20-30', name: '20-30K' },
  { code: 'sal:30-9999', name: '30K以上' },
] as const

export const EXPERIENCE_OPTIONS = [
  { code: '101', name: '在校/应届' },
  { code: '102', name: '经验不限' },
  { code: '103', name: '1年以内' },
  { code: '104', name: '1-3年' },
  { code: '105', name: '3-5年' },
  { code: '106', name: '5-10年' },
  { code: '107', name: '10年以上' },
] as const

export const DEGREE_OPTIONS = [
  { code: '202', name: '大专' },
  { code: '203', name: '本科' },
  { code: '204', name: '硕士' },
  { code: '205', name: '博士' },
  { code: '206', name: '中专/中技' },
  { code: '208', name: '高中' },
  { code: '209', name: '初中及以下' },
] as const

export const SCALE_OPTIONS = [
  { code: '301', name: '0-20人' },
  { code: '302', name: '20-99人' },
  { code: '303', name: '100-499人' },
  { code: '304', name: '500-999人' },
  { code: '305', name: '1000-9999人' },
  { code: '306', name: '10000人以上' },
] as const

export const STAGE_OPTIONS = [
  { code: '801', name: '未融资' },
  { code: '802', name: '天使轮' },
  { code: '803', name: 'A轮' },
  { code: '804', name: 'B轮' },
  { code: '805', name: 'C轮' },
  { code: '806', name: 'D轮及以上' },
  { code: '807', name: '已上市' },
  { code: '808', name: '不需要融资' },
] as const

export const JOB_TYPE_OPTIONS = [
  { code: '1901', name: '全职' },
  { code: '1902', name: '兼职' },
  { code: '1903', name: '实习' },
] as const

/** Boss 直聘官网「行业」筛选项（URL 参数 industry=编码），常用行业。 */
export const INDUSTRY_OPTIONS = [
  { code: '100020', name: '互联网' },
  { code: '100021', name: '电子商务' },
  { code: '100022', name: '计算机软件' },
  { code: '100023', name: '计算机硬件' },
  { code: '100024', name: '通信/网络设备' },
  { code: '100025', name: '电子/半导体/集成电路' },
  { code: '100026', name: '人工智能' },
  { code: '100027', name: '云计算/大数据' },
  { code: '100028', name: '企业服务' },
  { code: '100029', name: '金融/银行/证券' },
  { code: '100030', name: '游戏' },
  { code: '100031', name: '医疗健康' },
  { code: '100032', name: '教育培训' },
  { code: '100033', name: '汽车/新能源' },
  { code: '100034', name: '智能制造/工业自动化' },
  { code: '100035', name: '物流/仓储' },
  { code: '100036', name: '房地产/建筑' },
  { code: '100037', name: '消费零售' },
] as const


export interface SensitiveRow {
  id: number
  platform: string
  job_id?: string | null
  company?: string | null
  title?: string | null
  salary?: string | null
  location?: string | null
  reason?: string | null
  job_url?: string | null
  created_at: number
  kind?: string | null   // rejected=被排除 / stored=已入库
}
