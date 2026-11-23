/**
 * M4 决策记录页的纯逻辑：运行日志 seq 游标、决策行视图模型、筛选。
 *
 * 对照 Leptos 版已退役，见 git 历史，提交 53d05cb 之前的决策记录状态机，
 * 但运行日志增量改用 **seq 游标**（根 AGENTS.md §7.1）：
 * 后端 `list_logs(since)` 是闭区间（ts_ms >= since），游标那一毫秒会重送；
 * 旧版按 (ts,level,message) 多重集去重，同毫秒多条会误删。这里改为
 * 「since 走时间戳保证不漏 + afterSeq 过滤保证不重」，恰好一次。
 *
 * 决策行按冻结原型的 `.dl-row` 四列网格渲染（时间 / 行为 / 分数变化 / 原因），
 * 不用 Leptos 的五列表格。分数变化复用 lib_ui/logfmt 的 scoreDelta（两列皆 null
 * 时回落 reason 解析，绝不显示 0 → 0）。
 */

import type { DecisionRecord, LogRow } from '../../api/types';
import {
  decisionLabel,
  decisionScores,
  decisionTone,
  fmtDecisionTime,
  logVisible,
  matchesKeyword,
  parseLogLevel,
  reasonText,
  scoreDelta,
  splitCompanyJob,
  type LogLevel,
  type ScoreDelta,
} from '../../lib_ui/logfmt';

/** 自动刷新间隔（与 Leptos AUTO_REFRESH_MS 一致）。 */
export const AUTO_REFRESH_MS = 2000;
/** 决策记录每页条数（无限滚动一次拉这么多；后端 limit=0 会回落 200）。 */
export const DECISION_PAGE_SIZE = 10;

/* ------------------------------------------------------------------ *
 * 运行日志：seq 游标
 * ------------------------------------------------------------------ */

/** 一条运行日志的视图行。key 用 ts-seq，同毫秒多条也唯一。 */
export interface RunRow {
  ts: number;
  seq: number;
  level: LogLevel;
  /** 后端原始 level 串（tooltip 用）。 */
  levelRaw: string;
  message: string;
  key: string;
}

export function rowsFromReply(items: readonly LogRow[]): RunRow[] {
  return items.map((r) => ({
    ts: r.ts,
    seq: r.seq,
    level: parseLogLevel(r.level),
    levelRaw: r.level,
    message: r.message,
    key: `${r.ts}-${r.seq}`,
  }));
}

/**
 * 增量游标。`since` 是上次回执的 now（后端时钟 ms），null = 从头全量拉；
 * `afterSeq` 是已见过的最大序号，用来过滤闭区间重送的边界条目。
 */
export interface LogCursor {
  afterSeq: number;
  since: number | null;
}

export const INITIAL_CURSOR: LogCursor = { afterSeq: 0, since: null };

/**
 * 合并增量：只收 seq > afterSeq 的新行，按时间倒序（同毫秒按 seq 倒序）。
 * 没有新行时原样返回旧数组的副本（不触发无谓的重排）。
 */
export function mergeRunRows(
  old: readonly RunRow[],
  incoming: readonly RunRow[],
  afterSeq: number,
): RunRow[] {
  const fresh = incoming.filter((r) => r.seq > afterSeq);
  if (fresh.length === 0) return old.slice();
  return [...old, ...fresh].sort((a, b) => b.ts - a.ts || b.seq - a.seq);
}

/**
 * 推进游标：afterSeq 取见过的最大 seq；since 一律取本次回执 now
 * （即使本次空回也要推进，否则下次又全量拉）。
 */
export function advanceCursor(
  prev: LogCursor,
  incoming: readonly RunRow[],
  now: number,
): LogCursor {
  let afterSeq = prev.afterSeq;
  for (const r of incoming) if (r.seq > afterSeq) afterSeq = r.seq;
  return { afterSeq, since: now };
}

/** 级别筛选开关：选中再点取消；选满三档回到「全部」（空数组）。 */
export function toggleLevel(selected: readonly LogLevel[], lv: LogLevel): LogLevel[] {
  const next = selected.includes(lv) ? selected.filter((x) => x !== lv) : [...selected, lv];
  return next.length >= 3 ? [] : next;
}

export function filterRuns(
  rows: readonly RunRow[],
  selected: readonly LogLevel[],
  keyword: string,
): RunRow[] {
  return rows.filter((r) => logVisible(r.level, r.message, keyword, selected));
}

export function runCountLabel(shown: number, total: number, filtered: boolean): string {
  return filtered ? `${shown} / ${total} 条` : `${total} 条`;
}

/* ------------------------------------------------------------------ *
 * 视图切换
 * ------------------------------------------------------------------ */

export type LogView = 'runtime' | 'decision';

export const LOG_VIEWS: readonly { slug: LogView; label: string }[] = [
  { slug: 'decision', label: '决策日志' },
  { slug: 'runtime', label: '运行日志' },
];

/* ------------------------------------------------------------------ *
 * 决策记录
 * ------------------------------------------------------------------ */

export type DecisionTone = ReturnType<typeof decisionTone>;

/** 决策行的展示视图模型（组件只消费这个纯对象，不 import features）。 */
export interface DecisionRowView {
  id: number;
  /** MM-DD HH:mm（created_at 是 Unix 秒）。 */
  time: string;
  /** 行为文案：排除 / 入库 / 重算 / 未标注。 */
  kind: string;
  tone: DecisionTone;
  /** 分数变化文案，如「62 → 78」；无记录时「—」。 */
  verdict: string;
  verdictTitle: string;
  scoreDir: ScoreDelta['dir'];
  /** 「公司 · 岗位 · 原因」整行（tooltip 同）。 */
  reason: string;
  reasonEmpty: boolean;
  /** dl-row + is-in / is-out。 */
  rowClass: string;
  company: string;
  title: string;
}

export function scoreTitle(dir: ScoreDelta['dir']): string {
  switch (dir) {
    case 'up':
      return '分数上调';
    case 'down':
      return '分数下调';
    case 'same':
      return '分数未变';
    default:
      return '没有分数记录';
  }
}

/** 行配色：入库绿、排除红；重算按升降；其它不着色。 */
export function decisionRowClass(tone: DecisionTone, dir: ScoreDelta['dir']): string {
  if (tone === 'primary') return 'dl-row is-in';
  if (tone === 'bad') return 'dl-row is-out';
  if (tone === 'accent') {
    if (dir === 'up') return 'dl-row is-in';
    if (dir === 'down') return 'dl-row is-out';
  }
  return 'dl-row';
}

export function decisionRowFromRecord(r: DecisionRecord): DecisionRowView {
  const [companyRaw, titleRaw] = splitCompanyJob(r.company_job);
  const company = companyRaw || '（未知公司）';
  const title = titleRaw || '（未记录岗位）';
  const reason = reasonText(r.reason);
  const [oldS, newS] = decisionScores(r.old_score, r.new_score, r.reason);
  const delta = scoreDelta(oldS, newS);
  const tone = decisionTone(r.decision);
  return {
    id: r.id,
    time: fmtDecisionTime(r.created_at),
    kind: decisionLabel(r.decision),
    tone,
    verdict: delta.text,
    verdictTitle: scoreTitle(delta.dir),
    scoreDir: delta.dir,
    reason: `${company} · ${title} · ${reason}`,
    reasonEmpty: reason.startsWith('（未记录'),
    rowClass: decisionRowClass(tone, delta.dir),
    company,
    title,
  };
}

export type DecisionFilter = 'all' | 'reject' | 'accept' | 'other';

export const DECISION_FILTERS: readonly { slug: DecisionFilter; label: string }[] = [
  { slug: 'all', label: '全部' },
  { slug: 'reject', label: '排除' },
  { slug: 'accept', label: '入库' },
  { slug: 'other', label: '其它' },
];

/** 行为筛选按 tone 判定（与 decisionTone 的映射一一对应）。 */
export function decisionFilterAccepts(filter: DecisionFilter, tone: DecisionTone): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'reject':
      return tone === 'bad';
    case 'accept':
      return tone === 'primary';
    case 'other':
      return tone === 'default';
  }
}

export function filterDecisions(
  rows: readonly DecisionRowView[],
  filter: DecisionFilter,
  keyword: string,
): DecisionRowView[] {
  const q = keyword.trim();
  return rows.filter(
    (r) => decisionFilterAccepts(filter, r.tone) && (q === '' || matchesKeyword(r.reason, q)),
  );
}

export function decisionCountLabel(shown: number, total: number, filtered: boolean): string {
  return filtered ? `${shown} / ${total} 条` : `${total} 条`;
}
