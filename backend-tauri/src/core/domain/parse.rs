//! 岗位文本解析：薪资、经验、HR 活跃度。
//!
//! 全部从 `backend/app/scorer.py` 平移，**行为必须逐字节等价**——
//! 打分口径依赖这些解析结果，差一个字符就会让分数漂移。
//! 对应关系：
//! - [`decode_boss_salary`] ← `scorer.py:129`
//! - [`parse_salary`]       ← `scorer.py:214` `RuleScorer.parse_salary`
//! - [`hr_active_days`]     ← `scorer.py:160`
//! - [`EXP_REQUIRED_RE`]    ← `scorer.py:148`

use regex::Regex;
use std::sync::OnceLock;

/// Boss 列表卡片把薪资数字换成私用区字符做反爬：U+E031→"0"、U+E032→"1"…
const PUA_BASE: u32 = 0xE031;

/// 把 Boss 卡片里的私用区字符还原成阿拉伯数字。
///
/// 列表页的 `span.job-salary` 形如 `\u{E032}\u{E037}-\u{E032}\u{E038}K`，
/// 真实值是「16-17K」。不在 PUA 区的字符原样保留；
/// 其它 PUA 字符（字体占位符）直接丢弃。
pub fn decode_boss_salary(text: Option<&str>) -> String {
    let Some(text) = text else {
        return String::new();
    };
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if (PUA_BASE..=PUA_BASE + 9).contains(&code) {
            // 只可能是 '0'..='9'
            out.push(char::from_digit(code - PUA_BASE, 10).unwrap_or(ch));
        } else if (0xE000..=0xF8FF).contains(&code) {
            continue;
        } else {
            out.push(ch);
        }
    }
    out
}

fn salary_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d{1,3})\s*[-\u{2013}~\u{81f3}]\s*(\d{1,3})\s*[Kk\u{5343}]").unwrap())
}

fn salary_wan_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\d{1,2}(?:\.\d)?)\s*[-\u{2013}~\u{81f3}]\s*(\d{1,2}(?:\.\d)?)\s*\u{4e07}")
            .unwrap()
    })
}

/// 解析薪资文本为 `(下限K, 上限K)`。
///
/// 先解 Boss 反爬字符，否则列表卡片的「16-17K」永远匹配不到。
/// 「万」单位按 ×10 折算成 K（12.5万 → 125K）。
pub fn parse_salary(text: Option<&str>) -> Option<(u32, u32)> {
    let raw = text?;
    let decoded = decode_boss_salary(Some(raw));
    if let Some(c) = salary_re().captures(&decoded) {
        let lo = c.get(1)?.as_str().parse::<u32>().ok()?;
        let hi = c.get(2)?.as_str().parse::<u32>().ok()?;
        return Some((lo, hi));
    }
    if let Some(c) = salary_wan_re().captures(&decoded) {
        let lo = (c.get(1)?.as_str().parse::<f64>().ok()? * 10.0) as u32;
        let hi = (c.get(2)?.as_str().parse::<f64>().ok()? * 10.0) as u32;
        return Some((lo, hi));
    }
    None
}

/// 岗位薪资与用户勾选区间是否有交集；无交集返回 `true`。
///
/// 解析不出来时返回 `false`（宁可放行，不误杀）。
pub fn salary_out_of_range(text: Option<&str>, ranges: &[(u32, u32)]) -> bool {
    if ranges.is_empty() {
        return false;
    }
    let Some((low, high)) = parse_salary(text) else {
        return false;
    };
    !ranges
        .iter()
        .any(|&(r_low, r_high)| high >= r_low && low <= r_high)
}

/// JD 中「N 年以上经验」的识别（`scorer.py:148`）。
pub fn exp_required_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\d{1,2})\s*\u{5e74}\s*(?:\u{53ca}|)\s*(?:\u{4ee5}\u{4e0a}|\u{6216}\u{4ee5}\u{4e0a}|\u{4ee5}\u{4e0a}\u{7684})").unwrap()
    })
}

/// HR 活跃度文案 → 活跃于多少天内。`None` = 无法判断。
///
/// 查找顺序与 Python 的 `HR_ACTIVE_DAYS` 表严格一致（先命中先返回），
/// **不能改成 map 遍历**——列表里「今日」排在「今日活跃」前后的关系会影响结果。
const HR_ACTIVE_DAYS: &[(&str, u32)] = &[
    ("在线", 0),
    ("刚刚", 0),
    ("今日活跃", 0),
    ("今日", 0),
    ("今天活跃", 0),
    ("今日回复", 0),
    ("3日内活跃", 3),
    ("三日内活跃", 3),
    ("3天前活跃", 3),
    ("3日", 3),
    ("7日内活跃", 7),
    ("一周内活跃", 7),
    ("本周活跃", 7),
    ("一周内", 7),
    ("7天前活跃", 7),
    ("2周内活跃", 14),
    ("14日内活跃", 14),
    ("半个月内活跃", 15),
    ("1月内活跃", 30),
    ("本月活跃", 30),
    ("一月内活跃", 30),
    ("30日内活跃", 30),
];

pub fn hr_active_days(status: Option<&str>) -> Option<u32> {
    let raw = status?;
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    for (key, days) in HR_ACTIVE_DAYS {
        if text.contains(key) {
            return Some(*days);
        }
    }
    if let Some(c) = days_within_re().captures(text) {
        return c.get(1)?.as_str().parse::<u32>().ok();
    }
    if hours_within_re().is_match(text) {
        return Some(0);
    }
    if text.contains("今日") || text.contains("在线") {
        return Some(0);
    }
    None
}

fn days_within_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d{1,3})\s*(?:\u{5929}|\u{65e5})\u{5185}\u{6d3b}\u{8dc3}").unwrap())
}

fn hours_within_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\d{1,3})\s*(?:\u{5c0f}\u{65f6}|\u{5206}\u{949f})\u{5185}\u{6d3b}\u{8dc3}")
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_boss_pua_digits() {
        // \u{E032}=1, \u{E037}=6, \u{E038}=7 → "16-17K"
        let raw = "\u{E032}\u{E037}-\u{E032}\u{E038}K";
        assert_eq!(decode_boss_salary(Some(raw)), "16-17K");
    }

    #[test]
    fn drops_non_digit_pua_chars() {
        // \u{E000} 是字体占位符，必须丢弃而不是当成数字
        assert_eq!(decode_boss_salary(Some("a\u{E000}b")), "ab");
    }

    #[test]
    fn decode_handles_none_and_empty() {
        assert_eq!(decode_boss_salary(None), "");
        assert_eq!(decode_boss_salary(Some("")), "");
    }

    #[test]
    fn parses_plain_k_salary() {
        assert_eq!(parse_salary(Some("16-17K")), Some((16, 17)));
        assert_eq!(parse_salary(Some("20-30k")), Some((20, 30)));
        assert_eq!(parse_salary(Some("10-15千")), Some((10, 15)));
        assert_eq!(parse_salary(Some("10至15K")), Some((10, 15)));
    }

    #[test]
    fn parses_pua_encoded_salary() {
        // 端到端：Boss 反爬字符必须能被真实解析出来
        let raw = "\u{E032}\u{E037}-\u{E032}\u{E038}K";
        assert_eq!(parse_salary(Some(raw)), Some((16, 17)));
    }

    #[test]
    fn parses_wan_salary() {
        // 1.2万-1.5万 → 12-15K
        assert_eq!(parse_salary(Some("1.2-1.5万")), Some((12, 15)));
    }

    #[test]
    fn returns_none_for_unparsable() {
        assert_eq!(parse_salary(Some("面议")), None);
        assert_eq!(parse_salary(None), None);
        assert_eq!(parse_salary(Some("")), None);
    }

    #[test]
    fn salary_range_intersection() {
        // 16-17K 与 15-20K 有交集
        assert!(!salary_out_of_range(Some("16-17K"), &[(15, 20)]));
        // 16-17K 与 20-30K 无交集
        assert!(salary_out_of_range(Some("16-17K"), &[(20, 30)]));
        // 边界相接也算有交集
        assert!(!salary_out_of_range(Some("16-17K"), &[(17, 25)]));
    }

    #[test]
    fn unparsable_salary_is_let_through() {
        // 宁可放行不误杀
        assert!(!salary_out_of_range(Some("面议"), &[(20, 30)]));
        assert!(!salary_out_of_range(Some("16-17K"), &[]));
    }

    #[test]
    fn hr_active_days_table_matches_python_order() {
        assert_eq!(hr_active_days(Some("在线")), Some(0));
        assert_eq!(hr_active_days(Some("今日活跃")), Some(0));
        assert_eq!(hr_active_days(Some("刚刚活跃")), Some(0));
        assert_eq!(hr_active_days(Some("3日内活跃")), Some(3));
        assert_eq!(hr_active_days(Some("7日内活跃")), Some(7));
        assert_eq!(hr_active_days(Some("2周内活跃")), Some(14));
        assert_eq!(hr_active_days(Some("1月内活跃")), Some(30));
    }

    #[test]
    fn hr_active_days_regex_fallback() {
        assert_eq!(hr_active_days(Some("5日内活跃")), Some(5));
        assert_eq!(hr_active_days(Some("2小时内活跃")), Some(0));
        assert_eq!(hr_active_days(Some("半小时内活跃")), None);
    }

    #[test]
    fn hr_active_days_unknown() {
        assert_eq!(hr_active_days(Some("")), None);
        assert_eq!(hr_active_days(None), None);
        assert_eq!(hr_active_days(Some("很久以前")), None);
    }

    #[test]
    fn exp_required_matches_python() {
        assert!(exp_required_re().is_match("3年以上经验"));
        assert!(exp_required_re().is_match("5年或以上"));
        assert!(exp_required_re().is_match("10年以上的"));
        assert!(!exp_required_re().is_match("3-5年"));
    }
}
