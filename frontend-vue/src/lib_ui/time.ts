/**
 * 时间戳格式化：**纯函数**，入参是时间戳，不读系统时钟（调用方从 `clock.ts` 取「现在」）。
 * 展示时区固定 `UTC+8`（不依赖本地时区，否则同一条时间戳在不同机器上显示不同结果）；
 * 日历换算自己算（`civilFromDays`，纯整数运算，负数时间戳也不朝零截断）。
 */

/** 展示时区与 UTC 的偏移（秒）。当前只支持 `UTC+8`。 */
export const LOCAL_OFFSET_SECS = 8 * 3600;

/** 日期字段（月/日 1 起算，时/分/秒 0 起算）。 */
export interface DateTime {
  year: number;
  month: number; // 1..12
  day: number; // 1..31
  hour: number;
  minute: number;
  second: number;
}

/**
 * 整数除法（向负无穷取整）——对应 Rust 的 `div_euclid`。
 * `Math.trunc` 会朝零截断，对 1970 年以前的时间戳会算错一天。
 */
function divEuclid(a: number, b: number): number {
  return Math.floor(a / b);
}

/** 取余（结果恒与除数同号）——对应 Rust 的 `rem_euclid`。 */
function remEuclid(a: number, b: number): number {
  const r = a % b;
  return r < 0 ? r + b : r;
}

/** 补零到两位。超两位（不该发生）原样返回，便于一眼看出异常值。 */
export function pad2(n: number): string {
  const s = String(Math.abs(Math.trunc(n)));
  return (n < 0 ? '-' : '') + (s.length >= 2 ? s : '0' + s);
}

/**
 * 毫秒时间戳 → 展示时区下的日历字段。
 *
 * 全程用整除/取余的「欧几里得」版本，负数时间戳也能算对。
 */
export function dateTimeMs(tsMs: number, offsetSecs: number): DateTime {
  const secs = divEuclid(tsMs, 1000) + offsetSecs;
  const days = divEuclid(secs, 86_400);
  const sod = remEuclid(secs, 86_400);
  const [year, month, day] = civilFromDays(days);
  return {
    year,
    month,
    day,
    hour: Math.floor(sod / 3600),
    minute: Math.floor((sod % 3600) / 60),
    second: sod % 60,
  };
}

/**
 * 秒级时间戳 → 毫秒：绝对值小于 1e11（10 位及以下）视为秒。取 1e11 而非 1e12，因为 1e12 会把
 * 1973 年之前的毫秒戳误当秒。
 */
export function toMs(ts: number): number {
  return ts !== 0 && Math.abs(ts) < 100_000_000_000 ? ts * 1000 : ts;
}

/**
 * 时间戳 → `MM-DD HH:mm`，不显示年份。**兼容秒与毫秒**两种输入；
 * `0` 是「没有时间」的占位，不是 1970 年。
 */
export function fmtTime(ts: number): string {
  if (ts === 0) return '—';
  const dt = dateTimeMs(toMs(ts), LOCAL_OFFSET_SECS);
  let out = pad2(dt.month);
  out += '-';
  out += pad2(dt.day);
  out += ' ';
  out += pad2(dt.hour);
  out += ':';
  out += pad2(dt.minute);
  return out;
}

/**
 * 时间戳 → `MM-DD HH:mm:ss`。入参是**毫秒**，但同样容忍秒级输入（与 `fmtTime` 共用
 * `toMs`）；`0` 返回 `--:--:--` 而不是 `01-01 08:00:00`。
 */
export function fmtTimeSecs(tsMs: number): string {
  if (tsMs === 0) return '--:--:--';
  const dt = dateTimeMs(toMs(tsMs), LOCAL_OFFSET_SECS);
  let out = pad2(dt.month);
  out += '-';
  out += pad2(dt.day);
  out += ' ';
  out += pad2(dt.hour);
  out += ':';
  out += pad2(dt.minute);
  out += ':';
  out += pad2(dt.second);
  return out;
}

/**
 * 1970-01-01 起的天数 → `[年, 月, 日]`。Howard Hinnant 的 `civil_from_days`，
 * 纯整数运算，对任何天数都成立，不查表、不依赖时区库。
 */
export function civilFromDays(z0: number): [number, number, number] {
  const z = z0 + 719_468;
  const era = divEuclid(z, 146_097);
  const doe = remEuclid(z, 146_097); // [0, 146096]
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146_096)) / 365); // [0, 399]
  const y = yoe + era * 400;
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100)); // [0, 365]
  const mp = Math.floor((5 * doy + 2) / 153); // [0, 11]，3 月为 0
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1; // [1, 31]
  const m = mp < 10 ? mp + 3 : mp - 9; // [1, 12]
  return [m <= 2 ? y + 1 : y, m, d];
}
