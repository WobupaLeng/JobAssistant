/**
 * 抓取互斥按钮与进度文案的纯逻辑（C9 的文案在这里逐字定死）。
 *
 * 三态：idle（未在跑）/ running（在跑）/ paused（用户暂停）。
 * 注意"暂停"在 Leptos 版就是 `scrape_stop`——sidecar 停止后进度仍保留，
 * 再点「继续抓取」会重新 `scrape_start`。所以本文件只负责**显示成什么**
 * 与**点了发哪条命令**，不负责真正的抓取编排。
 */
import type { ScrapeProgress } from '../api/types';

export type ScrapeButtonState = 'idle' | 'running' | 'paused';

/**
 * `max_jd` 输入框的上限。
 *
 * 与后端 `core/domain/app_config.rs:162` 的 `MAX_JD_CAP = 500` 对齐：
 * 超过它 `set_limit` 会**直接拒绝**（不是静默夹住），所以界面先夹一次，
 * 避免用户看到「保存失败但值已经生效」。这是后端常量，不是业务门槛。
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

export function emptyProgress(): ScrapeProgressLike {
  return {
    running: false,
    scraped: 0,
    inserted: 0,
    duplicates: 0,
    below_threshold: 0,
    rejected: 0,
    last_title: null,
  };
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
 * 输入：进度里的 `running` + 用户是否点过暂停。
 *
 * 用户点了暂停但 sidecar 还没退出（`running` 仍为 true）时保持 running：
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
 * 数据库里三条 `status` 之外，计算模式的选择。
 *
 * LLM 打分本轮**不可选**（`scrape-mode-llm-seg` 恒 disabled + aria-disabled）：
 * 那需要配置里填 key，没配时选了只会每个岗位报一次错。
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
      label: 'LLM 打分',
      // LLM 恒不激活：即使配置里有 key，本轮也不给选（SPEC 裁决）。
      active: false,
      disabled: true,
      testId: 'scrape-mode-llm-seg',
    },
  ];
}

/** 积分显示：「1,234」千分位；数字越小越不该占位。 */
export function tokenText(n: number): string {
  const v = Math.max(0, Math.trunc(n));
  return v.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}
