/**
 * 时间戳格式化。纯函数，无副作用。
 */

/**
 * 时间戳 → `MM-DD HH:mm`。
 * 兼容三种输入：秒（Python 时代 `time.time()`）、毫秒（`Date.now()`）、带时区的 ISO 字符串。
 * 小于 1e12 的数值按秒处理——百年内的毫秒值都 ≥ 1e12，这个分界足够稳。
 */
export function fmtTime(v: number | string | null | undefined): string {
  if (v === null || v === undefined || v === '') return '—';
  let ms: number;
  if (typeof v === 'number') {
    ms = v < 1e12 ? v * 1000 : v;
  } else {
    ms = Date.parse(v);
  }
  if (!Number.isFinite(ms)) return String(v);
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
