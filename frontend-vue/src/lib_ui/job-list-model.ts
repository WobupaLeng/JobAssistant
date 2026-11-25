/**
 * 投递列表的纯逻辑：页签 / 排序 / 可见性 / 计数 / 批量显隐 / 卡片派生字段。
 * 组件只负责画，规则全在这里，由 `tests/jobs-list.test.ts` 逐条盯住。
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
 * 每个字段的默认方向，统一为降序（箭头语义是「走强的方向」）；
 * 发给后端时由 `sortOrder()` 统一翻译。
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
 * 界面侧再过滤一次。后端 `list_jobs` 已按 `tab` 过滤并分页，这层是幂等的兜底。
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
 * 「共 N 条」。**不用** `db_overview().total`：那是含历史否决的全库行数，
 * 混用会出现「三个页签加起来不等于全部」。
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

/** 批量按钮的唯一判据：已选 ≥ 1 即出（单选也能直接操作），0 条不出。 */
export function showBatchActions(selectedCount: number): boolean {
  return selectedCount >= 1;
}

/** 批量条上允许的操作。与后端命令一一对应，故不接受其它值。 */
export type BatchOp = 'reject' | 'deliver' | 'allow';

/**
 * 页签决定这一屏允许做哪几个批量动作：未投递 → 不投递 / 已投递；
 * 不投递 → 取消否决；已投递 → 取消投递标记。页签互斥，不会出现自相矛盾的组合。
 */
export function batchOps(tab: ListTab): readonly BatchOp[] {
  switch (tab) {
    case 'rejected':
      return ['allow'];
    case 'delivered':
      return ['allow'];
    default:
      return ['reject', 'deliver'];
  }
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

/**
 * 批量文案。`allow` 后端只有一条命令，但在两个页签含义不同（撤销否决 / 撤销投递标记），
 * 都说「待投递」会与页签名「未投递」对不上，所以按页签拆开写。
 */
export function batchLabel(op: BatchOp, n: number, tab: ListTab = 'pending'): string {
  if (op === 'reject') return `不投递（${n}）`;
  if (op === 'deliver') return `已投递（${n}）`;
  return `${tab === 'delivered' ? '取消投递' : '撤销否决'}（${n}）`;
}

// ---------------------------------------------------------------- 卡片派生

export type ScoreTier = 'is-high' | 'is-mid' | 'is-low' | 'is-bad' | 'is-none' | 'is-done';

/**
 * 分值方块档位，按**满分比例**（80% / 60% / 40%）算，不是绝对分；没有 `maxScore`
 * 就没有档位，**绝不用硬编码的 80 分当界限**（AGENTS.md C10）。
 * 比较走整数 `10 * score` 与 `k * max`，浮点误差会把满分判成低档。
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
 * 状态芯片三态（SPEC 7.11）。**不要**用 `status` 原值渲染——库里还有
 * `pending` / `new` / 空串三种历史写法。
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
  /** 条目本身：打开详情时整条交付，父级不回列表反查（列表只是分页窗口）。 */
  readonly job: Job;
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

/** 卡片展示模型：一次算完，模板只做插值。空字段兜底为「（无标题）」/「（未知公司）」/「面议」。 */
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
    job,
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

