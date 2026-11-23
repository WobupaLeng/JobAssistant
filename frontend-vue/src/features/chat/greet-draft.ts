/**
 * 打招呼意图的跨页暂存（M1 只负责"存"，M2 负责"取"）。
 *
 * 流程：列表里点卡片上的「打招呼」→ 切到 Boss 消息页 → 选中公司前缀匹配的
 * 好友 → 把草稿塞进输入框。第二步是 M2 的事，本轮只把意图安全地递过去。
 *
 * 为什么不用 localStorage：这是一次性的导航意图，刷新后就该消失；
 * 落盘会让用户下次打开应用看见一句莫名其妙的草稿。所以放内存。
 */

export interface GreetDraft {
  readonly company: string;
  readonly text: string;
  /** 生成时间（毫秒）。用于丢弃过期的意图。 */
  readonly atMs: number;
}

/** 意图有效期：超过 10 分钟视为用户已经放弃。 */
export const GREET_DRAFT_TTL_MS = 10 * 60 * 1000;

let current: GreetDraft | null = null;

/** 写入意图。重复点不同岗位会覆盖——只保留最后一次。 */
export function setGreetDraft(company: string, text: string, nowMs = now()): void {
  current = {
    company: company.trim(),
    text,
    atMs: nowMs,
  };
}

/** 取出并清空。取不到（没有 / 已过期）返回 `null`。 */
export function takeGreetDraft(nowMs = now()): GreetDraft | null {
  const d = current;
  current = null;
  if (!d) return null;
  if (nowMs - d.atMs > GREET_DRAFT_TTL_MS) return null;
  return d;
}

/** 只读窥视，不清空。给"消息页渲染草稿"用，避免重复渲染时意图被吃掉。 */
export function peekGreetDraft(nowMs = now()): GreetDraft | null {
  if (!current) return null;
  if (nowMs - current.atMs > GREET_DRAFT_TTL_MS) return null;
  return current;
}

/** 清空（用户取消发送、或消息页已经消费完）。 */
export function clearGreetDraft(): void {
  current = null;
}

/** 单测用：重置模块状态。正式路径不要调。 */
export function resetGreetDraftForTest(): void {
  current = null;
}

function now(): number {
  return Date.now();
}
