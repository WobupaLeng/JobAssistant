/**
 * 错误条的**队列**语义。
 *
 * 为什么不是单条：单条 ErrorBar 会被相邻错误互相覆盖，
 * 提交 53d05cb 之前），每次失败都会覆盖 `message`，后到的失败把前一条挤掉——用户只看到最后一条，
 * 前面那些（通常是同一批并发里的另一条命令）**静默丢失**。这里改成队列：
 *
 * - FIFO：先发生的先显示，用户按顺序读；
 * - 可逐条关闭：关掉第一条，第二条立刻顶上，不丢后面的；
 * - 同一条消息重复出现只保留一份，但要刷新 `atMs`（重连失败会连刷十几次）；
 * - 队列有上限，超了丢**最旧**的，并且**必须让用户看见丢了几条**——
 *   静默截断等同于新的静默失效。
 *
 * 纯函数，不碰 DOM；`aria-live` 与渲染在组件里。
 */

/** 队列上限。超过就丢最旧的，并统计丢弃条数。 */
export const ERROR_QUEUE_MAX = 5;

export interface ErrorItem {
  /** 稳定 key：渲染 `v-for` 与测试用。 */
  readonly id: number;
  readonly message: string;
  /** 触发它的命令名（原型 `app-error-code` 里显示）。 */
  readonly code: string;
  /** 入队时刻（`Date.now()`），同消息去重时用来刷新。 */
  readonly atMs: number;
  /** 重试动作的标签；`null` 表示这条不提供重试。 */
  readonly retryLabel: string | null;
}

export interface ErrorQueue {
  readonly items: readonly ErrorItem[];
  /** 自上次清空以来被挤掉的条数，用于顶部那句「另有 N 条更早的错误」。 */
  readonly dropped: number;
  /** 下一个分配的自增 id。 */
  readonly nextId: number;
}

export const EMPTY_QUEUE: ErrorQueue = { items: [], dropped: 0, nextId: 1 };

/** 入队。同 `(message, code)` 已存在则只刷新时间，不新增、不重排。 */
export function pushError(
  q: ErrorQueue,
  message: string,
  code: string,
  atMs: number,
  retryLabel: string | null = null,
): ErrorQueue {
  const msg = message.trim();
  if (msg === '') return q;
  const dup = q.items.findIndex((it) => it.message === msg && it.code === code);
  if (dup >= 0) {
    const items = q.items.map((it, i) => (i === dup ? { ...it, atMs } : it));
    return { ...q, items };
  }
  const item: ErrorItem = { id: q.nextId, message: msg, code, atMs, retryLabel };
  const all = [...q.items, item];
  // 超上限：从头丢（最旧的先走），dropped 累加，让用户知道"前面还有"。
  const overflow = Math.max(0, all.length - ERROR_QUEUE_MAX);
  return {
    items: overflow > 0 ? all.slice(overflow) : all,
    dropped: q.dropped + overflow,
    nextId: q.nextId + 1,
  };
}

/** 关掉一条（按 id）。队列空时 `dropped` 归零——下次错误从干净状态开始。 */
export function dismissError(q: ErrorQueue, id: number): ErrorQueue {
  const items = q.items.filter((it) => it.id !== id);
  if (items.length === q.items.length) return q;
  return { ...q, items, dropped: items.length === 0 ? 0 : q.dropped };
}

/** 全部关掉。 */
export function clearErrors(q: ErrorQueue): ErrorQueue {
  return q.items.length === 0 && q.dropped === 0 ? q : { ...EMPTY_QUEUE, nextId: q.nextId };
}

/** 当前展示的最前一条（原型里错误条只显示一行；多出来的折叠成计数）。 */
export function firstError(q: ErrorQueue): ErrorItem | null {
  return q.items.length > 0 ? q.items[0] : null;
}

/** 「另有 N 条」的计数：队列里除第一条之外的数量 + 被挤掉的条数。 */
export function pendingCount(q: ErrorQueue): number {
  return Math.max(0, q.items.length - 1) + q.dropped;
}

/** 提示文案：`另有 2 条更早的错误` / 空串（不渲染）。 */
export function pendingText(q: ErrorQueue): string {
  const n = pendingCount(q);
  return n > 0 ? `另有 ${n} 条更早的错误` : '';
}

/** sr-only 播报文本：`aria-live` 用它，屏读用户也能听到新错误。 */
export function announceText(q: ErrorQueue): string {
  if (q.items.length === 0) return '';
  const head = q.items[0];
  const more = pendingCount(q);
  return more > 0 ? `错误：${head.message}。${pendingText(q)}` : `错误：${head.message}`;
}

/** 是否还有可展示的错误。 */
export function hasError(q: ErrorQueue): boolean {
  return q.items.length > 0;
}

/** 测试与首帧用：把一串错误一次性塞进去。 */
export function seedErrors(
  messages: readonly string[],
  code: string,
  atMs: number,
): ErrorQueue {
  let q = EMPTY_QUEUE;
  for (const m of messages) q = pushError(q, m, code, atMs);
  return q;
}
