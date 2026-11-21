//! 日志页的**纯**格式化与过滤：级别归一、时间戳、分数变化箭头、公司岗位拆分。
//!
//! 为什么单独成模块而不是塞进 `features/logs/state.rs`：这里全部是无副作用
//! 字符串变换，`features` 层只负责「把信号喂进来、把结果挂到 view 上」。
//! 放在 `lib_ui` 还能在宿主目标（`cargo test`）上直接单测——
//! `features` 里但凡碰到 `js_sys::Date` 的写法在宿主上无法运行。
//!
//! **不依赖 `api`**（`lib_ui` 铁律，`tools/check_layers.py` R3）：所有入参都是
//! 裸类型（`u64` / `Option<u8>` / `&str`），不引用 `api::types::*`。

use crate::lib_ui::time::{fmt_time, fmt_time_secs};

/// 运行日志的级别。`Info` 同时承载「后端给了不认识的级别」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// 大小写不敏感归一。后端 `LogRing` 只写 `info` / `warn` / `error`，
    /// 但 sidecar 转发进来的文本可能带空格或大写，这里一并吃掉。
    /// **未知级别一律按 info**：宁可显示成普通行，也不要整行消失。
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "warn" | "warning" => LogLevel::Warn,
            "error" | "err" | "fatal" => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }

    /// 固定渲染宽度，等宽字体下三行前缀对齐，扫读时不会跳。
    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    /// 给可访问性用的中文名（`aria-label` / 筛选胶囊文案）。
    pub fn cn(self) -> &'static str {
        match self {
            LogLevel::Info => "信息",
            LogLevel::Warn => "警告",
            LogLevel::Error => "错误",
        }
    }

    /// 筛选顺序：错误在前——用户打开日志页通常就是来找错误的。
    pub fn all() -> [LogLevel; 3] {
        [LogLevel::Error, LogLevel::Warn, LogLevel::Info]
    }
}

/// 运行日志时间：`MM-DD HH:mm:ss`（与旧版 `web/` 日志框一致，带秒）。
///
/// 后端 `LogRow.ts` 是**毫秒**。纯函数：不读系统时钟，宿主目标上也能单测。
pub fn fmt_log_time(ts_ms: u64) -> String {
    fmt_time_secs(ts_ms)
}

/// 决策记录时间：`created_at` 是 **Unix 秒**，展示成 `MM-DD HH:mm`。
pub fn fmt_decision_time(created_at: i64) -> String {
    fmt_time(created_at)
}

/// 分数变化的渲染数据。`None` 表示「这条记录没有分数」，界面渲染成 `—`，
/// **绝不**显示 `null` / `undefined` 或假的 `0 → 0`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreDelta {
    pub old: Option<u8>,
    pub new: Option<u8>,
    /// 文案，如 `72 → 65`、`→ 65`、`72 →`
    pub text: String,
    /// `up` / `down` / `same` / `none`
    pub dir: &'static str,
}

impl ScoreDelta {
    /// 有没有真的「变了」——决定要不要画箭头。两端齐且相等时为 false。
    pub fn changed(&self) -> bool {
        self.dir == "up" || self.dir == "down"
    }
}

/// 由 `old_score` / `new_score` 组装展示数据。
///
/// 三种残缺形态都要处理（`rescore` 目前**只写 reason 文本、不写这两列**，
/// 所以 `(None, None)` 是常态而不是异常）：
/// - 两端都有 → 比较大小给出方向；相等时不算变化
/// - 只有新分 → `→ 65`（首次打分）
/// - 只有旧分 → `72 →`（重算后被清空 / 越界按 None 处理）
pub fn score_delta(old: Option<u8>, new: Option<u8>) -> ScoreDelta {
    let (text, dir) = match (old, new) {
        (Some(o), Some(n)) if o == n => (format!("{o} → {n}"), "same"),
        (Some(o), Some(n)) if n > o => (format!("{o} → {n}"), "up"),
        (Some(o), Some(n)) => (format!("{o} → {n}"), "down"),
        (None, Some(n)) => (format!("→ {n}"), "none"),
        (Some(o), None) => (format!("{o} →"), "none"),
        (None, None) => ("—".to_string(), "none"),
    };
    ScoreDelta { old, new, text, dir }
}

/// 决策记录的两端分数：优先用 `old_score` / `new_score` 列；
/// 两列都空时尝试从 `reason` 里抠出 `旧 → 新`（历史行由 `rescore.rs:153-159`
/// 写成 `八分制 X → 百分制 N（source）原因`）。
///
/// 抠不出来就返回 `(None, None)` —— 界面显示 `—`，不猜。
pub fn decision_scores(old: Option<u8>, new: Option<u8>, reason: &str) -> (Option<u8>, Option<u8>) {
    if old.is_some() || new.is_some() {
        return (old, new);
    }
    parse_reason_scores(reason).unwrap_or((None, None))
}

/// 从 `reason` 文本里找形如 `a → b` / `a→b` / `→ b` 的分数对。
///
/// 只认 ASCII 数字 + 可选空白 + 箭头；其它字符原样跳过。
fn parse_reason_scores(reason: &str) -> Option<(Option<u8>, Option<u8>)> {
    // 用 `split_at` 拿 &str 而不是 `Vec<char>` 切下标：后面要按「字符边界」
    // 安全地取子串，`&str` 的切片天然就是字节正确的。
    let arrow_at = reason.char_indices().find(|(_, c)| *c == '→')?.0;
    let arrow_len = '→'.len_utf8();
    let (left, right_with_arrow) = reason.split_at(arrow_at);
    let right = &right_with_arrow[arrow_len..];
    let before = parse_trailing_u8(left);
    let after = parse_leading_u8(right);
    if before.is_none() && after.is_none() {
        return None;
    }
    Some((before, after))
}

/// 从右往左跳过空白，读一个连续数字。数字多于 3 位或 >255 视为无效。
///
/// `rescore.rs:173` 的 `old_label` 在没有旧分时写的是 `"无分"`，此时左端
/// 解析失败，返回 `None`——正是「没有旧分」该有的语义，不猜 0。
fn parse_trailing_u8(s: &str) -> Option<u8> {
    let trimmed = s.trim_end();
    let digits_start = trimmed
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, _)| i)?;
    let digits = &trimmed[digits_start..];
    if digits.len() > 3 {
        return None;
    }
    digits.parse::<u8>().ok()
}

/// 从左往右读箭头后的分数。
///
/// 与左端不对称：左端紧贴箭头（`"8 →"`），右端**常带单位前缀**：
/// - `rescore.rs:153-159` 现行写法是 `"8 → 85（rule）薪资偏低"`（数字紧跟箭头）；
/// - 旧制迁移期的历史行写成 `"8 → 百分制 85"`（箭头与数字之间夹了三个汉字）。
///
/// 所以这里不「取第一个数字」就收工，而是跳过空白与「看似单位的词」：
/// 连续汉字 + 空白 + 数字才算单位的写法。判据是「空白之前有非 ASCII 字符」——
/// 这样 `"八分制 8 → 百分制 85"` 取 85（不是 8），而 `"7→70"` 仍取 70。
fn parse_leading_u8(s: &str) -> Option<u8> {
    // 先跳过开头的空白与非数字，逐个词试：第一个能构成 1~3 位数字的就用它。
    let mut rest = s.trim_start();
    loop {
        if rest.is_empty() {
            return None;
        }
        let digit_start = rest.char_indices().find(|(_, c)| c.is_ascii_digit()).map(|(i, _)| i);
        let start = digit_start?;
        let digits: String = rest[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() <= 3 {
            if let Ok(v) = digits.parse::<u8>() {
                return Some(v);
            }
        }
        // 这一串太长（不是分数）：跳过它继续找下一个数字串
        rest = &rest[start + digits.len()..];
    }
}

/// `company_job` 列写成 `公司/岗位`（`rescore.rs:147-151`、`jobs.rs:176`）。
///
/// 公司名本身可能带斜杠（「A/B 事业部」），所以**按最后一个**斜杠切；
/// 没有斜杠时整串当岗位名，公司留空（界面渲染成 `（未知公司）`）。
pub fn split_company_job(raw: &str) -> (String, String) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (String::new(), String::new());
    }
    match raw.rsplit_once('/') {
        Some((company, title)) => (company.trim().to_string(), title.trim().to_string()),
        None => (String::new(), raw.to_string()),
    }
}

/// 运行日志的关键字匹配（大小写不敏感，空关键字恒为真）。
pub fn matches_level_any(level: LogLevel, selected: &[LogLevel]) -> bool {
    // 空集合 = 不过滤。用户把三档全部取消勾选时也按「不过滤」处理，
    // 否则会出现「明明有日志却一片空白」——这正是筛选器最容易让人困惑的地方。
    selected.is_empty() || selected.contains(&level)
}

pub fn matches_keyword(text: &str, keyword: &str) -> bool {
    let q = keyword.trim().to_lowercase();
    q.is_empty() || text.to_lowercase().contains(&q)
}

/// 一条运行日志是否可见（级别 + 关键字）。关键字同时匹配消息与级别名，
/// 这样搜「error」既能命中消息里带 error 的行，也能命中错误级别的行。
pub fn log_visible(level: LogLevel, message: &str, keyword: &str, selected: &[LogLevel]) -> bool {
    if !matches_level_any(level, selected) {
        return false;
    }
    let q = keyword.trim();
    if q.is_empty() {
        return true;
    }
    matches_keyword(message, q)
        || matches_keyword(level.as_str(), q)
        || matches_keyword(level.cn(), q)
}

/// 决策行为的展示文案。后端 `decision` 列取值见 `rescore.rs:152`（`rescore`）
/// 与 `sqlite/tests.rs`（`reject` / `accept`）；未知值原样显示，不要吞掉。
pub fn decision_label(decision: &str) -> String {
    match decision.trim() {
        "" => "未标注".to_string(),
        "reject" => "排除".to_string(),
        "accept" => "入库".to_string(),
        "store" | "stored" => "入库".to_string(),
        "rescore" => "重算".to_string(),
        other => other.to_string(),
    }
}

/// 决策行为的色调：`bad`（排除）/ `primary`（入库）/ `accent`（重算）/ `default`。
pub fn decision_tone(decision: &str) -> &'static str {
    match decision.trim() {
        "reject" => "bad",
        "accept" | "store" | "stored" => "primary",
        "rescore" => "accent",
        _ => "default",
    }
}

/// 「为什么被排除 / 入库」的兜底文案。`reason` 为空时**不显示空白格**，
/// 给一句能说明「这条没记原因」的话，而不是让用户以为界面坏了。
pub fn reason_text(reason: &str) -> String {
    let r = reason.trim();
    if r.is_empty() {
        "（未记录原因）".to_string()
    } else {
        r.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_parse_is_case_and_space_tolerant() {
        assert_eq!(LogLevel::parse("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::parse(" WARNING "), LogLevel::Warn);
        assert_eq!(LogLevel::parse("Error"), LogLevel::Error);
        // 未知级别不能整行消失，降级成 info
        assert_eq!(LogLevel::parse("debug"), LogLevel::Info);
        assert_eq!(LogLevel::parse(""), LogLevel::Info);
        assert_eq!(LogLevel::all().len(), 3);
    }

    #[test]
    fn score_delta_covers_all_four_shapes() {
        let up = score_delta(Some(60), Some(85));
        assert_eq!(up.text, "60 → 85");
        assert_eq!(up.dir, "up");
        assert!(up.changed());

        let down = score_delta(Some(85), Some(60));
        assert_eq!(down.dir, "down");
        assert!(down.changed());

        let same = score_delta(Some(70), Some(70));
        assert_eq!(same.dir, "same");
        assert!(!same.changed(), "同分不是变化，不画箭头");

        // rescore 目前两列都空：显示破折号，绝不显示 null
        let none = score_delta(None, None);
        assert_eq!(none.text, "—");
        assert!(!none.text.contains("null"));
        assert!(!none.changed());

        assert_eq!(score_delta(None, Some(72)).text, "→ 72");
        assert_eq!(score_delta(Some(72), None).text, "72 →");
    }

    #[test]
    fn decision_scores_fall_back_to_reason_text() {
        // 列里有值：直接用列
        assert_eq!(decision_scores(Some(8), Some(85), "8 → 85（rule）x"), (Some(8), Some(85)));
        // 列全空（rescore 现状）：从 reason 抠
        assert_eq!(
            decision_scores(None, None, "八分制 8 → 百分制 85（rule）薪资偏低"),
            (Some(8), Some(85))
        );
        assert_eq!(decision_scores(None, None, "7→70"), (Some(7), Some(70)));
        // 抠不出来：如实给 None，不猜 0
        assert_eq!(decision_scores(None, None, "外包公司"), (None, None));
        assert_eq!(decision_scores(None, None, ""), (None, None));
        assert_eq!(score_delta_ui(None, None, "外包公司").text, "—");
    }

    /// 测试里的小助手：与界面上的调用序列一致。
    fn score_delta_ui(old: Option<u8>, new: Option<u8>, reason: &str) -> ScoreDelta {
        let (o, n) = decision_scores(old, new, reason);
        score_delta(o, n)
    }

    #[test]
    fn split_company_job_uses_last_slash() {
        assert_eq!(
            split_company_job("腾讯/高级 Java"),
            ("腾讯".to_string(), "高级 Java".to_string())
        );
        // 公司名里带斜杠：按最后一个切，岗位名不能被吃掉
        assert_eq!(
            split_company_job("A/B 事业部/后端"),
            ("A/B 事业部".to_string(), "后端".to_string())
        );
        assert_eq!(split_company_job("只有岗位"), (String::new(), "只有岗位".to_string()));
        assert_eq!(split_company_job("  "), (String::new(), String::new()));
    }

    #[test]
    fn log_time_is_mm_dd_hh_mm_ss() {
        // 0 是占位，不是 1970 年
        assert_eq!(fmt_log_time(0), "--:--:--");
        // 正数是纯函数的直接断言：不读系统时钟，宿主目标与 wasm 目标同结果。
        // 形状用 ASCII 断言（长度 / 分隔符 / 秒位），具体日期断言留给
        // lib_ui::time 的纯日期单测，这里不重复钉时区。
        let s = fmt_log_time(1_700_000_000_123);
        assert_eq!(s.len(), 14, "MM-DD HH:mm:ss, got {s}");
        assert_eq!(s.chars().nth(5), Some(' '));
        assert_eq!(s.chars().filter(|c| *c == ':').count(), 2);
        assert!(s.chars().all(|c| c.is_ascii() || c == ' '), "got {s}");
        assert!(fmt_log_time(1_700_000_059_000).ends_with(":19"));
    }

    #[test]
    fn reason_scores_cover_prefixed_and_plain_forms() {
        // 箭头两侧都带前缀（历史行）：右边取 85 而不是 70 之类的杂数
        assert_eq!(parse_reason_scores("八分制 8 → 百分制 85（rule）薪资偏低"), Some((Some(8), Some(85))));
        // 现行 rescore 写法：数字紧跟箭头
        assert_eq!(parse_reason_scores("8 → 85（rule）薪资偏低"), Some((Some(8), Some(85))));
        // 无旧分：左端是「无分」，不猜 0
        assert_eq!(parse_reason_scores("无分 → 85（rule）"), Some((None, Some(85))));
        // 无箭头 = 抠不出来
        assert_eq!(parse_reason_scores("外包公司"), None);
        assert_eq!(parse_reason_scores("百分制 85"), None);
    }

    #[test]
    fn filters_and_reason_fallbacks() {
        // 空选中集 = 不过滤
        assert!(log_visible(LogLevel::Error, "boom", "", &[]));
        // 只选 warn 时 info 被挡
        assert!(!log_visible(LogLevel::Info, "ok", "", &[LogLevel::Warn]));
        assert!(log_visible(LogLevel::Warn, "ok", "", &[LogLevel::Warn]));
        // 关键字大小写不敏感
        assert!(log_visible(LogLevel::Info, "Chrome started", "chrome", &[]));
        assert!(!log_visible(LogLevel::Info, "Chrome started", "edge", &[]));

        assert_eq!(decision_label("reject"), "排除");
        assert_eq!(decision_label("rescore"), "重算");
        assert_eq!(decision_label("weird"), "weird");
        assert_eq!(decision_tone("reject"), "bad");
        assert_eq!(reason_text("  "), "（未记录原因）");
        assert_eq!(reason_text("外包"), "外包");
    }
}
