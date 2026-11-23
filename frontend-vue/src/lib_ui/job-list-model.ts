/**
 * 投递列表的纯逻辑：页签 / 排序 / 可见性 / 计数 / 批量显隐 / 卡片派生字段。
 *
 * 为什么单独一个 `.ts`：Leptos 版这些规则散在 `features/jobs/state.rs` 与
 * `features/jobs/cards.rs` 的组件体里，只能靠肉眼看；搬成纯函数后 vitest 能
 * 逐条盯住（`tests/jobs-list.test.ts`）。组件只负责画，不再自己算。
 *
 * 层级：`features` 可以引 `api` 与 `lib_ui`（AGENTS.md §2 第 1 条），
 * 所以这里直接引 `api/types.ts` 的 `Job` 与 `lib_ui` 的 `fmtTime`。
 */
import type { Job, JobTabCounts } from '../api/types';
import { tidSafe } from './options';
import { fmtTime } from './time';
import { splitCompanyJob } from './logfmt';

// ---------------------------------------------------------------- 页签

/** 三个互斥页签。顺序即渲染顺序（`state.rs:26-67`）。 */
export type ListTab = 'pending' | 'delivered' | 'rejected';

export interface TabMeta {
  readonly key: ListTab;
  readonly slug: string;
  readonly label: string;
}

export const LIST_TABS: readonly TabMeta[] = [
  { key: 'pending', slug: 'pending', label: '未投递' },
  { key: 'delivered', slug: 'delivered', label: '已投递' },
  { key: 'rejected', slug: 'rejected', label: '不投递' },
];

/** 默认落在「未投递」：打开就该看见该干的活。 */
export const LIST_TAB_DEFAULT: ListTab = 'pending';

export function tabMeta(tab: ListTab): TabMeta {
  return LIST_TABS.find((t) => t.key === tab) ?? LIST_TABS[0]!;
}

/** 只有「不投递」要让后端带上被排除的岗位（`state.rs:55-57`）。 */
export function tabIncludeRejected(tab: ListTab): boolean {
  return tab === 'rejected';
}

/**
 * 发给 `list_jobs` 的 `tab` 参数。
 *
 * 「查看全部」时**不带** `tab`，后端才不按三态过滤。
 */
export function tabQuery(tab: ListTab, showAll: boolean): string | undefined {
  return showAll ? undefined : tabMeta(tab).slug;
}

// ---------------------------------------------------------------- 排序

export type SortField = 'created' | 'salary' | 'score';

export interface SortMeta {
  readonly key: SortField;
  readonly slug: string;
  readonly label: string;
}

export const SORT_FIELDS: readonly SortMeta[] = [
  { key: 'created', slug: 'time', label: '加入时间' },
  { key: 'salary', slug: 'salary', label: '薪资' },
  { key: 'score', slug: 'score', label: '分值' },
];

export const SORT_DEFAULT: SortField = 'created';

/** 加入时间的 testid 片段是 `time` 而不是 `created`（SPEC 4.4 定死）。 */
export function sortSlug(field: SortField): string {
  return SORT_FIELDS.find((s) => s.key === field)?.slug ?? 'time';
}

/**
 * 每个字段的默认方向。
 *
 * 界面上的箭头语义是「走强的方向」：分值/薪资先看大的（降序），
 * 加入时间先看新的（也是降序）。Leptos 版默认 `ascending = true` 是因为它
 * 把 `asc/desc` 直传给后端；这里改成更直白的 `desc`，由 `sortOrder()` 统一翻译。
 */
export function defaultDescending(field: SortField): boolean {
  switch (field) {
    case 'salary':
    case 'score':
      return true;
    case 'created':
    default:
      return true;
  }
}

/** 同字段再点一次就反向；换字段用该字段的默认方向（`state.rs:257-264`）。 */
export function nextSort(field: SortField, descending: boolean): boolean {
  return !descending;
}

/** `descending` → 后端 `ascending` 布尔（两者互为反义）。 */
export function sortAscending(descending: boolean): boolean {
  return !descending;
}

/** 后端 `order` 字符串：与 `ascending` 说的是同一件事（历史包袱，两个都给）。 */
export function sortOrder(descending: boolean): 'asc' | 'desc' {
  return descending ? 'desc' : 'asc';
}

/** 箭头字符：与 `.sort-arrow` 一致（↓ 降序 / ↑ 升序）。 */
export function sortArrow(descending: boolean): string {
  return descending ? '↓' : '↑';
}

// ---------------------------------------------------------------- 可见性

export function isRejectedJob(job: Job): boolean {
  return job.rejected !== 0;
}

/** 已投递：`status === 'delivered'` 且没有被否决。 */
export function isDeliveredJob(job: Job): boolean {
  return job.status === 'delivered' && job.rejected === 0;
}

/**
 * 界面侧再过滤一次。
 *
 * 后端 `list_jobs` 已按 `tab` 过滤并分页（`JobQuery::for_tab`），
 * 这层是幂等的兜底，不再是唯一过滤。
 */
export function visibleJobs(jobs: readonly Job[], tab: ListTab, showAll: boolean): Job[] {
  if (showAll) return jobs.slice();
  switch (tab) {
    case 'rejected':
      return jobs.filter(isRejectedJob);
    case 'delivered':
      return jobs.filter(isDeliveredJob);
    case 'pending':
    default:
      return jobs.filter((j) => j.status !== 'delivered' && !isRejectedJob(j));
  }
}

// ---------------------------------------------------------------- 计数

export function emptyTabCounts(): JobTabCounts {
  return { pending: 0, delivered: 0, rejected: 0 };
}

/**
 * 「共 N 条」。
 *
 * **不用** `db_overview().total`：那是含历史否决的全库行数，与页签的三态
 * 口径不是一回事，混用会出现「三个页签加起来不等于全部」（`state.rs:287-302`）。
 */
export function headCount(counts: JobTabCounts, tab: ListTab, showAll: boolean): number {
  if (showAll) return Math.max(0, counts.pending + counts.delivered + counts.rejected);
  const v = counts[tab] ?? 0;
  return Math.max(0, v);
}

export function tabCount(counts: JobTabCounts, tab: ListTab): number {
  return Math.max(0, counts[tab] ?? 0);
}

// ---------------------------------------------------------------- 批量

/** 批量按钮 `hidden` 的唯一判据：已选 ≥ 2（SPEC §15.2 裁决 Q）。 */
export function showBatchActions(selectedCount: number): boolean {
  return selectedCount >= 2;
}

/** 全选本页：已选数量等于可见数量且非空。 */
export function allVisibleChecked(selected: readonly string[], visibleIds: readonly string[]): boolean {
  if (visibleIds.length === 0) return false;
  return visibleIds.every((id) => selected.includes(id));
}

/** 勾选/取消单个（保持点击顺序）。 */
export function toggleChecked(selected: readonly string[], id: string): string[] {
  const i = selected.indexOf(id);
  if (i >= 0) {
    const next = selected.slice();
    next.splice(i, 1);
    return next;
  }
  return [...selected, id];
}

/** 批量文案：`不投递（3）` / `允许投递（3）`。 */
export function batchLabel(op: 'reject' | 'allow', n: number): string {
  return op === 'reject' ? `不投递（${n}）` : `允许投递（${n}）`;
}

// ---------------------------------------------------------------- 卡片派生

export type ScoreTier = 'is-high' | 'is-mid' | 'is-low' | 'is-bad' | 'is-none' | 'is-done';

/**
 * 分值方块档位，按**满分比例**算（`cards.rs:36-63`）。
 *
 * 阈值 80% / 60% / 40% 是"占满分的比例"，不是绝对分——满分 100 与满分 10
 * 同一段代码都要对。所以不传 `maxScore` 就没有档位（`is-none`），
 * 绝不用硬编码的 80 分当界限（AGENTS.md C10）。
 *
 * 用整数比较 `10 * score` 与 `k * max`：浮点里 `0.6 * 60` 是 36.000004，
 * 会把"满分 60 拿 60 分"判成低档（实测踩到过）。
 */
export function scoreTier(score: number, maxScore: number | null, delivered: boolean): ScoreTier {
  if (delivered) return 'is-done';
  if (maxScore === null || maxScore <= 0) return 'is-none';
  if (score <= 0) return 'is-none';
  const s = score * 10;
  if (s >= 8 * maxScore) return 'is-high';
  if (s >= 6 * maxScore) return 'is-mid';
  if (s >= 4 * maxScore) return 'is-low';
  return 'is-bad';
}

export type StatusTone = 'bad' | 'primary' | 'default';

export interface StatusChip {
  readonly tone: StatusTone;
  readonly label: string;
}

/**
 * 状态芯片三态（SPEC 7.11）。
 *
 * **不要**用 `status` 原值渲染——库里还有 `pending` / `new` / 空串三种历史写法
 * （`cards.rs:22-32`）。
 */
export function statusChip(job: Job): StatusChip {
  if (isRejectedJob(job)) return { tone: 'bad', label: '不投递' };
  if (job.status === 'delivered') return { tone: 'primary', label: '已投递' };
  return { tone: 'default', label: '待投递' };
}

/** 元信息行：城市 / 经验 / 学历 / 规模，空项跳过（分隔符由 CSS 画）。 */
export function metaItems(job: Job): string[] {
  return [job.location, job.experience, job.degree, job.company_scale]
    .map((s) => (s ?? '').trim())
    .filter((s) => s.length > 0);
}

/** 地址只取首行（旧版 `company_address.split('\n')[0]`）。 */
export function addrFirstLine(raw: string | null | undefined): string {
  const line = (raw ?? '').split('\n')[0] ?? '';
  return line.trim();
}

export interface CardView {
  readonly tid: string;
  readonly jobId: string;
  readonly title: string;
  readonly company: string;
  /** 原始公司名（未做「未知公司」占位替换），仅用于与聊天 Boss 的公司联动匹配。 */
  readonly rawCompany: string;
  readonly salary: string;
  readonly meta: string[];
  readonly metaText: string;
  readonly addr: string;
  readonly hasAddr: boolean;
  readonly score: number;
  readonly scoreTier: ScoreTier;
  readonly status: StatusChip;
  readonly delivered: boolean;
  readonly rejected: boolean;
  readonly time: string;
}

const DASH = '—';

/**
 * 卡片展示模型：一次算完，模板只做插值。
 *
 * 空字段的兜底文案与 Leptos 版逐字一致（`cards.rs:126-152`）——
 * 「（无标题）」/「（未知公司）」/「面议」。
 */
export function cardView(job: Job, maxScore: number | null): CardView {
  const title = job.title.trim() === '' ? '（无标题）' : job.title;
  const company = job.company.trim() === '' ? '（未知公司）' : job.company;
  const salary = job.salary.trim() === '' ? '面议' : job.salary;
  const meta = metaItems(job);
  const addr = addrFirstLine(job.company_address);
  const delivered = isDeliveredJob(job);
  return {
    tid: tidSafe(job.job_id),
    jobId: job.job_id,
    title,
    company,
    rawCompany: job.company.trim(),
    salary,
    meta,
    metaText: meta.length > 0 ? meta.join(' / ') : DASH,
    addr,
    hasAddr: addr.length > 0,
    score: job.match_score,
    scoreTier: scoreTier(job.match_score, maxScore, delivered),
    status: statusChip(job),
    delivered,
    rejected: isRejectedJob(job),
    time: fmtTime(job.created_at),
  };
}

/** 卡片点击是否该被当成「展开详情」：点在控件上就不算（`cards.rs:112-125`）。 */
export function shouldOpenCard(tagName: string): boolean {
  const tag = tagName.toLowerCase();
  return !['button', 'a', 'input', 'label', 'select', 'textarea'].includes(tag);
}

/** 保活锚点表的一行：`job-row-{id}` / `score-{id}` + 5 个单元格（C8）。 */
export interface KeepaliveRow {
  readonly jobId: string;
  readonly score: string;
  readonly title: string;
  readonly company: string;
  readonly location: string;
  readonly time: string;
}

export function keepaliveRows(jobs: readonly Job[]): KeepaliveRow[] {
  return jobs.map((job) => ({
    jobId: job.job_id,
    score: String(job.match_score),
    title: job.title.trim() === '' ? DASH : job.title,
    company: job.company.trim() === '' ? DASH : job.company,
    location: job.location.trim() === '' ? DASH : job.location,
    time: fmtTime(job.created_at),
  }));
}

/** 决策/日志里常见的 `公司 / 岗位` 串拆开（Leptos 版已退役，见 git 历史，提交 53d05cb 之前）。 */
export function splitCompanyTitle(raw: string): [string, string] {
  return splitCompanyJob(raw);
}
