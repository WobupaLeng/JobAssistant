/**
 * `commands_auto.rs` 的 5 条命令（自动投递）。
 *
 * 后端是**无定时器的状态机**：每调一次 `auto_greet_next` 最多发一条，
 * 回执带 `next_at_ms`；**节奏与随机间隔属业务规则，在前端不得自己算**。
 *
 * 载荷形状恰好是一对例外（AGENTS.md C2 明文点名）：
 * `auto_greet_next` 包 `args`，`auto_greet_pause` 平铺——不许统一化。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import type { AutoNextReply, AutoStatusReply } from '../types';

/**
 * 投递下一个岗位：取队首 → 生成话术 → 发送 → 标记已投递。
 *
 * 载荷结构：`{ args: { jobId? } }`。`jobId` **有值才发**；没指定时载荷是
 * `{ args: {} }`（`args` 这一层必须存在，否则命令层报缺 key）。
 *
 * 调用方必须按回执的 `next_at_ms` / `remaining_ms` 安排下一次调用。
 */
export function autoGreetNext(jobId?: string | null): Promise<AutoNextReply> {
  const trimmed = jobId?.trim() ?? '';
  return invokeArgs<AutoNextReply>('auto_greet_next', {
    jobId: trimmed.length > 0 ? trimmed : undefined,
  });
}

/**
 * 暂停自动投递（保留计数与队列）。
 *
 * 平铺：`{ reason? }`——`reason` **有值才发**。注意与 `auto_greet_next`
 * 的形状差异是契约本身，不要「美化」成 `args`。
 */
export function autoGreetPause(reason?: string | null): Promise<boolean> {
  const trimmed = reason?.trim() ?? '';
  return invokeFlat<boolean>('auto_greet_pause', {
    reason: trimmed.length > 0 ? trimmed : undefined,
  });
}

/** 恢复自动投递，返回下一次该调用的时刻（Unix 毫秒）。 */
export function autoGreetResume(): Promise<number> {
  return invokeNone<number>('auto_greet_resume');
}

/** 自动投递运行状态（含 `pending` 队列余量与 `stop_reason`）。 */
export function autoGreetStatus(): Promise<AutoStatusReply> {
  return invokeNone<AutoStatusReply>('auto_greet_status');
}

/** 停止并清零计数（用户点「停止」时用；`pause` 只是暂停）。 */
export function autoGreetReset(): Promise<boolean> {
  return invokeNone<boolean>('auto_greet_reset');
}
