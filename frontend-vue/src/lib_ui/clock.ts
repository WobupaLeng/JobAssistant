/**
 * 时钟的**唯一**读口：当前毫秒时间戳。其余格式化函数都是纯函数（入参是时间戳），
 * 只有真正需要「现在」的界面代码才调 `nowMs`，这样单测不碰系统时钟。
 */

/** 当前时间的 Unix 毫秒（与 `Date.now()` 同源）。 */
export function nowMs(): number {
  return Date.now();
}
