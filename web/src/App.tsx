import { useCallback, useEffect, useRef, useState } from 'react'
import { apiClient, CITIES, DEGREE_OPTIONS, EXPERIENCE_OPTIONS, JOB_TYPE_OPTIONS, PLATFORMS, SALARY_OPTIONS, SCALE_OPTIONS, STAGE_OPTIONS, WEEKEND_OPTIONS, SensitiveRow, type JobRow, type StatusResp, type LogRow, type Config } from './api'
import { DEGREE_VETO_OPTIONS, HR_ACTIVE_OPTIONS, INDUSTRY_OPTIONS } from './api'
import type { ExcludeRules } from './api'
import ChatPanel from './ChatPanel'

/** 关键字分隔符：空格、中英文逗号、顿号、分号、竖线、斜杠、换行等 */
const SPLIT_KEYWORDS = (text: string): string[] =>
  text
    .split(/[\s,，、;；|/\\]+/)
    .map((s) => s.trim())
    .filter(Boolean)

/** "12" -> 12，" " -> null（留空表示不排除） */
const NUM_OR_NULL = (v: string): number | null => {
  const t = v.trim()
  if (!t) return null
  const n = Number(t)
  return Number.isFinite(n) ? n : null
}

const CARD_SCORE_COLOR = (s: number | null) => {
  if (s == null) return 'bg-slate-300 text-slate-700'
  if (s >= 8) return 'bg-emerald-500 text-white'
  if (s >= 6) return 'bg-emerald-400 text-white'
  if (s >= 4) return 'bg-amber-400 text-amber-900'
  return 'bg-rose-400 text-white'
}

function fmtTime(ts: number) {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

function fmtDate(ts: number) {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

const STATUS_CHIP = (status: string, rejected?: number | null) => {
  if (status === 'delivered')
    return <span className="shrink-0 rounded bg-slate-400 px-1 py-0.5 text-[9px] font-medium text-white">已投递</span>
  if (rejected)
    return <span className="shrink-0 rounded bg-rose-500 px-1 py-0.5 text-[9px] font-medium text-white">不投递</span>
  return (
    <span className="shrink-0 rounded bg-emerald-100 px-1 py-0.5 text-[9px] font-medium text-emerald-700">
      待投递
    </span>
  )
}

function platformName(id: string) {
  return PLATFORMS.find((p) => p.id === id)?.name ?? id
}

const PLATFORM_CHIP_COLOR = (id: string) => {
  if (id === 'boss') return 'bg-teal-100 text-teal-700 dark:bg-teal-900/40 dark:text-teal-300'
  if (id === 'zhilian') return 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-300'
  if (id === '51job') return 'bg-orange-100 text-orange-700 dark:bg-orange-900/40 dark:text-orange-300'
  return 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-300'
}

type SortKey = 'score' | 'time' | 'salary'
const PAGE_SIZE = 100   // 一次加载 100 条，滚动到底再追加 100 条

/** 一组多选 chip（对应 Boss 官网的同一个筛选项维度）。 */
function ChipRow({
  label,
  options,
  value,
  onToggle,
  onClear,
  extra,
}: {
  label: string
  options: readonly { code: string; name: string }[]
  value: string[]
  onToggle: (v: string) => void
  onClear: () => void
  extra?: React.ReactNode
}) {
  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <span className="text-[11px] text-slate-500">
          {label}
          {value.length > 0 && <span className="ml-1 text-primary">已选 {value.length}</span>}
        </span>
        {value.length > 0 && (
          <button className="text-[10px] text-slate-400 hover:text-primary" onClick={onClear}>
            清空
          </button>
        )}
      </div>
      <div className="flex flex-wrap gap-1">
        {options.map((o) => (
          <button
            key={o.code}
            className={`chip ${value.includes(o.code) ? 'chip-on' : ''}`}
            onClick={() => onToggle(o.code)}
          >
            {o.name}
          </button>
        ))}
        {extra}
      </div>
    </div>
  )
}

// 推荐关键词：后端 PROFILE 的方向 + 常见同义岗，点一下就加进关键词
const RECOMMEND_KEYWORDS = ['Java', '后端开发', '全栈工程师', 'Node.js', '服务端开发', 'Java开发工程师']

const fmtTokens = (n: number) => (n >= 1_000_000 ? `${(n / 1_000_000).toFixed(2)}M` : n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n))

const ACTIVE_LEVEL = (s?: string | null): 'high' | 'mid' | 'low' | 'none' => {
  if (!s) return 'none'
  if (/在线|刚刚|今日|今天/.test(s)) return 'high'
  if (/3日|三日|本周|1周|一周/.test(s)) return 'mid'
  if (/2周|1月|一月|本月|月内|\d+天/.test(s)) return 'low'
  return 'none'
}

const PARSED_REASON = (j: JobRow) => {
  const reason = j.score_reason || '-'
  // 规则打分的 reason 形如「技能 20/40 | 薪资 7/20 | 经验 15/15 | 方向 15/15 | 公司质量 10/10」
  const parts = reason.split('|').map((x) => x.trim()).filter(Boolean)
  if (j.score_source === 'llm' || parts.length < 3) return reason
  return (
    <span className="flex flex-wrap gap-1">
      {parts.map((p, i) => {
        const m = p.match(/^(\S+?)\s*(\d+)\/(\d+)$/)
        const ratio = m ? Number(m[2]) / Math.max(1, Number(m[3])) : 1
        const tone =
          ratio >= 0.9 ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300'
            : ratio >= 0.5 ? 'bg-amber-50 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300'
              : 'bg-rose-50 text-rose-700 dark:bg-rose-900/30 dark:text-rose-300'
        return (
          <span key={i} className={`rounded px-1 ${tone}`}>
            {p}
          </span>
        )
      })}
    </span>
  )
}

// 列表里每条右侧的「允许投递 / 不投递」标签：单击即切换（不用开关）
function ReviewSwitch(props: { rejected: boolean; onToggle: (next: boolean) => void }) {
  const { rejected, onToggle } = props
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation()
        onToggle(!rejected)
      }}
      title={rejected ? '当前为「不投递」（绿），单击改为「允许投递」' : '当前为「允许投递」（红），单击改为「不投递」'}
      className={`shrink-0 cursor-pointer whitespace-nowrap rounded-full border px-1.5 py-[1px] text-[10px] font-medium transition hover:brightness-95 active:scale-95
        ${
          rejected
            ? 'border-emerald-400 bg-emerald-50 font-semibold text-emerald-600 dark:border-emerald-600 dark:bg-emerald-950/40 dark:text-emerald-400'
            : 'border-rose-400 bg-rose-50 font-semibold text-rose-600 dark:border-rose-600 dark:bg-rose-950/40 dark:text-rose-400'
        }`}
    >
      {rejected ? '允许投递' : '不投递'}
    </button>
  )
}

export default function App() {
  // ---------- 全局状态 ----------
  const [status, setStatus] = useState<StatusResp | null>(null)
  const [logs, setLogs] = useState<LogRow[]>([])
  const [config, setConfig] = useState<Config | null>(null)
  const [running, setRunning] = useState(false)
  const [paused, setPaused] = useState(false)
  // 左下：日志面板可以在「运行日志 / 敏感库」之间切换
  const [logView, setLogView] = useState<'logs' | 'sensitive'>('logs')
  const [sensitive, setSensitive] = useState<SensitiveRow[]>([])
  const [sensitiveTotal, setSensitiveTotal] = useState(0)
  const [sensitiveKw, setSensitiveKw] = useState('')
  // 敏感库视图：''=全部 / stored=已入库 / rejected=已排除
  const [sensitiveKind, setSensitiveKind] = useState('')
  const [sensitiveLoading, setSensitiveLoading] = useState(false)
  const [showSensitive, setShowSensitive] = useState(false)
  const [sensChecked, setSensChecked] = useState<Set<number>>(new Set())
  const [llmEnabled, setLlmEnabled] = useState(false)
  const [backendOk, setBackendOk] = useState<boolean | null>(null)
  const [err, setErr] = useState<string>('')
  const [dark, setDark] = useState(false)

  // ---------- 顶部：平台与筛选 ----------
  const [filtersOpen, setFiltersOpen] = useState(false)
  const [platforms, setPlatforms] = useState<string[]>(['boss'])
  const [keywords, setKeywords] = useState<string[]>([])
  // Boss 账号里配置的期望职位（点「读取期望职位」后拉取），以及推荐关键词
  const [expects, setExpects] = useState<{ name: string; city?: string }[]>([])
  const [expectBusy, setExpectBusy] = useState(false)
  const [kwInput, setKwInput] = useState('')
  const [city, setCity] = useState('101280600')  // 默认深圳
  const [salary, setSalary] = useState<string[]>([])
  const [experience, setExperience] = useState<string[]>([])
  const [degree, setDegree] = useState<string[]>([])
  const [scale, setScale] = useState<string[]>([])
  const [stage, setStage] = useState<string[]>([])
  const [industry, setIndustry] = useState<string[]>([])
  const [industryInput, setIndustryInput] = useState('')
  const [jobType, setJobType] = useState('1901')
  const DEFAULT_EXCLUDE =
    '外包,驻场,人力,兼职,跨境,金融,银行,证券,区块链,Web3,985,211,硕士,博士,销售'
  const [excludeText, setExcludeText] = useState(DEFAULT_EXCLUDE)
  // 「排除规则」独立填写项：留空 = 不排除
  const [vetoWords, setVetoWords] = useState<string[]>(SPLIT_KEYWORDS(DEFAULT_EXCLUDE))
  const [exclSalaryMin, setExclSalaryMin] = useState<number | null>(null)
  const [exclSalaryMax, setExclSalaryMax] = useState<number | null>(null)
  const [exclDegree, setExclDegree] = useState<string[]>([])
  const [exclFoundedBelow, setExclFoundedBelow] = useState<number | null>(null)
  const [exclMaxYears, setExclMaxYears] = useState<number | null>(null)
  const [exclHrDays, setExclHrDays] = useState<number | null>(null)
  const [weekendRule, setWeekendRule] = useState<string>('any')
  // 列表 / 页签切换动画
  const [listFadeKey, setListFadeKey] = useState(0)
  const seenIds = useRef<Set<string>>(new Set())

  // 切换页签：先淡出，再拉新数据，最后淡入
  const switchTab = (key: 'pending' | 'delivered' | 'rejected') => {
    if (key === listTab) return
    setChecked(new Set())
    setListTab(key)
    setListFadeKey((k) => k + 1)
  }

  // 有效 JD 上限（抓取受它约束，0=不限制）
  const [maxJd, setMaxJd] = useState<number>(20)
  const [maxPages, setMaxPages] = useState(3)

  // 多选开关
  const toggleIn = (list: string[], v: string) =>
    list.includes(v) ? list.filter((x) => x !== v) : [...list, v]

  // ---------- 配置抽屉 ----------
  const [drawer, setDrawer] = useState(false)
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('deepseek-chat')
  // 生成打招呼用的提示词（AI 按它 + 岗位 JD + 我的技能 写话术）
  const [greetPrompt, setGreetPrompt] = useState('')
  const [greetPromptDefault, setGreetPromptDefault] = useState('')
  // 「配置 → 技能」：生成话术时优先用它；留空则退回简历画像里的技能
  const [skills, setSkills] = useState('')
  // 打分方式：默认规则打分（快、不花钱）；切到大模型后抓取时直接用 JD+简历+技能+筛选条件打分
  const [scoreMode, setScoreMode] = useState<'rule' | 'llm'>('rule')
  const scoreModeRef = useRef<'rule' | 'llm'>('rule')
  // 大模型打分前的规则初筛线（规则分低于它的直接判掉，不花 token）
  const [prefilter, setPrefilter] = useState(5)
  const prefilterRef = useRef(5)
  // 累计 token：total=打分+生成，greet=生成打招呼部分
  const [tokens, setTokens] = useState({ total: 0, greet: 0, score: 0 })
  // ---------- 自动打招呼 ----------
  const [autoOn, setAutoOn] = useState(false)
  const [autoPaused, setAutoPaused] = useState(false)
  const [autoTick, setAutoTick] = useState(0)
  const [autoCount, setAutoCount] = useState(0)
  const [autoFail, setAutoFail] = useState(0)
  const [autoMsg, setAutoMsg] = useState('')
  const [autoBusy, setAutoBusy] = useState(false)
  // 「每日投递上限」当前值（改上限不重启循环，下一轮就生效）
  const maxJdRef = useRef(maxJd)
  maxJdRef.current = maxJd
  // 这些用 ref 是为了让「逐条发送」的循环读到最新值，而不是闭包里的旧值
  const autoOnRef = useRef(false)
  const autoPausedRef = useRef(false)
  const autoCountRef = useRef(0)
  const autoFailRef = useRef(0)
  const autoInFlight = useRef(false)
  // 已经处理过（发送成功或被判定跳过）的岗位，避免在同一轮里反复撞同一个
  const autoDoneRef = useRef<Set<string>>(new Set())
  // 每发一条之间等几秒（防封号）：基础 20 秒 ± 5 秒
  const AUTO_GAP_MS = 20000

  // ---------- 投递列表 ----------
  const [items, setItems] = useState<JobRow[]>([])
  const [total, setTotal] = useState(0)
  const [sortKey, setSortKey] = useState<SortKey>('score')
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc')
  const [selected, setSelected] = useState<JobRow | null>(null)
  // 点开未投递岗位时，右栏要切到的 HR 会话 uid
  const [chatFocusUid, setChatFocusUid] = useState<string | null>(null)
  // Boss 消息抽屉是否展开：收起时列表可以排 3 列，展开时让位给聊天
  const [chatOpen, setChatOpen] = useState(false)
  // 用 ref 包一层，保证 ChatPanel 拿到的回调引用恒定
  const chatOpenCb = useRef((v: boolean) => setChatOpen(v))
  const onChatOpenChange = useCallback((v: boolean) => chatOpenCb.current(v), [])
  const onChatTokens = useCallback(
    (t: { total: number; greet: number; score?: number }) =>
      setTokens({ total: t.total, greet: t.greet, score: t.score ?? 0 }),
    [],
  )
  const onChatSent = useCallback(() => {
    loadJobs(0)
    setRefreshTick((t) => t + 1)
  }, [])
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [listLoading, setListLoading] = useState(false)
  // 列表状态页签：pending=未投递（默认） / delivered=已投递 / rejected=不投递
  const [listTab, setListTab] = useState<'pending' | 'delivered' | 'rejected'>('pending')
  const [tabCounts, setTabCounts] = useState<{ pending: number; delivered: number; rejected: number }>({
    pending: 0,
    delivered: 0,
    rejected: 0,
  })
  // 任何会影响列表的操作后 +1，用来触发页签计数刷新
  const [refreshTick, setRefreshTick] = useState(0)
  // 分值门槛：抓取时低于该分值的 JD 不入库（不影响列表展示）
  const [minScore, setMinScore] = useState(7)
  // 自定义薪资档位输入（下限/上限，单位 K）
  const [salLow, setSalLow] = useState('')
  const [salHigh, setSalHigh] = useState('')
  // 开关切换后先播放 2 秒移出动画，再从列表里移除
  const [leaving, setLeaving] = useState<Set<string>>(new Set())

  // 各平台登录态
  const [loggedIn, setLoggedIn] = useState<Record<string, boolean>>({})
  const [loginBusy, setLoginBusy] = useState<string>('')
  const [loginMsg, setLoginMsg] = useState<Record<string, string>>({})

  const logBoxRef = useRef<HTMLDivElement>(null)
  const listBoxRef = useRef<HTMLDivElement>(null)

  // ---------- 主题 ----------
  useEffect(() => {
    document.documentElement.classList.toggle('dark', dark)
  }, [dark])

  // 点开未投递岗位：右栏切到该岗位 HR 的会话。
  // 后端 /api/chat/open 按 encryptJobId / 公司名在 Boss 会话列表里找。
  // 点卡片展开时只用已有会话（不打扰 Boss）。
  // 点「打招呼」才是用户明确要主动发起：force_new 让后端去岗位详情页点「立即沟通」，
  // 把和这位 HR 的会话建出来并跳进聊天页（只建会话，不发任何内容）。
  const openJobChat = useCallback(async (j: JobRow, forceNew = false) => {
    const ejid = (j.job_id || '').startsWith('boss:') ? j.job_id.slice(5) : undefined
    setChatOpen(true)
    try {
      const r = await apiClient.chatOpen({
        encrypt_job_id: ejid,
        brand: j.company || undefined,
        force_new: forceNew,
      })
      if (r.ok && r.friend) {
        setChatFocusUid(String(r.friend.uid))
        if (r.created) {
          setErr('')
          setAutoMsg(`已向 ${j.company || '该公司'} 的 HR 发起沟通，右侧可以直接发消息`)
          window.setTimeout(() => setAutoMsg(''), 5000)
        }
        return
      }
      // 没建立过会话：右侧已经列出全部 HR，直接让用户点一位开聊（不当作错误）
      const n = r.candidates?.length || 0
      setErr(
        n > 0
          ? `这家公司有 ${n} 位 HR 可选，点右侧任意一位即可开始沟通`
          : `${j.company || '这个岗位'} 还没和 Boss 建会话，右侧已列出全部 HR，挑一位开始沟通`,
      )
      window.setTimeout(() => setErr(''), 4000)
    } catch (e: any) {
      setErr(String(e?.message || e))
      window.setTimeout(() => setErr(''), 4000)
    }
  }, [])

  // ---------- 自动打招呼驱动 ----------
  // 后端一次只处理一条（浏览器同一时刻只能给一个任务用），循环放在前端：
  //   取待投递 → 生成话术 → 发送 → 记 token → 等 20 秒左右 → 下一条
  // 停下来的时候：待投递发完 / 到每日上限 / 连续失败 3 次 / 用户手动暂停
  useEffect(() => {
    if (!autoOn || autoPaused) return
    let cancelled = false
    let timer = 0

    const tick = async () => {
      if (cancelled || autoInFlight.current) return
      if (autoDoneRef.current.size >= maxJdRef.current) {
        setAutoMsg(`已发满本轮上限 ${maxJdRef.current} 条，自动打招呼已停止`)
        setAutoOn(false)
        return
      }
      try {
        autoInFlight.current = true
        setAutoBusy(true)
        const n = await apiClient.autoGreetNext()
        if (!n.ok) {
          setAutoMsg(n.error || '取不到待投递岗位')
          setAutoOn(false)
          return
        }
        const next = (n.items || []).find((x) => !autoDoneRef.current.has(x.job_id))
        if (!next) {
          setAutoMsg('待投递列表已经发完了，自动打招呼已停止')
          setAutoOn(false)
          return
        }
        const r = await apiClient.autoGreetOne(next.job_id)
        if (r.tokens_total != null) {
          setTokens({ total: r.tokens_total, greet: r.tokens_greet ?? 0, score: r.tokens_score ?? 0 })
        }
        if (r.ok) {
          autoCountRef.current += 1
          autoFailRef.current = 0
          setAutoCount(autoCountRef.current)
          setAutoFail(0)
          setAutoMsg(`刚发给「${r.company || next.company}」（${r.tokens ?? 0} token）`)
        } else if (r.skip) {
          // 跳过不算失败：这个岗位没有 HR 会话，按已处理记下来继续下一条
          autoDoneRef.current.add(next.job_id)
          setAutoMsg(`跳过「${next.company}」：${r.error || ''}`)
        } else {
          autoFailRef.current += 1
          setAutoFail(autoFailRef.current)
          setAutoMsg(`「${next.company}」发送失败：${r.error || '未知原因'}`)
          if (autoFailRef.current >= 3) {
            setAutoPaused(true)
            setAutoMsg(`连续失败 3 次，已自动暂停：${r.error || ''}`)
            return
          }
        }
        setAutoTick((t) => t + 1)
      } catch (e: any) {
        autoFailRef.current += 1
        setAutoFail(autoFailRef.current)
        setAutoMsg(String(e?.message || e))
        if (autoFailRef.current >= 3) {
          setAutoPaused(true)
          setAutoMsg(`连续失败 3 次，已自动暂停：${String(e?.message || e)}`)
          return
        }
      } finally {
        autoInFlight.current = false
        setAutoBusy(false)
        if (!cancelled && autoOnRef.current && !autoPausedRef.current) {
          // 每条之间留个随机间隔，别像机器人在刷屏
          const gap = AUTO_GAP_MS + (Math.random() * 10000 - 5000)
          timer = window.setTimeout(tick, gap)
        }
      }
    }

    timer = window.setTimeout(tick, 400)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
    // autoTick 只是「再来一轮」的信号
  }, [autoOn, autoPaused, autoTick])

  const toggleAuto = () => {
    const next = !autoOn
    autoOnRef.current = next
    setAutoOn(next)
    if (next) {
      autoDoneRef.current = new Set()
      autoCountRef.current = 0
      autoFailRef.current = 0
      setAutoCount(0)
      setAutoFail(0)
      setAutoPaused(false)
      autoPausedRef.current = false
      setAutoMsg('自动打招呼已开启')
    } else {
      setAutoMsg('')
    }
  }

  // ---------- 拉取岗位（分页 + 排序） ----------
  // offset>0 表示「追加加载」（无限滚动），=0 表示刷新重来
  const loadJobs = useCallback(
    async (appendOffset = 0) => {
      setListLoading(true)
      try {
        const r = await apiClient.jobs({
          limit: PAGE_SIZE,
          offset: appendOffset,
          sort: sortKey,
          order: sortOrder,
          ...(listTab === 'pending'
            ? { include_rejected: false, status: 'pending' }
            : listTab === 'delivered'
              ? { status: 'delivered' }
              : { rejected_only: true }),
        })
        setTotal(r.total)
        // 先渲染，再把 id 记为"已出现"：新行会播一次淡入动画，之后轮询不再重复播
        setItems((prev) => (appendOffset > 0 ? [...prev, ...r.items] : r.items))
        window.setTimeout(() => {
          r.items.forEach((it) => seenIds.current.add(it.job_id))
        }, 600)
        setBackendOk(true)
      } catch (e: any) {
        setBackendOk(false)
        setErr(e?.message || String(e))
      } finally {
        setListLoading(false)
      }
    },
    [sortKey, sortOrder, listTab],
  )

  useEffect(() => {
    loadJobs(0)
  }, [loadJobs])

  // 三个状态页签的条数
  const loadTabCounts = useCallback(async () => {
    try {
      const [a, b, c] = await Promise.all([
        apiClient.jobs({ limit: 0, include_rejected: false, status: 'pending' }),
        apiClient.jobs({ limit: 0, status: 'delivered' }),
        apiClient.jobs({ limit: 0, rejected_only: true }),
      ])
      setTabCounts({ pending: a.total, delivered: b.total, rejected: c.total })
    } catch {
      /* 忽略：状态刷新失败不影响主流程 */
    }
  }, [])

  useEffect(() => {
    loadTabCounts()
    const timer = window.setInterval(() => loadTabCounts(), 5000)
    return () => window.clearInterval(timer)
  }, [loadTabCounts, refreshTick])

  // 滚动到底部自动加载更多
  // 滚动到底部自动追加下一页（每次 10 条）；appendLoading 防抖，避免一次滚动触发多次请求
  const [appendLoading, setAppendLoading] = useState(false)
  const onListScroll = useCallback(() => {
    const box = listBoxRef.current
    if (!box || listLoading || appendLoading) return
    if (items.length >= total) return
    if (box.scrollTop + box.clientHeight >= box.scrollHeight - 80) {
      setAppendLoading(true)
      loadJobs(items.length).finally(() => window.setTimeout(() => setAppendLoading(false), 300))
    }
  }, [items.length, total, listLoading, appendLoading, loadJobs])

  // ---------- 拉取状态 ----------
  // 把后端返回的登录会话状态同步到 UI：waiting/opening → 忙碌（黄点），logged_in → 绿点
  const syncLoginSessions = (sess?: Record<string, any>) => {
    if (!sess) return
    for (const [pid, v] of Object.entries(sess)) {
      const state = (v as any)?.state
      if (state === 'opening' || state === 'waiting') {
        setLoginBusy((b) => (b && b !== pid ? b : pid))
      } else if (state === 'logged_in') {
        setLoggedIn((v2) => ({ ...v2, [pid]: true }))
        setLoginBusy((b) => (b === pid ? '' : b))
      } else if (state === 'closed' || state === 'idle' || state === 'error') {
        setLoginBusy((b) => (b === pid ? '' : b))
      }
    }
  }

  const refresh = useCallback(async () => {
    try {
      const st = await apiClient.status()
      setStatus(st)
      setRunning(st.running)
      setPaused(!!st.paused)
      if (st.logged_in) setLoggedIn(st.logged_in)
      if (st.login_sessions) {
        const msg: Record<string, string> = {}
        for (const [k, v] of Object.entries(st.login_sessions)) {
          if (v.message) msg[k] = v.message
        }
        setLoginMsg(msg)
        syncLoginSessions(st.login_sessions)
      }
      setBackendOk(true)
    } catch (e: any) {
      setBackendOk(false)
      setErr(e?.message || String(e))
    }
  }, [])

  const loadSensitive = useCallback(async (kw: string, kindArg?: string) => {
    setSensitiveLoading(true)
    try {
      const kd = kindArg !== undefined ? kindArg : sensitiveKind
      const r = await apiClient.sensitive({
        keyword: kw.trim() || undefined,
        kind: kd || undefined,
        limit: 200,
      })
      setSensitive(r.items)
      setSensitiveTotal(r.total)
    } catch (e: any) {
      setErr(e?.message || String(e))
    } finally {
      setSensitiveLoading(false)
    }
  }, [sensitiveKind])

  useEffect(() => {
    refresh()
    apiClient
      .getConfig()
      .then((c) => {
        setConfig(c)
        setBaseUrl(c.base_url)
        setModel(c.model)
        setDark(c.theme === 'dark')
        if (c.exclude_keywords?.length) setExcludeText(c.exclude_keywords.join(','))
        if (c.exclude_rules) {
          setVetoWords(c.exclude_rules.name_words || [])
          setExclSalaryMin(c.exclude_rules.exclude_salary_min ?? null)
          setExclSalaryMax(c.exclude_rules.exclude_salary_max ?? null)
          setExclDegree(c.exclude_rules.exclude_degree || [])
          setExclFoundedBelow(c.exclude_rules.exclude_founded_below ?? null)
          setExclMaxYears(c.exclude_rules.exclude_resume_max_years ?? null)
          setExclHrDays(c.exclude_rules.exclude_hr_active_days ?? null)
          setWeekendRule(c.exclude_rules.weekend === 'double' ? 'double' : 'any')
        }
        if (typeof c.max_jd === 'number') setMaxJd(c.max_jd)
        if (typeof c.greet_prompt === 'string') setGreetPrompt(c.greet_prompt)
        if (typeof c.greet_prompt_default === 'string') setGreetPromptDefault(c.greet_prompt_default)
        if (typeof c.skills === 'string') setSkills(c.skills)
        if (c.score_mode === 'llm' || c.score_mode === 'rule') {
          setScoreMode(c.score_mode)
          scoreModeRef.current = c.score_mode
        }
        if (typeof c.llm_prefilter === 'number') {
          setPrefilter(c.llm_prefilter)
          prefilterRef.current = c.llm_prefilter
        }
        setTokens({ total: c.tokens_total ?? 0, greet: c.tokens_greet ?? 0, score: c.tokens_score ?? 0 })
      })
      .catch(() => {})
    const t = setInterval(refresh, 3000)
    return () => clearInterval(t)
  }, [refresh])

  // 岗位列表定时刷新（保持当前页与排序）
  useEffect(() => {
    const t = setInterval(() => loadJobs(0), 5000)
    return () => clearInterval(t)
  }, [loadJobs])

  // 日志实时拉取
  const lastLogTs = useRef(0)
  useEffect(() => {
    const t = setInterval(async () => {
      try {
        const r = await apiClient.logs(lastLogTs.current)
        if (r.items.length) {
          lastLogTs.current = r.now
          setLogs((prev) => [...prev, ...r.items].slice(-500))
        }
      } catch {
        /* ignore */
      }
    }, 1200)
    return () => clearInterval(t)
  }, [])

  useEffect(() => {
    if (logBoxRef.current) {
      logBoxRef.current.scrollTop = logBoxRef.current.scrollHeight
    }
  }, [logs])

  // ---------- 登录 ----------
  // 批量标记：先播放 2 秒移出动画，再提交给后端
  const batchReview = async (rejected: boolean) => {
    const ids = Array.from(checked)
    if (!ids.length) return
    setLeaving((prev) => {
      const nx = new Set(prev)
      ids.forEach((id) => nx.add(id))
      return nx
    })
    window.setTimeout(async () => {
      try {
        if (rejected) await apiClient.rejectJobs(ids)
        else await apiClient.allowJobs(ids)
        setChecked(new Set())
        await loadJobs(0)
        setRefreshTick((t) => t + 1)
      } catch (e: any) {
        setErr(e?.message || String(e))
      } finally {
        setLeaving((prev) => {
          const nx = new Set(prev)
          ids.forEach((id) => nx.delete(id))
          return nx
        })
      }
    }, 2000)
  }

  const doLogin = async (platform: string) => {
    setErr('')
    setLoginBusy(platform)
    try {
      await apiClient.loginStart(platform)
      setLoginMsg((m) => ({ ...m, [platform]: '登录窗口已弹出，请完成扫码登录' }))
      let tries = 0
      const timer = setInterval(async () => {
        tries += 1
        try {
          const s = await apiClient.loginStatus(platform)
          const idle = !s.running && (s.state === 'idle' || s.state === 'closed')
          // 会话还没起来时不要显示"idle"，避免一打开就显示空闲
          if (!idle || tries > 1) setLoginMsg((m) => ({ ...m, [platform]: s.message || s.state }))
          if (s.saved) {
            setLoggedIn((v) => ({ ...v, [platform]: true }))
            setLoginBusy('')
            clearInterval(timer)
            // 登录成功后立刻把 cookie 存下来：读取期望职位走纯 HTTP，需要这份 cookie
            try {
              await apiClient.loginSave(platform)
            } catch (e) {
              console.warn('保存登录 cookie 失败', e)
            }
            refresh()
          } else if (s.state === 'error') {
            setLoginBusy('')
            clearInterval(timer)
            setErr(`${platform} 登录失败：${s.message || '未知错误'}`)
          } else if (idle || tries > 150) {
            setLoginBusy('')
            clearInterval(timer)
            setLoginMsg((m) => ({ ...m, [platform]: idle ? '登录窗口已关闭（未检测到登录）' : '登录等待超时' }))
          }
        } catch {
          clearInterval(timer)
          setLoginBusy('')
        }
      }, 2000)
    } catch (e: any) {
      setLoginBusy('')
      setErr(e?.message || String(e))
    }
  }

  const doLogout = async (platform: string) => {
    try {
      await apiClient.logout(platform)
      setLoggedIn((v) => ({ ...v, [platform]: false }))
      setLoginMsg((m) => ({ ...m, [platform]: '已退出登录' }))
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  // ---------- 抓取 ----------
  const onStart = async () => {
    setErr('')
    if (!keywords.length) {
      setErr('请先填写关键词（在「关键词（可多个，逐个搜索）」输入框里回车添加）')
      setFiltersOpen(true)
      return
    }
    try {
      setPaused(false)
      await apiClient.start({
        platforms,
        keywords,
        city,
        salary,
        experience,
        degree,
        scale,
        stage,
        industry,
        job_type: jobType,
        exclude_keywords: SPLIT_KEYWORDS(excludeText),
        max_pages: maxPages,
        daily_limit: 100,
        max_jd: maxJd,
      })
      setRunning(true)
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  const onStop = async () => {
    try {
      await apiClient.stop()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  // 暂停 / 恢复：暂停时浏览器页面停在当前 JD 上，方便观察
  const onTogglePause = async () => {
    try {
      if (paused) {
        await apiClient.resume()
        setPaused(false)
      } else {
        await apiClient.pause()
        setPaused(true)
      }
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  const saveConfig = async () => {
    try {
      const patch: Record<string, any> = {
        base_url: baseUrl,
        model,
        theme: dark ? 'dark' : 'light',
        greet_prompt: greetPrompt,
        skills,
        score_mode: scoreModeRef.current,
        llm_prefilter: prefilterRef.current,
      }
      if (apiKey.trim()) patch.api_key = apiKey.trim()
      const r = await apiClient.setConfig(patch)
      setLlmEnabled(r.llm_enabled)
      setApiKey('')
      const c = await apiClient.getConfig()
      setConfig(c)
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  // 切打分方式：立即写库并重建打分器，下一轮抓取就按新模式打分
  const switchScoreMode = async (mode: 'rule' | 'llm') => {
    if (mode === scoreModeRef.current) return
    scoreModeRef.current = mode
    setScoreMode(mode)
    try {
      await apiClient.setConfig({ score_mode: mode, llm_prefilter: prefilterRef.current })
      setLlmEnabled((await apiClient.getConfig()).has_key)
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  // 局部覆盖：切 chip 时 state 还是旧值，需要把新值直接传进来
  const saveRules = async (patch: Partial<ExcludeRules> = {}) => {
    try {
      await apiClient.setConfig({
        exclude_rules: {
          name_words: vetoWords,
          exclude_salary_min: exclSalaryMin,
          exclude_salary_max: exclSalaryMax,
          exclude_degree: exclDegree,
          exclude_founded_below: exclFoundedBelow,
          exclude_resume_max_years: exclMaxYears,
          exclude_hr_active_days: exclHrDays,
      weekend: weekendRule === 'double' ? 'double' : null,
          ...patch,
        },
      })
      await refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  // ---------- 列表交互 ----------
  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortOrder((o) => (o === 'desc' ? 'asc' : 'desc'))
    } else {
      setSortKey(key)
      // 时间默认倒序（最新在前），分数/薪资默认正序由 desc 起
      setSortOrder('desc')
    }
    setItems([])
    setSelected(null)
  }

  const sortArrow = (key: SortKey) =>
    sortKey === key ? (sortOrder === 'desc' ? '↓' : '↑') : ''

  const toggleChecked = (id: string) => {
    setChecked((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const allChecked = items.length > 0 && items.every((j) => checked.has(j.job_id))

  const toggleAll = () => {
    setChecked((prev) => {
      const next = new Set(prev)
      if (allChecked) items.forEach((j) => next.delete(j.job_id))
      else items.forEach((j) => next.add(j.job_id))
      return next
    })
  }

  const allowSelected = async () => {
    const ids = Array.from(checked)
    if (!ids.length) return
    try {
      await apiClient.allowJobs(ids)
      setChecked(new Set())
      loadJobs(0)
      setRefreshTick((t) => t + 1)
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  const deliverSelected = async () => {
    const ids = Array.from(checked)
    if (!ids.length) return
    try {
      await apiClient.markDelivered(ids)
      setChecked(new Set())
      loadJobs(0)
      setRefreshTick((t) => t + 1)
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }

  const rejectSelected = async () => {
    const ids = Array.from(checked)
    if (!ids.length) return
    try {
      await apiClient.rejectJobs(ids)
      setChecked(new Set())
      setSelected(null)
      loadJobs(0)
      setRefreshTick((t) => t + 1)
      refresh()
    } catch (e: any) {
      setErr(e?.message || String(e))
    }
  }


  return (
    <div className="flex h-full flex-col bg-light-bg dark:bg-dark-bg">
      {/* 顶栏 */}
      <header className="flex h-11 shrink-0 items-center justify-between border-b border-slate-200 bg-light-card px-4 dark:border-dark-border dark:bg-dark-card">
        <div className="flex min-w-0 items-center gap-2 overflow-x-auto whitespace-nowrap">
          {backendOk === false && <span className="chip bg-rose-100 text-rose-700">后端未连接</span>}
          <span className="mx-1 h-4 w-px shrink-0 bg-slate-200 dark:bg-dark-border" />
          <span className="shrink-0 text-[10px] text-slate-400">招聘网站</span>
          {PLATFORMS.map((p) => {
            const on = loggedIn[p.id] === true
            const busy = loginBusy === p.id
            return (
              <span
                key={p.id}
                title={p.ready ? undefined : '该平台尚未接入，暂不可用'}
                className={`flex shrink-0 items-center gap-1 rounded-md border px-1.5 py-0.5 text-[11px] ${
                  !p.ready
                    ? 'cursor-not-allowed border-slate-200 bg-slate-100 opacity-60 dark:border-slate-700 dark:bg-slate-800'
                    : on
                      ? 'border-emerald-300 bg-emerald-50 dark:border-emerald-700 dark:bg-emerald-900/20'
                      : 'border-slate-200 dark:border-slate-700'
                }`}
              >
                <input
                  type="checkbox"
                  className="accent-emerald-500"
                  disabled={!p.ready}
                  checked={p.ready && platforms.includes(p.id)}
                  onChange={(e) =>
                    setPlatforms((prev) =>
                      e.target.checked ? [...prev, p.id] : prev.filter((x) => x !== p.id),
                    )
                  }
                />
                <span>{p.name}</span>
                {!p.ready && <span className="text-[9px] text-slate-400">未接入</span>}
                {/* 已连接=绿点，未连接=红点（未接入平台不显示） */}
                {p.ready && (
                <span
                  className={`h-2 w-2 rounded-full ${
                    busy
                      ? 'animate-pulse bg-amber-400 shadow-[0_0_0_2px_rgba(251,191,36,0.25)]'
                      : on
                        ? 'bg-emerald-500 shadow-[0_0_0_2px_rgba(16,185,129,0.25)]'
                        : 'bg-rose-500 shadow-[0_0_0_2px_rgba(244,63,94,0.2)]'
                  }`}
                  title={
                    busy
                      ? '正在等待登录完成…'
                      : on
                        ? '已连接（已登录）'
                        : loginMsg[p.id]
                          ? `未连接（未登录）：${loginMsg[p.id]}`
                          : '未连接（未登录）'
                  }
                />
                )}
                {p.ready && (on ? (
                  <button
                    className="text-[10px] text-slate-400 hover:text-rose-500"
                    onClick={() => doLogout(p.id)}
                    title="退出登录并清除登录态"
                  >
                    退出
                  </button>
                ) : (
                  <button
                    className={`cursor-pointer rounded px-1.5 py-0.5 text-[10px] font-medium text-white ${
                      busy ? 'bg-emerald-400' : 'bg-emerald-500 hover:bg-emerald-600 active:scale-95'
                    }`}
                    onClick={() => {
                      // 点一下：打开登录窗口；登录中再点一次：重新弹出窗口
                      doLogin(p.id)
                    }}
                    title={
                      busy
                        ? '登录窗口已打开，请在弹出的浏览器里完成扫码（可手动处理验证码）'
                        : '弹出登录窗口，扫码登录后自动保存登录态'
                    }
                  >
                    {busy ? '重新登录' : '登录'}
                  </button>
                ))}
              </span>
            )
          })}
          {/* 打分方式开关：和敏感库并排，切了就立刻生效 */}
          <span className="ml-1 flex shrink-0 items-center gap-1">
            <span className="flex overflow-hidden rounded-md border border-slate-300 text-[11px] dark:border-slate-600">
              <button
                className={`px-2 py-0.5 ${
                  scoreMode === 'llm'
                    ? 'bg-primary text-white'
                    : 'bg-white text-slate-600 dark:bg-slate-800 dark:text-slate-300'
                }`}
                onClick={() => void switchScoreMode('llm')}
                title="抓取到 JD 后先用规则初筛，过了初筛再用大模型结合简历 + 配置的技能 + 筛选条件细打分（消耗 token）"
              >
                大模型打分
              </button>
              <button
                className={`px-2 py-0.5 ${
                  scoreMode === 'rule'
                    ? 'bg-primary text-white'
                    : 'bg-white text-slate-600 dark:bg-slate-800 dark:text-slate-300'
                }`}
                onClick={() => void switchScoreMode('rule')}
                title="按内置规则打分，速度快、不消耗 token（默认）"
              >
                规则打分
              </button>
            </span>
            {scoreMode === 'llm' && (
              <label
                className="flex items-center gap-1 text-[10px] text-slate-500"
                title="规则分低于这条线的不调用大模型（省 token），高于它的才让大模型细打分"
              >
                初筛≥
                <input
                  type="number"
                  min={0}
                  max={10}
                  className="input h-6 w-11 !px-1 !py-0 text-center text-[10px]"
                  value={prefilter}
                  onChange={(e) => {
                    const v = Math.max(0, Math.min(10, Number(e.target.value) || 0))
                    setPrefilter(v)
                    prefilterRef.current = v
                  }}
                  onBlur={() => void switchScoreMode('llm')}
                />
                分
              </label>
            )}
          </span>
          <button
            className="chip shrink-0"
            onClick={() => {
              const next = !showSensitive
              setShowSensitive(next)
              if (next) loadSensitive(sensitiveKw)
              else setSensChecked(new Set())
            }}
            title="查看被排除的岗位（敏感库），可手动移出"
          >
            敏感库{sensitiveTotal ? ` ${sensitiveTotal}` : ''}
          </button>
        </div>
        <div className="flex items-center gap-2">
          <span
            className="flex items-center gap-2 text-[11px] text-slate-500 dark:text-slate-400"
            title="累计消耗 token，跨启动累加：合计 = 模型打分 + 打招呼生成"
          >
            <span>
              累计 <b className="text-slate-700 dark:text-slate-200">{fmtTokens(tokens.total)}</b> token
            </span>
            <span className="text-slate-300 dark:text-slate-600">|</span>
            <span title="模型打分消耗">
              模型打分 {fmtTokens(tokens.score)}
            </span>
            <span className="text-slate-300 dark:text-slate-600">|</span>
            <span title="打招呼生成消耗">
              打招呼 {fmtTokens(tokens.greet)}
            </span>
          </span>
          <button
            className={`btn-ghost shrink-0 whitespace-nowrap ${autoOn ? '!bg-emerald-50 !text-emerald-700 dark:!bg-emerald-950 dark:!text-emerald-300' : ''}`}
            onClick={toggleAuto}
            title="从「待投递」里逐条生成招呼语并发给 HR；发出去即标记为已投递"
          >
            自动打招呼 {autoOn ? '开' : '关'}
          </button>
          {autoOn && (
            <button
              className="btn-ghost shrink-0 whitespace-nowrap"
              onClick={() => {
                const v = !autoPaused
                autoPausedRef.current = v
                setAutoPaused(v)
                setAutoMsg(v ? '已暂停，点「继续」接着发' : '继续发送')
              }}
            >
              {autoPaused ? '继续' : '暂停'}
            </button>
          )}
          {autoOn && (
            <span
              className="max-w-[300px] shrink-0 truncate text-[11px] text-slate-500 dark:text-slate-400"
              title={autoMsg}
            >
              {autoBusy ? '正在发…' : autoMsg}
              {autoCount > 0 && `（已发 ${autoCount}`}
              {autoCount > 0 && autoFail > 0 ? `，连续失败 ${autoFail}）` : autoCount > 0 ? '）' : ''}
            </span>
          )}
          <button className="btn-ghost" onClick={() => setDrawer((v) => !v)}>
            {drawer ? '收起配置' : '配置'}
          </button>
          <button className="btn-ghost" onClick={() => setDark((v) => !v)}>
            {dark ? '浅色' : '深色'}
          </button>
        </div>
      </header>

      {err && (
        <div className="shrink-0 bg-rose-50 px-4 py-1 text-[11px] text-rose-700 dark:bg-rose-950 dark:text-rose-300">
          {err}
        </div>
      )}

      {/* 模型配置小窗（浮层，不占据页面空间） */}
      {drawer && (
        <div
          className="fixed inset-0 z-40 flex items-start justify-center bg-black/25 pt-24"
          onClick={() => setDrawer(false)}
        >
          <div
            className="card w-[520px] max-w-[92vw] p-4 shadow-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="mb-3 flex items-center">
              <h2 className="text-sm font-semibold">配置</h2>
              <span className="ml-2 text-[11px] text-slate-400">JD 打分与生成招呼共用这个模型</span>
              <button className="ml-auto chip" onClick={() => setDrawer(false)}>
                关闭
              </button>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="label">API Key</label>
                <input
                  className="input"
                  type="password"
                  placeholder={config?.has_key ? config.api_key_masked : 'sk-...'}
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                />
              </div>
              <div>
                <label className="label">Base URL</label>
                <input
                  className="input"
                  placeholder="https://api.deepseek.com"
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                />
              </div>
              <div>
                <label className="label">模型</label>
                <input className="input" value={model} onChange={(e) => setModel(e.target.value)} />
              </div>
              <div className="col-span-2">
                <label className="label flex items-center">
                  提示词
                  <span className="ml-2 font-normal text-slate-400">
                    「生成回复」按它 + 岗位 JD + 我的技能 写打招呼
                  </span>
                  {greetPromptDefault && (
                    <button
                      className="ml-auto text-[10px] text-primary hover:underline"
                      onClick={() => setGreetPrompt(greetPromptDefault)}
                    >
                      恢复默认
                    </button>
                  )}
                </label>
                <textarea
                  className="input min-h-[132px] resize-y !text-[11px] leading-5"
                  placeholder={greetPromptDefault || '在这里写提示词…'}
                  value={greetPrompt}
                  onChange={(e) => setGreetPrompt(e.target.value)}
                />
              </div>
              <div className="col-span-2">
                <label className="label flex items-center">
                  技能
                  <span className="ml-2 font-normal text-slate-400">
                    生成招呼语时优先用这里的技能；留空则用简历画像里的技能。多个用逗号/顿号分隔
                  </span>
                </label>
                <textarea
                  className="input min-h-[64px] resize-y !text-[11px] leading-5"
                  placeholder="例如：Java、Spring Boot、MySQL、Redis、Vue，10 年后端与全栈经验"
                  value={skills}
                  onChange={(e) => setSkills(e.target.value)}
                />
              </div>
              <div className="col-span-2 flex items-center gap-2">
                <span className="text-[11px] text-slate-400">
                  累计 {fmtTokens(tokens.total)} token · 模型打分 {fmtTokens(tokens.score)} · 打招呼{' '}
                  {fmtTokens(tokens.greet)}
                </span>
                <button className="ml-auto btn-primary" onClick={saveConfig}>
                  保存配置
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* 主体：左栏吃满，右侧是收起的竖栏 / 展开的一半宽抽屉 */}
      <div className="flex min-h-0 flex-1 gap-3 p-3">
        {/* ================= 左栏 ================= */}
        <section className="flex min-w-0 flex-[1] flex-col gap-3">
          {/* 顶部：筛选与抓取 */}
          <div className="card flex shrink-0 items-center gap-2 overflow-hidden whitespace-nowrap px-3 py-2">
            <button
              className="btn-ghost"
              onClick={() => setFiltersOpen((v) => !v)}
              title="展开/收起筛选条件"
            >
              {filtersOpen ? '收起筛选 ▲' : '筛选条件 ▼'}
            </button>
            {/* 开始 / 暂停 / 继续 合用一个按钮 */}
            <button
              className={`w-20 !px-2 ${!running || paused ? 'btn-primary' : 'btn-ghost'}`}
              onClick={!running ? onStart : onTogglePause}
              title={
                !running
                  ? '开始抓取'
                  : paused
                    ? '继续抓取'
                    : '暂停抓取（浏览器停在当前 JD，方便观察）'
              }
            >
              {!running ? '开始抓取' : paused ? '继续' : '暂停'}
            </button>
            <button className="btn-danger w-20 !px-2" disabled={!running} onClick={onStop}>
              停止抓取
            </button>
            <span
              className="min-w-0 flex-1 truncate text-[11px] text-slate-500"
              title={
                running
                  ? `${paused ? '已暂停' : '抓取中'}${status?.progress_text ? ' · ' + status.progress_text : ''}`
                  : status?.progress_text
                    ? `空闲 · ${status.progress_text}`
                    : '空闲'
              }
            >
              {running
                ? `${paused ? '已暂停' : '抓取中'}${status?.progress_text ? ' · ' + status.progress_text : ''}`
                : status?.progress_text
                  ? `空闲 · ${status.progress_text}`
                  : '空闲'}
            </span>
            {/* 右侧：本次抓取的 token 统计 + 有效 JD 上限 + 分值设定 */}
            <div className="ml-auto flex shrink-0 items-center gap-3 whitespace-nowrap text-[11px] text-slate-500">
              <label className="flex items-center gap-1" title="本次抓取最多入库多少条有效 JD，0 表示不限制（抓取中改动立即生效）">
                有效 JD 上限
                <input
                  className="input !h-6 w-12 !py-0 text-center [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                  type="number"
                  min={0}
                  value={maxJd}
                  onChange={(e) => setMaxJd(Math.max(0, Number(e.target.value) || 0))}
                  onBlur={() => apiClient.setLimit(maxJd).catch((e: any) => setErr(e?.message || String(e)))}
                />
                条
              </label>
              <label
                className="flex items-center gap-1"
                title="抓取时匹配度低于该分值的 JD 直接不入库（已入库的历史数据不受影响）"
              >
                分值门槛 ≥
                <input
                  className="input !h-6 w-12 !py-0 text-center [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                  type="number"
                  min={1}
                  max={10}
                  value={minScore}
                  onChange={(e) => setMinScore(Math.max(1, Math.min(10, Number(e.target.value) || 1)))}
                  onBlur={() =>
                    apiClient
                      .setLimit(maxJd, minScore)
                      .catch((e: any) => setErr(e?.message || String(e)))
                  }
                />
                分（低于不入库）
              </label>
            </div>
          </div>

          {/* 展开的筛选条件 */}
          {filtersOpen && (
            <div className="card shrink-0 p-3">
              {Object.entries(loginMsg).some(([k, v]) => v && !loggedIn[k]) && (
                <div className="mb-2 space-y-0.5">
                  {Object.entries(loginMsg)
                    .filter(([k, v]) => v && !loggedIn[k])
                    .map(([k, v]) => (
                      <div key={k} className="text-[11px] text-amber-600 dark:text-amber-400">
                        {PLATFORMS.find((p) => p.id === k)?.name ?? k}：{v}
                      </div>
                    ))}
                </div>
              )}
              {/* 关键词：可加多个，逐个搜索（Boss 的 query 只支持一个） */}
              <div className="mb-1.5">
                <label className="label">关键词（可多个，逐个搜索）</label>
                <div className="flex flex-wrap items-center gap-1.5">
                  {keywords.map((k) => (
                    <span
                      key={k}
                      className="inline-flex items-center gap-1 rounded bg-primary/10 px-2 py-0.5 text-[11px] text-primary"
                    >
                      {k}
                      <button
                        className="text-primary/70 hover:text-primary"
                        onClick={() => setKeywords(keywords.filter((x) => x !== k))}
                        title="删除"
                      >
                        ×
                      </button>
                    </span>
                  ))}
                  <input
                    className="input !w-40 !py-0.5 text-[11px]"
                    placeholder="输入后回车添加（可空格/逗号分隔多个）"
                    value={kwInput}
                    onChange={(e) => setKwInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        // 一次可粘贴多个：用空格/逗号/顿号/分号等分隔
                        const parts = SPLIT_KEYWORDS(kwInput).filter((v) => !keywords.includes(v))
                        if (parts.length) setKeywords([...keywords, ...parts])
                        setKwInput('')
                      }
                    }}
                  />
                  <button
                    className="chip"
                    title="从 Boss 直聘账号里读取「求职期望」，点一下加进关键词"
                    disabled={expectBusy}
                    onClick={async () => {
                      setExpectBusy(true)
                      setErr('')
                      try {
                        // 后端纯 HTTP 直读（不经过浏览器/采集线程），通常 <1 秒返回
                        const r = await apiClient.expectations('boss')
                        if (r.items?.length) {
                          setExpects(r.items)
                          setErr('')
                        } else {
                          const msg = r.error || '没有读到期望职位（账号里可能没配置，或登录已失效）'
                          // 登录态失效时顺手把登录按钮标出来，别让用户干瞪眼
                          if (/cookie|登录/.test(msg)) {
                            setLoggedIn((v) => ({ ...v, boss: false }))
                            refresh()
                          }
                          setErr(msg)
                        }
                      } catch (e: any) {
                        setErr(e?.message || String(e))
                      } finally {
                        setExpectBusy(false)
                      }
                    }}
                  >
                    {expectBusy ? '读取中…' : '读取期望职位'}
                  </button>
                </div>
                {(expects.length > 0 || RECOMMEND_KEYWORDS.length > 0) && (
                  <div className="mt-1 space-y-1 text-[11px] text-slate-500">
                    {expects.length > 0 && (
                      <div className="flex flex-wrap items-center gap-1">
                        <span className="shrink-0">Boss 期望职位</span>
                        {expects.map((e) => (
                          <button
                            key={e.name}
                            className={`chip ${keywords.includes(e.name) ? 'chip-on' : ''}`}
                            title={`点击加入关键词${e.city ? '（期望城市：' + e.city + '）' : ''}`}
                            onClick={() =>
                              setKeywords(keywords.includes(e.name) ? keywords : [...keywords, e.name])
                            }
                          >
                            {e.name}
                          </button>
                        ))}
                      </div>
                    )}
                    <div className="flex flex-wrap items-center gap-1">
                      <span className="shrink-0">推荐</span>
                      {RECOMMEND_KEYWORDS.map((k) => (
                        <button
                          key={k}
                          className={`chip ${keywords.includes(k) ? 'chip-on' : ''}`}
                          onClick={() => setKeywords(keywords.includes(k) ? keywords : [...keywords, k])}
                        >
                          {k}
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              <div className="grid grid-cols-4 items-start gap-x-3 gap-y-1.5">
                <div>
                  <label className="label">城市</label>
                  <select className="input" value={city} onChange={(e) => setCity(e.target.value)}>
                    {CITIES.map((c) => (
                      <option key={c.code} value={c.code}>
                        {c.name}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="label">职位类型</label>
                  <select className="input" value={jobType} onChange={(e) => setJobType(e.target.value)}>
                    <option value="">不限</option>
                    {JOB_TYPE_OPTIONS.map((o) => (
                      <option key={o.code} value={o.code}>
                        {o.name}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="label" title="每次搜索最多翻多少页（页数越多越慢、越容易被风控）">
                    搜索最大翻页次数
                  </label>
                  <input
                    className="input"
                    type="number"
                    min={1}
                    max={20}
                    value={maxPages}
                    onChange={(e) => setMaxPages(Number(e.target.value) || 1)}
                  />
                </div>
              </div>

              {/* 多选筛选项 + 排除规则：与 Boss 官网一致 */}
              <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5 items-start">
                <ChipRow
                  label="薪资"
                  options={SALARY_OPTIONS}
                  value={salary}
                  onToggle={(v) => setSalary(toggleIn(salary, v))}
                  onClear={() => setSalary([])}
                  extra={
                    <span className="ml-1 inline-flex items-center gap-1 align-middle text-[11px] text-slate-500">
                      <span className="shrink-0">自定义</span>
                      <input
                        className="input !h-6 !w-20 !py-0 text-center [appearance:textfield]"
                        type="number"
                        min={0}
                        placeholder="下限K"
                        value={salLow}
                        onChange={(e) => setSalLow(e.target.value)}
                      />
                      <span>-</span>
                      <input
                        className="input !h-6 !w-20 !py-0 text-center [appearance:textfield]"
                        type="number"
                        min={0}
                        placeholder="上限K"
                        value={salHigh}
                        onChange={(e) => setSalHigh(e.target.value)}
                      />
                      <button
                        className="chip"
                        title="把「下限-上限」加进薪资条件（上限留空 = XX K 以上）"
                        onClick={() => {
                          const low = Math.max(0, Number(salLow) || 0)
                          const high = Number(salHigh) || 0
                          if (!high && !low) return
                          const code = high > low ? `sal:${low}-${high}` : `sal:${low}-9999`
                          if (!salary.includes(code)) setSalary([...salary, code])
                          setSalLow('')
                          setSalHigh('')
                        }}
                      >
                        添加
                      </button>
                      {salary
                        .filter((c) => c.startsWith('sal:') && !SALARY_OPTIONS.some((o) => o.code === c))
                        .map((c) => {
                          const [a, b] = c.slice(4).split('-')
                          return (
                            <button
                              key={c}
                              className="chip chip-on"
                              title="点击移除这个自定义档位"
                              onClick={() => setSalary(salary.filter((x) => x !== c))}
                            >
                              {Number(b) >= 9000 ? `${a}K以上` : `${a}-${b}K`} ×
                            </button>
                          )
                        })}
                    </span>
                  }
                />

                <ChipRow
                  label="工作经验"
                  options={EXPERIENCE_OPTIONS}
                  value={experience}
                  onToggle={(v) => setExperience(toggleIn(experience, v))}
                  onClear={() => setExperience([])}
                />
                <ChipRow
                  label="学历"
                  options={DEGREE_OPTIONS}
                  value={degree}
                  onToggle={(v) => setDegree(toggleIn(degree, v))}
                  onClear={() => setDegree([])}
                />

                {/* 排除规则：留空 = 不排除；按行分组，更紧凑 */}
                <div className="col-span-2 mt-1 border-t border-slate-100 pt-2 dark:border-dark-border">

                  {/* 第 1 行：公司规模 + 融资阶段 */}
                  <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                    <ChipRow
                      label="公司规模"
                      options={SCALE_OPTIONS}
                      value={scale}
                      onToggle={(v) => setScale(toggleIn(scale, v))}
                      onClear={() => setScale([])}
                    />
                    <ChipRow
                      label="融资阶段"
                      options={STAGE_OPTIONS}
                      value={stage}
                      onToggle={(v) => setStage(toggleIn(stage, v))}
                      onClear={() => setStage([])}
                    />
                  </div>

                  {/* 第 2 行：行业 */}
                  <div className="mt-1 grid grid-cols-1 gap-x-4 gap-y-1">
                    <div>
                      <div className="mb-1 flex items-center justify-between">
                        <span className="text-[11px] text-slate-500">行业</span>
                        {industry.length > 0 && (
                          <button
                            className="text-[10px] text-slate-400 hover:text-primary"
                            onClick={() => setIndustry([])}
                          >
                            清空
                          </button>
                        )}
                      </div>
                      <div className="flex flex-wrap items-center gap-1">
                        <button className="chip" onClick={() => setIndustry([])} title="清空行业筛选">
                          行业不限
                        </button>
                        {INDUSTRY_OPTIONS.map((o) => {
                          const on = industry.includes(o.code)
                          return (
                            <button
                              key={o.code}
                              className={`chip ${on ? 'chip-on' : ''}`}
                              onClick={() => setIndustry(toggleIn(industry, o.code))}
                              title={`行业编码 ${o.code}`}
                            >
                              {o.name}
                              {on ? ' ×' : ''}
                            </button>
                          )
                        })}
                        <input
                          className="input !w-28 !py-0.5 text-[11px]"
                          placeholder="自定义编码回车"
                          value={industryInput}
                          onChange={(e) => setIndustryInput(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              const v = industryInput.trim()
                              if (v && !industry.includes(v)) setIndustry([...industry, v])
                              setIndustryInput('')
                            }
                          }}
                        />
                      </div>
                    </div>

                    <div className="mt-1 flex min-h-0 flex-col">
                      <label
                        className="flex flex-wrap items-center gap-x-2 text-[11px] text-slate-500"
                        title="空格/逗号/顿号/分号均可分隔，命中公司名或岗位名即排除"
                      >
                        <span>排除关键词</span>
                        <span className="font-normal text-slate-400">
                          空格/逗号/顿号分隔；任意一项留空即表示不按该项排除，改动后自动保存
                        </span>
                      </label>
                      <textarea
                        className="input mt-1 min-h-[72px] flex-1 resize-y leading-4"
                        rows={4}
                        placeholder="外包 驻场 兼职 日薪 跨境 外贸 金融 银行 证券 区块链 Web3 985 211 硕士 博士 销售 产品经理 总监"
                        value={excludeText}
                        onChange={(e) => {
                          setExcludeText(e.target.value)
                          setVetoWords(SPLIT_KEYWORDS(e.target.value))
                        }}
                      />
                    </div>
                  </div>

                  {/* 第 3 行：HR 活跃度 + 工作制 + JD 岗位要求 + 成立年限 */}
                  <div className="mt-1 grid grid-cols-4 gap-x-4 gap-y-1">
                    <div>
                      <label className="text-[11px] text-slate-500">HR 活跃度 → 排除</label>
                      <div className="mt-1 flex flex-wrap items-center gap-1">
                        <button
                          className={`chip ${exclHrDays === null ? 'chip-on' : ''}`}
                          onClick={() => {
                            setExclHrDays(null)
                            saveRules({ exclude_hr_active_days: null })
                          }}
                        >
                          不限
                        </button>
                        {HR_ACTIVE_OPTIONS.map((o) => (
                          <button
                            key={o.days}
                            className={`chip ${exclHrDays === o.days ? 'chip-on' : ''}`}
                            title={`只保留最近 ${o.days} 天内活跃的 HR；活跃度未知的也会被排除`}
                            onClick={() => {
                              setExclHrDays(o.days)
                              saveRules({ exclude_hr_active_days: o.days })
                            }}
                          >
                            {o.name}
                          </button>
                        ))}
                      </div>
                    </div>
                    <div>
                      <label className="text-[11px] text-slate-500">工作制</label>
                      <div className="mt-1 flex flex-wrap items-center gap-1">
                        {WEEKEND_OPTIONS.map((o) => (
                          <button
                            key={o.value}
                            className={`chip ${weekendRule === o.value ? 'chip-on' : ''}`}
                            title={
                              o.value === 'double'
                                ? '只要双休岗位；职位描述或岗位名出现单休 / 大小周 / 周六上班 / 做六休一 / 月休N天 就排除'
                                : o.value === 'no_bigweek'
                                  ? '勾选后排除大小周 / 单休 / 做六休一 / 月休N天 的岗位（对应 Boss 直聘筛选里的「大小周」排除项）'
                                  : '不限制休息制度'
                            }
                            onClick={() => {
                              setWeekendRule(o.value)
                              saveRules({ weekend: o.value === 'any' ? null : o.value })
                            }}
                          >
                            {o.name}
                          </button>
                        ))}
                      </div>
                    </div>
                    <div>
                      <label className="text-[11px] text-slate-500">JD 岗位要求 → 排除</label>
                      <div className="mt-1 flex flex-wrap items-center gap-1">
                        <button
                          className="chip"
                          onClick={() => {
                            setExclDegree([])
                            saveRules()
                          }}
                        >
                          学历不限
                        </button>
                        {DEGREE_VETO_OPTIONS.map((d) => (
                          <button
                            key={d}
                            className={`chip ${exclDegree.includes(d) ? 'chip-on' : ''}`}
                            onClick={() => {
                              const next = exclDegree.includes(d)
                                ? exclDegree.filter((x) => x !== d)
                                : [...exclDegree, d]
                              setExclDegree(next)
                              saveRules({ exclude_degree: next })
                            }}
                          >
                            {d}
                          </button>
                        ))}
                      </div>
                    </div>
                    <div>
                      <label className="truncate text-[11px] text-slate-500">成立年限少于（年）</label>
                      <input
                        className="input mt-1"
                        inputMode="numeric"
                        placeholder="留空 = 不排除"
                        value={exclFoundedBelow ?? ''}
                        onChange={(e) => setExclFoundedBelow(NUM_OR_NULL(e.target.value))}
                        onBlur={() => saveRules()}
                      />
                    </div>
                  </div>

                </div>
              </div>
            </div>
          )}

          {/* 投递列表 */}
          <div className="card flex min-h-0 flex-1 flex-col p-3">
            <div className="mb-2 flex items-center gap-2">
              <h2 className="text-xs font-semibold">投递列表</h2>
              <div className="flex items-center gap-1">
                {(
                  [
                    { key: 'pending', label: '未投递', n: tabCounts.pending, on: 'bg-emerald-500 text-white', off: 'bg-slate-100 text-slate-600 dark:bg-dark-border dark:text-slate-300' },
                    { key: 'delivered', label: '已投递', n: tabCounts.delivered, on: 'bg-slate-600 text-white', off: 'bg-slate-100 text-slate-600 dark:bg-dark-border dark:text-slate-300' },
                    { key: 'rejected', label: '不投递', n: tabCounts.rejected, on: 'bg-rose-500 text-white', off: 'bg-slate-100 text-slate-600 dark:bg-dark-border dark:text-slate-300' },
                  ] as const
                ).map((t) => (
                  <button
                    key={t.key}
                    onClick={() => switchTab(t.key)}
                    className={`rounded-full px-2 py-[2px] text-[11px] transition ${listTab === t.key ? t.on : t.off}`}
                  >
                    {t.label} <span className="tabular-nums opacity-80">{t.n}</span>
                  </button>
                ))}
              </div>
              <span className="text-[11px] text-slate-400">共 {total} 条</span>
              <button
                className="btn-ghost !px-1.5 !py-0.5 text-[11px]"
                title="刷新列表"
                onClick={() => {
                  loadJobs(0)
                  setRefreshTick((t) => t + 1)
                }}
              >
                {listLoading ? '刷新中…' : '刷新'}
              </button>
              <div className="ml-auto flex items-center gap-1 text-[11px]">
                <button
                  className="btn-ghost !px-2 !py-0.5"
                  onClick={() => toggleSort('time')}
                  title="按加入时间排序"
                >
                  加入时间 {sortArrow('time')}
                </button>
                <button
                  className="btn-ghost !px-2 !py-0.5"
                  onClick={() => toggleSort('salary')}
                  title="按薪资排序"
                >
                  薪资 {sortArrow('salary')}
                </button>
                <button
                  className="btn-ghost !px-2 !py-0.5"
                  onClick={() => toggleSort('score')}
                  title="按匹配度排序"
                >
                  分值 {sortArrow('score')}
                </button>
              </div>
            </div>

            {/* 批量操作条 */}
            <div className="mb-1 flex items-center gap-2 rounded-md bg-slate-50 px-2 py-1 text-[11px] dark:bg-slate-800/50">
              <label className="flex cursor-pointer items-center gap-1">
                <input
                  type="checkbox"
                  className="accent-emerald-500"
                  checked={allChecked}
                  onChange={toggleAll}
                />
                全选本页
              </label>
              <span className="font-medium text-slate-500">已选 {checked.size} 条</span>
              <span className="text-slate-400">列表共 {total} 条</span>
              {checked.size >= 2 ? (
                <div className="ml-auto flex items-center gap-2">
                  <button
                    className="rounded border border-rose-400 bg-rose-50 px-2 py-0.5 font-semibold text-rose-600 hover:bg-rose-100 active:scale-95 dark:border-rose-600 dark:bg-rose-950/40 dark:text-rose-400"
                    onClick={() => batchReview(true)}
                  >
                    不投递（{checked.size}）
                  </button>
                  <button
                    className="rounded border border-emerald-400 bg-emerald-50 px-2 py-0.5 font-semibold text-emerald-600 hover:bg-emerald-100 active:scale-95 dark:border-emerald-600 dark:bg-emerald-950/40 dark:text-emerald-400"
                    onClick={() => batchReview(false)}
                  >
                    允许投递（{checked.size}）
                  </button>
                </div>
              ) : (
                <span className="ml-auto text-slate-400">
                  勾选 2 条以上可批量标记；单条直接用行内标签
                </span>
              )}
            </div>

            {/* 投递卡片：按列分栏（竖向排），展开只撑高自己这一列，不影响旁边卡片 */}
            <div
              ref={listBoxRef}
              onScroll={onListScroll}
              className={`min-h-0 flex-1 overflow-y-auto ${listFadeKey > 0 ? 'list-fade' : ''}`}
              key={`list-${listFadeKey}`}
            >
              <div style={{ columnCount: chatOpen ? 2 : 3, columnGap: '6px' }}>

              {items.length === 0 && !listLoading && (
                <div className="py-8 text-center text-[11px] text-slate-400" style={{ columnSpan: 'all' }}>
                  暂无岗位
                </div>
              )}
              {items.map((j, idx) => (
                <div
                  key={j.job_id}
                  onClick={(e) => {
                    const el = e.target as HTMLElement
                    if (el.closest('button, a, input, label, select, textarea')) return
                    const next = selected?.job_id === j.job_id ? null : j
                    setSelected(next)
                    if (next && next.status !== 'delivered') openJobChat(next)
                  }}
                  className={`mb-1.5 break-inside-avoid overflow-hidden rounded-lg border px-2 py-1.5 dark:border-dark-border transition-all duration-500 ease-out ${
                    seenIds.current.has(j.job_id) ? '' : 'row-in'
                  } ${
                    checked.has(j.job_id)
                      ? 'border-primary bg-primary/5 ring-1 ring-primary/40 shadow-sm dark:bg-primary/10'
                      : j.status === 'delivered'
                        ? 'border-slate-200 bg-slate-100/60 opacity-55 dark:bg-slate-800/40'
                        : j.rejected
                          ? 'border-rose-200 bg-rose-50/50 opacity-75 dark:bg-rose-950/20'
                          : 'border-slate-100 bg-white hover:border-primary/40 hover:shadow-sm dark:bg-transparent'
                  } ${
                    leaving.has(j.job_id)
                      ? 'pointer-events-none -translate-x-6 scale-[0.98] opacity-0'
                      : 'translate-x-0 opacity-100'
                  }`}
                >
                  {/* 第 1 行：勾选 + 分数 + 职位名 + 标记 */}
                  <div className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      className={`h-4 w-4 shrink-0 cursor-pointer rounded-md border-2 transition ${
                        checked.has(j.job_id)
                          ? 'border-primary bg-primary accent-primary shadow-[0_0_0_3px_rgba(16,185,129,0.18)]'
                          : 'border-slate-300 bg-white accent-primary hover:border-primary/70 dark:border-slate-500 dark:bg-transparent'
                      }`}
                      checked={checked.has(j.job_id)}
                      onChange={() => toggleChecked(j.job_id)}
                      onClick={(e) => e.stopPropagation()}
                      title={checked.has(j.job_id) ? '取消勾选' : '勾选后可批量操作'}
                    />
                    <span
                      className={
                        'chip w-7 shrink-0 justify-center font-bold ' +
                        (j.status === 'delivered' ? 'bg-slate-300 text-slate-600' : CARD_SCORE_COLOR(j.match_score))
                      }
                      title="匹配度评分"
                    >
                      {j.match_score ?? '-'}
                    </span>
                    <span
                      className={
                        'min-w-0 flex-1 cursor-pointer truncate font-medium ' +
                        (j.status === 'delivered' ? 'text-slate-500 line-through' : '')
                      }
                      onClick={() => {
                        const next = selected?.job_id === j.job_id ? null : j
                        setSelected(next)
                        if (next && next.status !== 'delivered') openJobChat(next)
                      }}
                      title={j.title ?? ''}
                    >
                      {j.title}
                    </span>
                    {j.status !== 'delivered' && (
                      <ReviewSwitch
                        rejected={!!j.rejected}
                        onToggle={(next) => {
                          setLeaving((prev) => new Set(prev).add(j.job_id))
                          window.setTimeout(async () => {
                            try {
                              if (next) await apiClient.rejectJobs([j.job_id])
                              else await apiClient.allowJobs([j.job_id])
                              await loadJobs(0)
                              setRefreshTick((t) => t + 1)
                            } catch (e: any) {
                              setErr(e?.message || String(e))
                            } finally {
                              setLeaving((prev) => {
                                const nx = new Set(prev)
                                nx.delete(j.job_id)
                                return nx
                              })
                            }
                          }, 2000)
                        }}
                      />
                    )}
                  </div>
                  {/* 第 2 行：公司名 + 薪资 + 平台 + 状态（标记按钮在第 1 行右上角） */}
                  <div className="mt-1 flex min-w-0 items-center gap-1.5 overflow-hidden">
                    <span
                      className="min-w-0 flex-1 truncate text-[11px] font-medium text-slate-600 dark:text-slate-300"
                      title={j.company ?? ''}
                    >
                      {j.company || '（未知公司）'}
                    </span>
                    <span className="shrink-0 text-[12px] font-semibold text-secondary">
                      {j.salary ?? '-'}
                    </span>
                    <span
                      className={
                        'shrink-0 rounded px-1 py-0.5 text-[9px] font-medium ' +
                        PLATFORM_CHIP_COLOR(j.platform)
                      }
                      title={`来源平台：${platformName(j.platform)}`}
                    >
                      {platformName(j.platform)}
                    </span>
                    {STATUS_CHIP(j.status, j.rejected)}
                  </div>
                  <div className="mt-[3px] flex min-w-0 items-center gap-1.5 overflow-hidden text-[11px] text-slate-500">
                    <span className="shrink-0">{j.location ?? '-'}</span>
                    <span className="shrink-0 text-slate-300">|</span>
                    <span className="shrink-0">{j.experience ?? '-'}</span>
                    <span className="shrink-0 text-slate-300">|</span>
                    <span className="shrink-0">{j.degree ?? '-'}</span>
                    {j.company_scale && (
                      <>
                        <span className="shrink-0 text-slate-300">|</span>
                        <span className="shrink-0">{j.company_scale}</span>
                      </>
                    )}
                    <span className="ml-auto shrink-0 text-[10px] text-slate-400">
                      {fmtDate(j.created_at)}
                    </span>
                  </div>
                  {/* 展开提示：单条卡片高度有限，点任意空白处展开 JD 详情 */}
                  {selected?.job_id !== j.job_id && (
                    <div className="mt-[3px] flex items-center gap-1 text-[10px] text-slate-400">
                      <span className="truncate">
                        {j.rejected ? '已标记不投递 · 单击展开详情 / JD' : '单击展开详情 / JD'}
                      </span>
                      {j.status !== 'delivered' && (
                        <button
                          className="shrink-0 rounded border border-primary/40 px-1 text-[10px] text-primary hover:bg-primary/10"
                          title="主动和这位 HR 发起沟通：还没聊过就去岗位页点「立即沟通」建会话，然后跳进聊天框"
                          onClick={(e) => {
                            e.stopPropagation()
                            setSelected(j)
                            openJobChat(j, true)
                          }}
                        >
                          打招呼
                        </button>
                      )}
                      {j.company_address && (
                        <span
                          className="ml-auto max-w-[45%] shrink truncate text-slate-400"
                          title={j.company_address}
                        >
                          {/* 去掉「点击查看地图」那行尾巴 */}
                          📍 {String(j.company_address).split('\n')[0].trim()}
                        </span>
                      )}
                      <span className={`${j.company_address ? 'shrink-0' : 'ml-auto shrink-0'} text-slate-300`}>
                        ▾
                      </span>
                    </div>
                  )}

                  {selected?.job_id === j.job_id && (
                    <div className="mt-2 border-t border-slate-200 pt-2 text-[11px] dark:border-dark-border">
                      <div className="mb-1 grid grid-cols-2 gap-x-3 gap-y-0.5 text-slate-600 dark:text-slate-300">
                        <div>公司规模：{j.company_scale ?? '-'}</div>
                        <div>融资阶段：{j.company_stage ?? '-'}</div>
                        <div>行业：{j.company_industry ?? '-'}</div>
                        <div>在招岗位：{j.hiring_count != null ? `${j.hiring_count} 个` : '-'}</div>
                        <div>
                          成立年限：
                          {j.founded_years != null
                            ? `${j.founded_years} 年${j.founded_year ? `（${j.founded_year} 年成立）` : ''}`
                            : '-'}
                        </div>
                        <div>注册资本：{j.reg_capital ?? '-'}</div>
                        <div>HR：{j.hr_name ?? '-'}</div>
                        <div>
                          HR 活跃度：
                          <span
                            className={
                              ACTIVE_LEVEL(j.hr_active_status) === 'high'
                                ? 'text-emerald-600 dark:text-emerald-400'
                                : ACTIVE_LEVEL(j.hr_active_status) === 'mid'
                                  ? 'text-amber-600 dark:text-amber-400'
                                  : ACTIVE_LEVEL(j.hr_active_status) === 'low'
                                    ? 'text-rose-500'
                                    : ''
                            }
                          >
                            {j.hr_active_status ?? '-'}
                          </span>
                        </div>
                        <div>学历：{j.degree ?? '-'}</div>
                        <div>经验：{j.experience ?? '-'}</div>
                        <div className="flex items-center gap-1">
                          <span>
                            打分依据（{j.score_source === 'llm' ? '大模型' : '规则'}
                            {j.token_used ? ` · ${j.token_used} token` : ''}）：
                          </span>
                        </div>
                        <div className="flex flex-wrap gap-1">{PARSED_REASON(j)}</div>
                      </div>
                      <div className="mt-1 max-h-48 overflow-y-auto whitespace-pre-wrap text-slate-600 dark:text-slate-300">
                        {j.jd_text || '（该平台未提供 JD 全文）'}
                      </div>
                    </div>
                  )}
                </div>
              ))}
              </div>
            </div>

            {/* 只在加载中/还有更多时提示，全部加载完就不显示了 */}
            {(listLoading || items.length < total) && (
              <div className="mt-1 shrink-0 text-center text-[11px] text-slate-400">
                {listLoading
                  ? '加载中…'
                  : `已加载 ${items.length} / ${total} 条 · 滚动到底自动加载`}
              </div>
            )}
          </div>

          {/* 左下：合并的日志区 */}
          <div className="card flex h-44 shrink-0 flex-col p-3">
            <div className="mb-1 flex items-center gap-2">
              <h2 className="text-xs font-semibold">决策日志 / 运行日志</h2>
              <span className="font-normal text-slate-400">实时</span>
            </div>
            <div
              ref={logBoxRef}
              className="min-h-0 flex-1 overflow-y-auto font-mono text-[10px] leading-4 text-slate-500"
            >
              {logs.length === 0 && <div className="text-slate-400">（暂无）</div>}
              {logs.slice(-300).map((l, i) => (
                <div
                  key={i}
                  className={
                    l.level === 'error'
                      ? 'text-rose-500'
                      : l.level === 'warn'
                        ? 'text-amber-500'
                        : ''
                  }
                >
                  {fmtTime(l.ts)} {l.message}
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* ========== 右栏：Boss 消息列表常显；对话框收起时只留列表，
             展开时对话框占掉剩下的宽度 ========== */}
        <div
          className={`flex min-h-0 shrink-0 flex-col rounded-lg transition-[width] duration-200 ${
            chatOpen ? 'w-[60%]' : 'w-[34%]'
          }`}
        >
          <ChatPanel
            focusUid={chatFocusUid}
            onSent={onChatSent}
            onTokens={onChatTokens}
            onOpenChange={onChatOpenChange}
          />
        </div>
      </div>

      {/* 敏感库浮层：盖在页面之上，关闭后下面的 Boss 消息面板还在，不用重新加载 */}
      {showSensitive && (
        <div
          className="fixed inset-0 z-40 flex items-start justify-center bg-black/25 pt-12"
          onClick={() => {
            setShowSensitive(false)
            setSensChecked(new Set())
          }}
        >
          <div onClick={(e) => e.stopPropagation()} className="contents">
            <div className="card flex max-h-[86vh] w-[min(1080px,94vw)] flex-col overflow-hidden p-3 shadow-xl">
                <div className="mb-2 flex items-center gap-2">
                  <h2 className="text-xs font-semibold">敏感库 {sensitiveTotal} 条</h2>
                  <span className="text-[10px] text-slate-400">
                    已抓过的岗位（已入库 + 被排除）都会记在这里，下一轮抓取直接跳过；可手动移出
                  </span>
                  <button className="ml-auto chip" onClick={() => setShowSensitive(false)}>
                    关闭
                  </button>
                </div>
                <div className="mb-2 flex items-center gap-2">
                  <input
                    className="input h-7 flex-1 !py-0 text-[11px]"
                    placeholder="搜索公司 / 岗位 / 排除原因"
                    value={sensitiveKw}
                    onChange={(e) => setSensitiveKw(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') loadSensitive(sensitiveKw)
                    }}
                  />
                  <button className="chip" onClick={() => loadSensitive(sensitiveKw)}>
                    搜索
                  </button>
                  <button
                    className="chip"
                    onClick={() => {
                      setSensitiveKw('')
                      loadSensitive('')
                    }}
                  >
                    重置
                  </button>
                  {[
                    { k: '', name: '全部' },
                    { k: 'stored', name: '已入库' },
                    { k: 'rejected', name: '已排除' },
                  ].map((t) => (
                    <button
                      key={t.k || 'all'}
                      className={`chip whitespace-nowrap ${sensitiveKind === t.k ? 'chip-on' : ''}`}
                      onClick={() => {
                        setSensitiveKind(t.k)
                        loadSensitive(sensitiveKw, t.k)
                      }}
                    >
                      {t.name}
                    </button>
                  ))}
                  <button
                    className="chip"
                    onClick={() =>
                      setSensChecked(
                        sensChecked.size === sensitive.length
                          ? new Set()
                          : new Set(sensitive.map((r) => r.id)),
                      )
                    }
                    disabled={sensitive.length === 0}
                  >
                    {sensitive.length > 0 && sensChecked.size === sensitive.length ? '取消全选' : '全选'}
                  </button>
                  <button
                    className="chip !text-rose-600 disabled:opacity-40"
                    disabled={sensChecked.size === 0}
                    onClick={async () => {
                      await apiClient.deleteSensitive(Array.from(sensChecked))
                      setSensChecked(new Set())
                      loadSensitive(sensitiveKw)
                    }}
                  >
                    移出所选（{sensChecked.size}）
                  </button>
                </div>
                <div
                  className="grid min-h-0 flex-1 auto-rows-min content-start items-start gap-1.5 overflow-y-auto"
                  style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))' }}
                >
                  {sensitiveLoading && (
                    <div className="col-span-full text-[11px] text-slate-400">加载中…</div>
                  )}
                  {!sensitiveLoading && sensitive.length === 0 && (
                    <div className="col-span-full text-[11px] text-slate-400">（暂无记录）</div>
                  )}
                  {sensitive.map((r) => (
                    <div
                      key={r.id}
                      className={`flex flex-col overflow-hidden rounded-lg border px-2 py-1.5 transition-all duration-300 dark:border-dark-border ${
                        r.kind === 'stored'
                          ? 'border-emerald-100 bg-white hover:border-emerald-300 hover:shadow-sm dark:bg-transparent'
                          : 'border-rose-200 bg-rose-50/40 hover:border-rose-300 hover:shadow-sm dark:bg-rose-950/20'
                      }`}
                    >
                      {/* 第 1 行：网站来源 + 公司 + 薪资 */}
                      <div className="flex min-w-0 items-center gap-1.5">
                        <input
                          type="checkbox"
                          className="h-3.5 w-3.5 shrink-0 cursor-pointer rounded border-slate-300 accent-rose-500"
                          checked={sensChecked.has(r.id)}
                          onChange={() =>
                            setSensChecked((prev) => {
                              const nx = new Set(prev)
                              if (nx.has(r.id)) nx.delete(r.id)
                              else nx.add(r.id)
                              return nx
                            })
                          }
                        />
                        <span
                          className={'shrink-0 rounded px-1 py-0.5 text-[9px] font-medium ' + PLATFORM_CHIP_COLOR(r.platform)}
                          title={`网站来源：${platformName(r.platform)}`}
                        >
                          {platformName(r.platform)}
                        </span>
                        <span
                          className={`shrink-0 rounded px-1 py-0.5 text-[9px] font-medium ${
                            r.kind === 'stored'
                              ? 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-300'
                              : 'bg-rose-50 text-rose-500 dark:bg-rose-900/30 dark:text-rose-300'
                          }`}
                          title={r.kind === 'stored' ? '已入库（抓取过，会跳过）' : '已排除（原因见下行）'}
                        >
                          {r.kind === 'stored' ? '已入库' : '已排除'}
                        </span>
                        <span
                          className="min-w-0 flex-1 truncate text-[12px] font-semibold"
                          title={r.company || ''}
                        >
                          {r.company || '-'}
                        </span>
                        <span className="shrink-0 text-[11px] font-semibold text-secondary">
                          {r.salary || '-'}
                        </span>
                      </div>
                      {/* 第 2 行：岗位名 + 记录类型 */}
                      <div className="mt-[3px] flex min-w-0 items-center gap-1">
                        <span
                          className={`shrink-0 rounded px-1 text-[10px] font-medium ${
                            r.kind === 'stored'
                              ? 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-300'
                              : 'bg-rose-50 text-rose-500 dark:bg-rose-900/30 dark:text-rose-300'
                          }`}
                        >
                          {r.kind === 'stored' ? '已入库' : '已排除'}
                        </span>
                        <span className="min-w-0 flex-1 truncate text-[11px] text-slate-600 dark:text-slate-300" title={r.title || ''}>
                          {r.title || '-'}
                        </span>
                      </div>
                      {/* 第 3 行：排除原因（可换行） + 移出按钮 */}
                      <div className="mt-1 flex min-w-0 items-start gap-1">
                        <span
                          className={`min-w-0 flex-1 text-[11px] leading-4 ${
                            r.kind === 'stored' ? 'text-slate-500' : 'text-rose-500'
                          }`}
                          title={r.reason || ''}
                        >
                          {r.kind === 'stored'
                            ? `已投递信息：${r.reason || '已入库（抓取过，下一轮自动跳过）'}`
                            : `排除原因：${r.reason || '-'}`}
                        </span>
                        <span className="mt-[1px] shrink-0 text-[10px] text-slate-400">{fmtTime(r.created_at)}</span>
                        <button
                          className="shrink-0 whitespace-nowrap rounded border border-slate-300 px-1 text-[10px] text-slate-500 hover:border-rose-400 hover:text-rose-500"
                          onClick={async () => {
                            await apiClient.deleteSensitive([r.id])
                            loadSensitive(sensitiveKw)
                          }}
                          title="从敏感库移出（下次抓取不再跳过）"
                        >
                          移出
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
          </div>
        </div>
      )}
    </div>
  )
}
