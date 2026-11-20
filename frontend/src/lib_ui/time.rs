//! 时间戳格式化：**纯函数**，入参是时间戳，不读系统时钟。
//!
//! 不用 chrono/time 之类的 crate：前端只需要把毫秒转成 `MM-DD HH:mm[:ss]`，
//! 引一个大依赖只为了两位数补零不划算。日历换算自己算（见 [`civil_from_days`]），
//! 原因有两个：
//!
//! 1. **可测**：宿主目标上 `js_sys::Date` 是 wasm 桩，一调就不可展开 panic
//!    （`0xc0000409`），整条 `cargo test` 会 abort。纯函数化后单测直接跑在 host 上。
//! 2. **时区确定**：`js_sys::Date` 用的是**浏览器本地时区**，同一条时间戳在不同
//!    机器的单测里结果不同；这里统一按 `UTC+8`（本应用唯一面对的本地时区）。
//!
//! 「现在」从哪来：调用方去 [`crate::lib_ui::clock::now_ms`] 取，再传进来。

/// 展示时区与 UTC 的偏移（秒）。当前只支持 `UTC+8`。
pub const LOCAL_OFFSET_SECS: i64 = 8 * 3600;

/// 日期字段（月/日 1 起算，时/分/秒 0 起算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// 毫秒时间戳 → 展示时区下的日历字段。
///
/// 负数（1970 年以前）也能算对：这里全程用 `div_euclid` / `rem_euclid`，
/// 不会像 `/` 与 `%` 那样朝零截断。
pub fn date_time_ms(ts_ms: i64, offset_secs: i64) -> DateTime {
    let secs = ts_ms.div_euclid(1000) + offset_secs;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    DateTime {
        year,
        month,
        day,
        hour: (sod / 3600) as u32,
        minute: ((sod % 3600) / 60) as u32,
        second: (sod % 60) as u32,
    }
}

/// 毫秒时间戳 → `MM-DD HH:mm`（方案 B：不显示年份，界面里所有时间同款）。
///
/// **兼容两种输入**：秒（Python 时代 `time.time()`）与毫秒（`Date.now()`）。
/// 小于 1e12 的数值按秒处理——百年内的毫秒值都 ≥ 1e12，这个分界足够稳。
/// `0` 是「没有时间」的占位（后端未写该列），不是 1970 年。
pub fn fmt_time(ts: i64) -> String {
    if ts == 0 {
        return "—".to_string();
    }
    let dt = date_time_ms(to_ms(ts), LOCAL_OFFSET_SECS);
    format!("{:02}-{:02} {:02}:{:02}", dt.month, dt.day, dt.hour, dt.minute)
}

/// 毫秒时间戳 → `MM-DD HH:mm:ss`（运行日志要秒，与旧版 `web/` 日志框一致）。
///
/// 入参单位是**毫秒**（`LogRow.ts` 就是毫秒）。`0` 仍是占位，返回 `--:--:--`
/// 而不是 `01-01 08:00:00`。
pub fn fmt_time_secs(ts_ms: u64) -> String {
    if ts_ms == 0 {
        return "--:--:--".to_string();
    }
    let dt = date_time_ms(ts_ms.min(i64::MAX as u64) as i64, LOCAL_OFFSET_SECS);
    format!(
        "{:02}-{:02} {:02}:{:02}:{:02}",
        dt.month, dt.day, dt.hour, dt.minute, dt.second
    )
}

/// 秒级时间戳 → 毫秒，用于需要精确到秒的展示。
///
/// 与 [`fmt_time`] 的「< 1e12 视为秒」判据保持一致；已经是毫秒的原样返回。
pub fn to_ms(ts: i64) -> i64 {
    if ts != 0 && ts.abs() < 1_000_000_000_000 {
        ts * 1000
    } else {
        ts
    }
}

/// 1970-01-01 起的天数 → `(年, 月, 日)`。Howard Hinnant 的 `civil_from_days`，
/// 纯整数运算，对任何 i64 天数都成立，不查表、不依赖时区库。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]，3 月为 0
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_placeholder() {
        assert_eq!(fmt_time(0), "—");
        assert_eq!(fmt_time_secs(0), "--:--:--");
    }

    #[test]
    fn seconds_to_calendar_fields() {
        // 1970-01-01 00:00 UTC -> 08:00 in the +08:00 display zone
        let dt = date_time_ms(0, LOCAL_OFFSET_SECS);
        assert_eq!((dt.year, dt.month, dt.day, dt.hour), (1970, 1, 1, 8));
        // 1_700_000_000.123 s = 2023-11-14 22:13:20 UTC -> 11-15 06:13:20 +08:00
        let dt = date_time_ms(1_700_000_000_123, LOCAL_OFFSET_SECS);
        assert_eq!((dt.year, dt.month, dt.day), (2023, 11, 15));
        assert_eq!((dt.hour, dt.minute, dt.second), (6, 13, 20));
    }

    #[test]
    fn leap_day_in_century_and_normal_year() {
        // 2000-02-29 00:00 +08:00 (century leap year)
        let dt = date_time_ms(951_753_600_000, LOCAL_OFFSET_SECS);
        assert_eq!((dt.year, dt.month, dt.day), (2000, 2, 29));
        assert_eq!(dt.hour, 0);
        // 2024-02-29 00:00 +08:00 (ordinary leap year)
        let dt = date_time_ms(1_709_136_000_000, LOCAL_OFFSET_SECS);
        assert_eq!((dt.year, dt.month, dt.day), (2024, 2, 29));
        assert_eq!(dt.hour, 0);
        // 2100 is NOT a leap year: 02-28 is followed directly by 03-01
        let a = date_time_ms(4_107_427_200_000, LOCAL_OFFSET_SECS); // 2100-02-28 00:00 +08:00
        let b = date_time_ms(4_107_513_600_000, LOCAL_OFFSET_SECS); // 2100-03-01 00:00 +08:00
        assert_eq!((a.year, a.month, a.day), (2100, 2, 28));
        assert_eq!((b.year, b.month, b.day), (2100, 3, 1));
    }

    #[test]
    fn seconds_and_millis_are_interchangeable() {
        // same instant given as seconds or millis must format identically
        assert_eq!(fmt_time(1_700_000_000), fmt_time(1_700_000_000_000));
        assert_eq!(to_ms(1_700_000_000), 1_700_000_000_000);
        assert_eq!(to_ms(1_700_000_000_000), 1_700_000_000_000);
        assert_eq!(to_ms(0), 0);
    }

    #[test]
    fn negative_timestamps_do_not_truncate_toward_zero() {
        // 1969-12-31 23:59:59 UTC is 1970-01-01 07:59:59 in +08:00
        let dt = date_time_ms(-1_000, LOCAL_OFFSET_SECS);
        assert_eq!((dt.year, dt.month, dt.day), (1970, 1, 1));
        assert_eq!((dt.hour, dt.minute, dt.second), (7, 59, 59));
    }

    #[test]
    fn shape_is_zero_padded() {
        let s = fmt_time_secs(1_700_000_059_000);
        assert_eq!(s.len(), 14, "MM-DD HH:mm:ss, got {s}");
        assert_eq!(s.chars().nth(5), Some(' '));
        assert_eq!(s.chars().filter(|c| *c == ':').count(), 2);
        assert!(s.ends_with(":19"), "got {s}");
        assert!(s.chars().all(|c| c.is_ascii() || c == ' '), "got {s}");
    }
}
