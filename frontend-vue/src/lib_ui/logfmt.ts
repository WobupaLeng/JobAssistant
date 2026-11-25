/**
 * 日志页的**纯**格式化与过滤：级别归一、时间戳、分数变化箭头、公司岗位拆分。
 * 全部是无副作用字符串变换，入参都是裸类型，**不依赖 api / 框架**（`lib_ui` 铁律）。
 */

import { fmtTime, fmtTimeSecs } from './time';

/** 运行日志的级别。`info` 同时承载「后端给了不认识的级别」。 */
export type LogLevel = 'info' | 'warn' | 'error';

/**
 * 大小写不敏感归一。后端 `LogRing` 只写 `info` / `warn` / `error`，
 * 但 sidecar 转发进来的文本可能带空格或大写，这里一并吃掉。
 * **未知级别一律按 info**：宁可显示成普通行，也不要整行消失。
 */
export function parseLogLevel(raw: string): LogLevel {
  switch (raw.trim().toLowerCase()) {
    case 'warn':
    case 'warning':
      return 'warn';
    case 'error':
    case 'err':
    case 'fatal':
      return 'error';
    default:
      return 'info';
  }
}

/** 固定渲染宽度，等宽字体下三行前缀对齐，扫读时不会跳。 */
export function levelLabel(level: LogLevel): string {
  switch (level) {
    case 'info':
      return 'INFO ';
    case 'warn':
      return 'WARN ';
    case 'error':
      return 'ERROR';
  }
}

/** 给可访问性用的中文名（`aria-label` / 筛选胶囊文案）。 */
export function levelCn(level: LogLevel): string {
  switch (level) {
    case 'info':
      return '信息';
    case 'warn':
      return '警告';
    case 'error':
      return '错误';
  }
}

/** 筛选顺序：错误在前——用户打开日志页通常就是来找错误的。 */
export function allLevels(): LogLevel[] {
  return ['error', 'warn', 'info'];
}

/**
 * 运行日志时间：`MM-DD HH:mm:ss`（与旧版 `web/` 日志框一致，带秒）。
 *
 * 后端 `LogRow.ts` 是**毫秒**。纯函数：不读系统时钟，单测环境同结果。
 */
export function fmtLogTime(ts: number): string {
  return fmtTimeSecs(ts);
}

/** 决策记录时间：`created_at` 通常是 **Unix 秒**，展示成 `MM-DD HH:mm`。 */
export function fmtDecisionTime(createdAt: number): string {
  return fmtTime(createdAt);
}

/** 分数变化方向。 */
export type ScoreDir = 'up' | 'down' | 'same' | 'none';

/**
 * 分数变化的渲染数据。`null` 表示「这条记录没有分数」，界面渲染成 `—`，
 * **绝不**显示 `null` / `undefined` 或假的 `0 → 0`。
 */
export interface ScoreDelta {
  old: number | null;
  new: number | null;
  /** 文案，如 `72 → 65`、`→ 65`、`72 →` */
  text: string;
  dir: ScoreDir;
}

/**
 * 由两端分数组装展示数据。`rescore` 只写 reason、不写这两列，所以残缺是常态：
 * 两端都有 → 比较方向；只有新分 → `→ 65`；只有旧分 → `72 →`。
 */
export function scoreDelta(old: number | null, next: number | null): ScoreDelta {
  let text: string;
  let dir: ScoreDir;
  if (old !== null && next !== null) {
    text = `${old} → ${next}`;
    dir = old === next ? 'same' : next > old ? 'up' : 'down';
  } else if (old === null && next !== null) {
    text = `→ ${next}`;
    dir = 'none';
  } else if (old !== null && next === null) {
    text = `${old} →`;
    dir = 'none';
  } else {
    text = '—';
    dir = 'none';
  }
  return { old, new: next, text, dir };
}

/**
 * 决策记录的两端分数：优先用 `old_score` / `new_score`，两列都空时才从 `reason`
 * 里抠「旧 → 新」；抠不出来返回 `[null, null]`，界面显示 `—`，不猜。
 */
export function decisionScores(
  old: number | null,
  next: number | null,
  reason: string,
): [number | null, number | null] {
  if (old !== null || next !== null) return [old, next];
  return parseReasonScores(reason) ?? [null, null];
}

/**
 * 从 `reason` 文本里找 `a → b` / `a→b` / `→ b` 形式的分数对，只认 ASCII 数字。
 * 导出仅为便于单测。
 */
export function parseReasonScores(reason: string): [number | null, number | null] | null {
  const arrowAt = reason.indexOf('→');
  if (arrowAt < 0) return null;
  const left = reason.slice(0, arrowAt);
  const right = reason.slice(arrowAt + '→'.length);
  const before = parseTrailingU8(left);
  const after = parseLeadingU8(right);
  if (before === null && after === null) return null;
  return [before, after];
}

/** 从尾巴读完一个 u8：数字多于 3 位或 >255 视为无效（返回 `null`，不猜 0）。 */
export function parseTrailingU8(s: string): number | null {
  const trimmed = s.replace(/\s+$/, '');
  const m = /[0-9]+$/.exec(trimmed);
  if (!m) return null;
  const digits = m[0];
  if (digits.length > 3) return null;
  const v = Number(digits);
  return v <= 255 ? v : null;
}

/**
 * 读箭头后的分数：右端常带单位前缀（历史行写成「百分制 85」），所以不能取第一个
 * 数字了事，而是逐个数字串试，第一个 1~3 位且 ≤255 的才算。
 */
export function parseLeadingU8(s: string): number | null {
  let rest = s.replace(/^\s+/, '');
  const re = /[0-9]+/g;
  for (;;) {
    if (rest.length === 0) return null;
    re.lastIndex = 0;
    const m = re.exec(rest);
    if (!m || m.index === undefined) return null;
    const digits = m[0];
    if (digits.length <= 3) {
      const v = Number(digits);
      if (v <= 255) return v;
    }
    // 这一串太长（不是分数）：跳过它继续找下一个数字串
    rest = rest.slice(m.index + digits.length);
  }
}

/**
 * `company_job` 列写成 `公司/岗位`。公司名本身可能带斜杠，所以**按最后一个**斜杠切；
 * 没有斜杠时整串当岗位名、公司留空。
 */
export function splitCompanyJob(raw: string): [string, string] {
  const s = raw.trim();
  if (s.length === 0) return ['', ''];
  const at = s.lastIndexOf('/');
  if (at < 0) return ['', s];
  return [s.slice(0, at).trim(), s.slice(at + 1).trim()];
}

/** 级别集合过滤：空集合 = 不过滤（全取消勾选时也按不过滤处理，否则一片空白）。 */
export function matchesLevelAny(level: LogLevel, selected: readonly LogLevel[]): boolean {
  return selected.length === 0 || selected.includes(level);
}

/** 关键字匹配（大小写不敏感，空关键字恒为真）。 */
export function matchesKeyword(text: string, keyword: string): boolean {
  const q = keyword.trim().toLowerCase();
  return q.length === 0 || text.toLowerCase().includes(q);
}

/**
 * 一条运行日志是否可见（级别 + 关键字）。关键字同时匹配消息与级别名，
 * 这样搜「error」既能命中消息里带 error 的行，也能命中错误级别的行。
 */
export function logVisible(
  level: LogLevel,
  message: string,
  keyword: string,
  selected: readonly LogLevel[],
): boolean {
  if (!matchesLevelAny(level, selected)) return false;
  const q = keyword.trim();
  if (q.length === 0) return true;
  return matchesKeyword(message, q) || matchesKeyword(level, q) || matchesKeyword(levelCn(level), q);
}

/**
 * 决策行为的展示文案。后端 `decision` 列取值见 `rescore.rs:152`（`rescore`）
 * 与 `sqlite/tests.rs`（`reject` / `accept`）；未知值原样显示，不要吞掉。
 */
export function decisionLabel(decision: string): string {
  switch (decision.trim()) {
    case '':
      return '未标注';
    case 'reject':
      return '排除';
    case 'accept':
      return '入库';
    case 'store':
    case 'stored':
      return '入库';
    case 'rescore':
      return '重算';
    default:
      return decision.trim();
  }
}

/** 决策行为的色调：`bad`（排除）/ `primary`（入库）/ `accent`（重算）/ `default`。 */
export function decisionTone(decision: string): 'bad' | 'primary' | 'accent' | 'default' {
  switch (decision.trim()) {
    case 'reject':
      return 'bad';
    case 'accept':
    case 'store':
    case 'stored':
      return 'primary';
    case 'rescore':
      return 'accent';
    default:
      return 'default';
  }
}

/**
 * 「为什么被排除 / 入库」的兜底文案。`reason` 为空时**不显示空白格**，
 * 给一句能说明「这条没记原因」的话，而不是让用户以为界面坏了。
 */
export function reasonText(reason: string): string {
  const r = reason.trim();
  return r.length === 0 ? '（未记录原因）' : r;
}
