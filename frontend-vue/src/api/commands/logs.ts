/**
 * `commands_logs.rs` 的 2 条命令。
 */

import { invokeFlat, invokeNone } from '../invoke';
import type { LogsReply } from '../types';

/**
 * 取 `ts >= since`（Unix 毫秒）的日志，首次 / 手动刷新传 `null`。平铺，`since` 有值才发。
 * 回执的 `now` 是**本次游标**，下次原样带回。后端按**闭区间**比较，游标那一毫秒会重送
 * 一次，由界面多重集去重消化。**不要把 `since` 加一**——同毫秒的后几条会永久丢失。
 */
export function listLogs(since?: number | null): Promise<LogsReply> {
  return invokeFlat<LogsReply>('list_logs', { since: since ?? undefined });
}

/** 清空运行日志缓冲。 */
export function clearLogs(): Promise<void> {
  return invokeNone<void>('clear_logs');
}
