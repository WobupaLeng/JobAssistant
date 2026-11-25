/**
 * 抓取互斥按钮与进度文案的纯逻辑。三态：idle / running / paused；「暂停」就是
 * `scrape_stop`（进度保留），再点「继续抓取」会重新 `scrape_start`。本文件只负责显示什么
 * 与点击发哪条命令，不负责抓取编排。
 */
import type { ScrapeProgress, ScrapeUpdate } from '../api/types';

export type ScrapeButtonState = 'idle' | 'running' | 'paused';

/**
 * `max_jd` 输入框的上限，与后端 `MAX_JD_CAP = 500` 对齐：超过它 `set_limit` 会直接拒绝
 * 而不是静默夹住，所以界面先夹一次，免得出现「保存失败但值已生效」。
 */
export const MAX_JD_CEILING = 500;

export interface ScrapeProgressLike {
  readonly running: boolean;
  readonly scraped: number;
  readonly inserted: number;
  readonly duplicates: number;
  readonly below_threshold: number;
  readonly rejected: number;
  readonly last_title?: string | null;
}

/** 从命令返回体里取字段，缺字段用 0 兜底（后端旧版本可能没有 duplicates）。 */
export function fromProgress(p: Partial<ScrapeProgress> | null | undefined): ScrapeProgressLike {
  const v = (n: unknown): number => (typeof n === 'number' && Number.isFinite(n) ? n : 0);
  return {
    running: p?.running === true,
    scraped: v(p?.scraped),
    inserted: v(p?.inserted),
    duplicates: v(p?.duplicates),
    below_threshold: v(p?.below_threshold),
    rejected: v(p?.rejected),
    last_title: p?.last_title ?? null,
  };
}

/**
 * 输入：进度里的 `running` + 用户是否点过暂停。点了暂停但 sidecar 还没退出时仍算 running，
 * 只有真的停了才显示「继续抓取」。
 */
export function buttonState(running: boolean, pausedByUser: boolean): ScrapeButtonState {
  if (running) return 'running';
  if (pausedByUser) return 'paused';
  return 'idle';
}

/** 按钮文案：「暂停抓取」在跑时不叫「停止」——用户以为能续。 */
export function buttonLabel(state: ScrapeButtonState): string {
  switch (state) {
    case 'running':
      return '暂停抓取';
    case 'paused':
      return '继续抓取';
    case 'idle':
    default:
      return '开始抓取';
  }
}

/** 按钮的 class：`.btn-pause` 是琥珀色，与 `.btn-primary` 拉开"破坏性"层级。 */
export function buttonClass(state: ScrapeButtonState): string {
  switch (state) {
    case 'running':
      return 'btn btn-sm btn-pause';
    case 'paused':
    case 'idle':
    default:
      return 'btn btn-sm btn-primary';
  }
}

/** 顶条一句话进度（C9 逐字）：「抓取中 · 已入库 12 条 · 已抓 30 · 低于门槛 5 · 规则否决 13」。 */
export function progressText(p: ScrapeProgressLike): string {
  const head = p.running ? '抓取中' : '空闲';
  return `${head} · 已入库 ${p.inserted} 条 · 已抓 ${p.scraped} · 低于门槛 ${p.below_threshold} · 规则否决 ${p.rejected}`;
}

/** sr-only 摘要（C9 逐字，五个数一个不能少）。 */
export function srSummary(p: ScrapeProgressLike): string {
  return `已抓 ${p.scraped} · 入库 ${p.inserted} · 重复 ${p.duplicates} · 低于门槛 ${p.below_threshold} · 规则否决 ${p.rejected}`;
}

export interface ProgressStat {
  readonly testId: string;
  readonly label: string;
  readonly value: number;
  readonly tone: '' | 'is-primary' | 'is-warn' | 'is-bad';
}

/** 五个 Stat，标签逐字（C9）：已抓 / 入库 / 重复 / 低于门槛 / 规则否决。 */
export function progressStats(p: ScrapeProgressLike): ProgressStat[] {
  return [
    { testId: 'scrape-scraped', label: '已抓', value: p.scraped, tone: '' },
    { testId: 'scrape-inserted', label: '入库', value: p.inserted, tone: p.inserted > 0 ? 'is-primary' : '' },
    { testId: 'scrape-duplicates', label: '重复', value: p.duplicates, tone: '' },
    { testId: 'scrape-below', label: '低于门槛', value: p.below_threshold, tone: '' },
    { testId: 'scrape-rejected', label: '规则否决', value: p.rejected, tone: p.rejected > 0 ? 'is-warn' : '' },
  ];
}

/** 「最近：标题」/「最近：—」。 */
export function lastTitleText(title: string | null | undefined): string {
  const t = (title ?? '').trim();
  return `最近：${t === '' ? '—' : t}`;
}

/** 启动按钮是否禁用：启动中或已在跑都不能再点。 */
export function startDisabled(starting: boolean, running: boolean): boolean {
  return starting || running;
}

/**
 * 抓取条主按钮是否禁用：只有「启动中」。
 * 跑着的时候主按钮文案是「暂停抓取」、点击 emit stop——再套 running 禁用会让暂停变灰不可点。
 */
export function mainDisabled(starting: boolean): boolean {
  return starting;
}

/**
 * 数据库里三条 `status` 之外的计算模式选择。选中大模型打分后顶栏多一个 0-100 的初筛输入
 * （`llm_prefilter`，默认 50）：规则分低于它就不调大模型。初筛判定在后端，前端只负责选值与落库。
 */
export interface ScoreModeSeg {
  readonly key: 'rule' | 'llm';
  readonly label: string;
  readonly active: boolean;
  readonly disabled: boolean;
  readonly testId: string;
}

export function scoreModeSegs(selected: 'rule' | 'llm'): ScoreModeSeg[] {
  return [
    {
      key: 'rule',
      label: '规则打分',
      active: selected === 'rule',
      disabled: false,
      testId: 'scrape-mode-rule-seg',
    },
    {
      key: 'llm',
      label: '大模型打分',
      active: selected === 'llm',
      disabled: false,
      testId: 'scrape-mode-llm-seg',
    },
  ];
}

/** 初筛线默认值，与后端 `DEFAULT_LLM_PREFILTER` / `LLM_PREFILTER_DEFAULT` 对齐。 */
export const LLM_PREFILTER_DEFAULT = 50;

/** 初筛线上限（百分制）。 */
export const LLM_PREFILTER_MAX = 100;

/** 初筛线夹到 0-100 的整数；非数字回落到默认 50（输入框被清空时不写脏值）。 */
export function clampPrefilter(v: unknown): number {
  // 空串不能被 Number() 当成 0：输入框清空时回落默认值，而不是偷偷设成「不设初筛」
  if (typeof v === 'string' && v.trim() === '') return LLM_PREFILTER_DEFAULT;
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n)) return LLM_PREFILTER_DEFAULT;
  return Math.min(LLM_PREFILTER_MAX, Math.max(0, Math.trunc(n)));
}

/** 积分显示：「1,234」千分位；数字越小越不该占位。 */
export function tokenText(n: number): string {
  const v = Math.max(0, Math.trunc(n));
  return v.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

// ---------------------------------------------------------------- 事件流

/**
 * 事件流里一行。`tag` 是行首标记，`tone` 决定颜色（`panel.rs` 的 FeedLine）。
 * `ts` 是**接收时刻**（`ScrapeUpdate` 不带时间戳，前端 drain 时补），用于与运行日志
 * 在同一条时间线上混排；`n` 是单调序号，只用来给混排后的行一个稳定 key。
 */
export interface FeedLine {
  readonly ts: number;
  readonly n: number;
  readonly tag: string;
  readonly msg: string;
  readonly tone: '' | 'ok' | 'skip' | 'warn' | 'bad';
}

/**
 * 入库标记。**这不是装饰**（C9）：E2E 用 `feed.count("✔")` 统计入库条数，
 * 改字符等于把那条断言弄瞎。
 */
export const FEED_ACCEPTED_TAG = '✔';

/** 被规则跳过的标记（`panel.rs` to_feed_line 的另一半）。 */
export const FEED_SKIPPED_TAG = '跳过';

/** 事件流最多留多少行（超出从头部丢，`FEED_MAX`）。 */
export const FEED_MAX = 300;

/**
 * `ScrapeUpdate` → 事件流一行。只转「岗位」这一型：日志 / 登录 / 畸形 / 退出 / 重启各有
 * 去处，混进事件流会淹掉进度信息。
 *
 * `now` / `n` 由调用方给：`now` 是这一批增量被 drain 的本地时刻（与运行日志同一时钟），
 * `n` 是全局递增序号，混排后靠它区分同一毫秒的两条事件。
 */
export function toFeedLine(u: ScrapeUpdate, now: number = Date.now(), n: number = 0): FeedLine | null {
  if (u.kind !== 'job') return null;
  const summary = String(u.summary ?? '').trim();
  if (u.accepted) {
    const score = typeof u.score === 'number' ? `${u.score} 分 · ` : '';
    return { ts: now, n, tag: FEED_ACCEPTED_TAG, msg: `${score}${summary}`, tone: 'ok' };
  }
  return { ts: now, n, tag: FEED_SKIPPED_TAG, msg: summary, tone: 'skip' };
}

/** 事件流只留最后 `FEED_MAX` 行。 */
export function trimFeed(lines: readonly FeedLine[], max = FEED_MAX): FeedLine[] {
  if (lines.length <= max) return lines.slice();
  return lines.slice(lines.length - max);
}
