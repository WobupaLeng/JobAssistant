/**
 * `commands_auto.rs` 的 5 条命令（自动投递）。后端是**无定时器的状态机**，
 * 节奏与随机间隔属业务规则，**前端不得自己算**。
 * 载荷形状是一对例外（AGENTS.md C2）：`auto_greet_next` 包 `args`，`auto_greet_pause` 平铺。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import type { AutoNextReply, AutoStatusReply } from '../types';

/**
 * 投递下一个岗位：取队首 → 生成话术 → 发送 → 标记已投递。载荷 `{ args: { jobId? } }`，
 * `args` 这一层必须存在；调用方按回执的 `next_at_ms` 安排下一次调用。
 */
export function autoGreetNext(jobId?: string | null): Promise<AutoNextReply> {
  const trimmed = jobId?.trim() ?? '';
  return invokeArgs<AutoNextReply>('auto_greet_next', {
    jobId: trimmed.length > 0 ? trimmed : undefined,
  });
}

/**
 * 暂停自动投递（保留计数与队列）。载荷平铺 `{ reason? }`，与 `auto_greet_next`
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
