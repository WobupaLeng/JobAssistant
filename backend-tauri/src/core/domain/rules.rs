//! 硬性排除规则（`backend/app/scorer.py:228 RuleScorer.reject` 的平移）。
//!
//! 判断顺序**必须与 Python 严格一致**：最先命中的规则决定用户的"排除原因"文案，
//! 调整顺序会改变界面提示，属于静默行为变更。

use crate::core::domain::parse::{exp_required_re, hr_active_days, parse_salary};
use crate::core::domain::profile::Profile;
use regex::Regex;
use std::sync::OnceLock;

/// 命中即大幅扣分/判负信号（`scorer.py:41`）。
pub const OUTSOURCE_WORDS: &[&str] = &[
    "外包",
    "驻场",
    "外派",
    "派遣",
    "人力外包",
    "劳务派遣",
    "驻场开发",
    "驻场工程师",
    "外派至",
    "派遣员工",
    "外包编制",
    "外包岗",
    "驻场岗",
];

/// 公司名/岗位名里出现就排除的默认词表（`scorer.py:46`）。
pub const VETO_NAME_WORDS: &[&str] = &[
    "外包", "驻场", "外派", "派遣", "人力", "兼职", "日薪", "跨境", "外贸", "海外仓",
    "亚马逊", "独立站", "Shopify", "电商分销", "私域结算", "金融", "证券", "基金", "银行",
    "支付清结算", "风控", "期权", "币圈", "区块链", "Web3", "硕士", "博士", "985", "211",
    "销售", "产品经理", "总监",
];

/// 用户在界面上可调的排除规则。值为空 = 该项不排除（`scorer.py:61 ExcludeRules`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeRules {
    /// 公司/岗位名命中即排除
    pub name_words: Vec<String>,
    /// 岗位薪资下限低于此值(K) → 排除
    pub exclude_salary_min: Option<u32>,
    /// 岗位薪资上限高于此值(K) → 排除
    pub exclude_salary_max: Option<u32>,
    /// JD 硬卡这些学历 → 排除，如 ["硕士","博士"]
    pub exclude_degree: Vec<String>,
    /// 公司成立年限小于此年数 → 排除
    pub exclude_founded_below: Option<u32>,
    /// JD 要求经验超过此年限 → 排除
    pub exclude_resume_max_years: Option<u32>,
    /// HR 超过这么天内没活跃（或无法判断）就排除
    pub exclude_hr_active_days: Option<u32>,
    /// `Some("double")` = 必须双休；`None` = 不限制
    pub weekend: Option<String>,
}

impl Default for ExcludeRules {
    fn default() -> Self {
        Self {
            name_words: VETO_NAME_WORDS.iter().map(|s| s.to_string()).collect(),
            exclude_salary_min: None,
            exclude_salary_max: None,
            exclude_degree: Vec::new(),
            exclude_founded_below: None,
            exclude_resume_max_years: None,
            exclude_hr_active_days: None,
            weekend: None,
        }
    }
}

impl ExcludeRules {
    /// 是否要求双休。
    pub fn require_double_weekend(&self) -> bool {
        self.weekend.as_deref() == Some("double")
    }

    /// 用用户排除词构造名称否决规则（逗号/顿号/分号/斜杠/空白分隔）。
    ///
    /// 解析后为空则回退 [`Default`]（保留内置否决词）：空串更可能是
    /// 「还没配置」而不是「明确要求不排除」，前端草稿的默认值本身就是
    /// 一份非空词表。其余规则项保持默认，由调用方按需填充
    /// （见 `core::scrape::filters::exclude_rules_from_defaults`）。
    pub fn from_keywords(raw: &str) -> Self {
        let words: Vec<String> = raw
            .split(|c: char| {
                matches!(c, ',' | '，' | '、' | ';' | '；' | '/' | '\n') || c.is_whitespace()
            })
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if words.is_empty() {
            Self::default()
        } else {
            Self {
                name_words: words,
                ..Self::default()
            }
        }
    }
}

/// 待判定岗位的输入。字段名与 `reject()` 参数一一对应。
#[derive(Debug, Clone, Default)]
pub struct JobInput<'a> {
    pub company: Option<&'a str>,
    pub title: Option<&'a str>,
    pub salary: Option<&'a str>,
    pub experience: Option<&'a str>,
    pub jd: Option<&'a str>,
    /// 公司成立年数，`None` = 未知
    pub founded_years: Option<u32>,
    /// Boss 的 HR 活跃度文案，`None` = 未知
    pub hr_active_status: Option<&'a str>,
}

fn weekend_bad_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(单休|大小周|大周|小周|六天工作制|六天制|每周\s*6\s*天|月休\s*[1-7]\s*天|周六(?:上班|加班|需上班|要上班)|周日(?:上班|加班)|做六休一|995|996|997|上班时间[:：]?\s*单休|排班|轮休|调休制度|不固定休息|休息不固定)",
        )
        .unwrap()
    })
}

fn short_term_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\d+\s*[-\u{2013}~\u{81f3}]\s*\d+\s*个?月|周期\s*\d+|短期项目|项目制|过渡岗")
            .unwrap()
    })
}

fn degree_985_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(985|211)\s*(院校|高校|大学)?\s*(毕业|及以上|统招|全日制)").unwrap()
    })
}

fn outsource_neg_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(非|不是|无需|拒绝|不做|不接)\s*(外包|驻场|派遣)").unwrap()
    })
}

fn weekend_neg_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // 否定语境整体吞掉：非单休 / 无大小周 / 不加班 / 拒绝996 等。
        // 必须先整体匹配再替换，否则「非单休」会被拆成「非」+「单休」，
        // 残留的「非」不影响结果，但「单休」仍会被 WEEKEND_BAD_RE 抓到。
        Regex::new(
            r"(?:不是|非|无|没有|拒绝|不做|不接|不支持)\s*(?:单休|大小周|大周|小周|996|997|995|加班|六天工作制|六天制)",
        )
        .unwrap()
    })
}

/// 要求双休时，返回命中的"非双休"说法。
///
/// 先剔除否定语境：「非单休」「无大小周」都不算问题。
///
/// **相对 Python 的行为修正**：原实现只 `replace("非双休", ...)`，
/// 导致「非单休，双休」这类表述被判成非双休（用户 m00692 决定修掉）。
pub fn weekend_bad(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let cleaned = weekend_neg_re().replace_all(text, "__NEG__").to_string();
    weekend_bad_re()
        .captures(&cleaned)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

/// 岗位是否属于短期/项目制。
pub fn is_short_term(text: &str) -> bool {
    short_term_re().is_match(text)
}

/// 外包/短期信号判定（`_company` 与总分减半共用）。
///
/// 注意：这里**不做**否定语境剔除，与 Python 的 `_company` 一致；
/// `reject()` 内部另有剔除逻辑，两者不同，不要合并。
pub fn outsource_hit(company: Option<&str>, title: Option<&str>, jd: Option<&str>) -> bool {
    let text = format!(
        "{} {} {}",
        company.unwrap_or(""),
        title.unwrap_or(""),
        jd.unwrap_or("")
    );
    if OUTSOURCE_WORDS.iter().any(|w| text.contains(w)) {
        return true;
    }
    is_short_term(&format!("{} {}", title.unwrap_or(""), jd.unwrap_or("")))
}

/// 硬性排除：返回排除原因，`None` 表示通过。
///
/// 判断顺序与 `scorer.py:228` 严格一致，**不可重排**。
pub fn reject(job: &JobInput<'_>, rules: &ExcludeRules) -> Option<String> {
    reject_labeled(job, rules).map(|(_, detail)| detail)
}

/// 硬性排除（带规则名）：返回 `(规则名, 给用户看的原因)`，`None` 表示通过。
///
/// 与 [`reject`] 是同一份判定的两个出口，**顺序与分支完全同步**：`reject` 只是
/// 丢掉规则名。决策日志需要知道「被哪条规则否掉」，而规则名不是从原因文本里
/// 反推出来的——反推会在改一次文案时静默失配。
///
/// 规则名是稳定的英文标识（`weekend` / `name_word` / …），原因文本给人看。
pub fn reject_labeled(job: &JobInput<'_>, rules: &ExcludeRules) -> Option<(&'static str, String)> {
    // 1. 双休规则
    if rules.require_double_weekend() {
        let probe = format!("{} {}", job.title.unwrap_or(""), job.jd.unwrap_or(""));
        if let Some(bad) = weekend_bad(&probe) {
            return Some((
                "weekend",
                format!("非双休（JD/岗位名出现「{bad}」）"),
            ));
        }
    }

    // 2. 公司/岗位名命中排除词
    let name_text = format!("{} {}", job.company.unwrap_or(""), job.title.unwrap_or(""));
    for w in &rules.name_words {
        if !w.is_empty() && name_text.contains(w.as_str()) {
            return Some((
                "name_word",
                format!("公司/岗位名命中排除词「{w}」"),
            ));
        }
    }

    // 3. 岗位名显示短期
    if let Some(title) = job.title {
        if is_short_term(title) {
            return Some(("short_term", "岗位名显示短期项目/项目制".into()));
        }
    }

    if let Some(jd) = job.jd {
        // 4. 外包类（先剔除否定语境）
        let cleaned = outsource_neg_re().replace_all(jd, "${1}__NEG__").to_string();
        for w in OUTSOURCE_WORDS {
            if cleaned.contains(w) {
                return Some((
                    "outsource",
                    format!("JD 命中外包/驻场类表述「{w}」"),
                ));
            }
        }

        // 5. 学历硬卡（只有勾选了才判断）
        for lv in &rules.exclude_degree {
            if lv.is_empty() {
                continue;
            }
            let pattern = format!(
                r"{}\s*(研究生)?\s*(及以上|以上|学历)",
                regex::escape(lv)
            );
            if let Ok(re) = Regex::new(&pattern) {
                if re.is_match(jd) {
                    return Some(("degree", format!("JD 要求{lv}学历")));
                }
            }
        }

        // 6. 985/211
        if degree_985_re().is_match(jd) {
            return Some(("degree_985", "JD 硬卡 985/211".into()));
        }

        // 7. 经验门槛（默认不限制）
        if let Some(max_years) = rules.exclude_resume_max_years {
            if let Some(c) = exp_required_re().captures(jd) {
                if let Some(need) = c.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
                    if need > max_years {
                        return Some((
                            "experience",
                            format!("JD 要求 {need} 年以上经验（上限 {max_years} 年）"),
                        ));
                    }
                }
            }
        }
    }

    // 8. HR 活跃度：无法判断的按"不活跃"处理
    if let Some(limit) = rules.exclude_hr_active_days {
        match hr_active_days(job.hr_active_status) {
            None => {
                return Some((
                    "hr_active_unknown",
                    format!("HR 活跃度未知（要求 {limit} 天内活跃）"),
                ));
            }
            Some(days) if days > limit => {
                return Some((
                    "hr_inactive",
                    format!(
                        "HR 最近活跃 {}，超过 {} 天门槛",
                        job.hr_active_status.unwrap_or(""),
                        limit
                    ),
                ));
            }
            _ => {}
        }
    }

    // 9. 公司成立年限
    if let Some(floor) = rules.exclude_founded_below {
        if let Some(years) = job.founded_years {
            if years < floor {
                return Some((
                    "founded",
                    format!("公司成立 {years} 年，不足 {floor} 年"),
                ));
            }
        }
    }

    // 10. 薪资上下限
    if rules.exclude_salary_min.is_some() || rules.exclude_salary_max.is_some() {
        if let Some((low, high)) = parse_salary(job.salary) {
            if let Some(min) = rules.exclude_salary_min {
                if high < min {
                    return Some((
                        "salary_floor",
                        format!("薪资 {low}-{high}K 低于下限 {min}K"),
                    ));
                }
            }
            if let Some(max) = rules.exclude_salary_max {
                if low > max {
                    return Some((
                        "salary_ceiling",
                        format!("薪资 {low}-{high}K 高于上限 {max}K"),
                    ));
                }
            }
        }
    }

    None
}

/// 分数是否达到门槛。`profile` 参数为将来画像可配置预留。
pub fn passes_threshold(score: u8, min_score: u8, _profile: &Profile) -> bool {
    score >= min_score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job<'a>(company: &'a str, title: &'a str, jd: &'a str) -> JobInput<'a> {
        JobInput {
            company: Some(company),
            title: Some(title),
            jd: Some(jd),
            ..Default::default()
        }
    }

    #[test]
    fn default_rules_have_veto_words() {
        let r = ExcludeRules::default();
        assert_eq!(r.name_words.len(), VETO_NAME_WORDS.len());
        assert!(r.exclude_degree.is_empty());
        assert_eq!(r.exclude_hr_active_days, None);
    }

    #[test]
    fn rejects_by_company_name() {
        let r = ExcludeRules::default();
        let j = job("某人力资源公司", "Java 工程师", "写代码");
        let reason = reject(&j, &r).unwrap();
        assert!(reason.contains("命中排除词"), "实际：{reason}");
    }

    #[test]
    fn clean_job_passes() {
        let r = ExcludeRules::default();
        let j = job("某某科技有限公司", "Java 后端工程师", "负责后端服务开发");
        assert_eq!(reject(&j, &r), None);
    }

    #[test]
    fn rejects_short_term_title() {
        let r = ExcludeRules::default();
        let j = job("某科技", "Java 3个月项目制", "写代码");
        assert!(reject(&j, &r).unwrap().contains("短期项目"));
    }

    #[test]
    fn rejects_outsourcing_in_jd() {
        let r = ExcludeRules::default();
        let j = job("某科技", "Java 工程师", "本岗位为外包岗位，需驻场");
        assert!(reject(&j, &r).unwrap().contains("外包/驻场类表述"));
    }

    #[test]
    fn negation_context_is_not_outsourcing() {
        // 「非外包」不应被误判
        let r = ExcludeRules::default();
        let j = job("某科技", "Java 工程师", "本岗位不是外包，无需驻场");
        assert_eq!(reject(&j, &r), None);
    }

    #[test]
    fn rejects_985_requirement() {
        let r = ExcludeRules::default();
        let j = job("某科技", "Java 工程师", "要求 985 院校毕业");
        assert!(reject(&j, &r).unwrap().contains("985"));
    }

    #[test]
    fn degree_check_only_when_configured() {
        let mut r = ExcludeRules::default();
        let j = job("某科技", "Java 工程师", "要求硕士及以上学历");
        // 未配置 exclude_degree 时不因学历排除
        assert_eq!(reject(&j, &r), None);
        r.exclude_degree = vec!["硕士".into()];
        assert!(reject(&j, &r).unwrap().contains("硕士"));
    }

    #[test]
    fn rejects_too_much_experience() {
        let mut r = ExcludeRules::default();
        r.exclude_resume_max_years = Some(5);
        let j = job("某科技", "Java 工程师", "要求 8 年以上经验");
        assert!(reject(&j, &r).unwrap().contains("8 年以上"));
    }

    #[test]
    fn unknown_hr_activity_counts_as_inactive() {
        let mut r = ExcludeRules::default();
        r.exclude_hr_active_days = Some(7);
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.hr_active_status = None;
        assert!(reject(&j, &r).unwrap().contains("活跃度未知"));

        j.hr_active_status = Some("今日活跃");
        assert_eq!(reject(&j, &r), None);

        j.hr_active_status = Some("1月内活跃");
        assert!(reject(&j, &r).unwrap().contains("超过 7 天门槛"));
    }

    /// 抓取端真实产出的文案必须能被规则认出。
    /// `.boss-active-time` 实测取到的是「刚刚活跃」，按 regex 兜底截成「刚刚」
    /// （`scraper/boss.py` 里那段 re.search 的最后一个分支），等价于「在线 = 0 天」。
    /// 若这条断言挂了，说明抓取文案变了而 parse 侧没跟上——规则会静默把所有岗位
    /// 判成「活跃度未知」，这正是 2026-11-21 之前的真实故障。
    #[test]
    fn scraped_variants_of_hr_activity_are_recognized() {
        let mut r = ExcludeRules::default();
        r.exclude_hr_active_days = Some(7);
        let mut j = job("某科技", "Java 工程师", "写代码");

        for status in ["刚刚", "刚刚活跃", "在线", "今日活跃"] {
            j.hr_active_status = Some(status);
            assert_eq!(reject(&j, &r), None, "「{status}」应视为 0 天内活跃");
        }

        for status in ["本月活跃", "1月内活跃", "半年内活跃"] {
            j.hr_active_status = Some(status);
            assert!(reject(&j, &r).is_some(), "「{status}」应超过 7 天门槛");
        }
    }

    #[test]
    fn rejects_salary_below_floor() {
        let mut r = ExcludeRules::default();
        r.exclude_salary_min = Some(20);
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.salary = Some("10-15K");
        assert!(reject(&j, &r).unwrap().contains("低于下限"));
        j.salary = Some("25-30K");
        assert_eq!(reject(&j, &r), None);
    }

    #[test]
    fn rejects_salary_above_ceiling() {
        let mut r = ExcludeRules::default();
        r.exclude_salary_max = Some(20);
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.salary = Some("30-40K");
        assert!(reject(&j, &r).unwrap().contains("高于上限"));
    }

    #[test]
    fn rejects_young_company() {
        let mut r = ExcludeRules::default();
        r.exclude_founded_below = Some(3);
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.founded_years = Some(1);
        assert!(reject(&j, &r).unwrap().contains("不足 3 年"));
        j.founded_years = None; // 未知时不排除
        assert_eq!(reject(&j, &r), None);
    }

    #[test]
    fn weekend_rule_only_when_double() {
        let mut r = ExcludeRules::default();
        let j = job("某科技", "Java 工程师", "大小周");
        assert_eq!(reject(&j, &r), None);
        r.weekend = Some("double".into());
        assert!(reject(&j, &r).unwrap().contains("非双休"));
    }

    #[test]
    fn weekend_negation_not_flagged() {
        let r = ExcludeRules {
            weekend: Some("double".into()),
            ..Default::default()
        };
        let j = job("某科技", "Java 工程师", "非单休，双休");
        assert_eq!(reject(&j, &r), None);
    }

    #[test]
    fn weekend_negation_variants_not_flagged() {
        let r = ExcludeRules {
            weekend: Some("double".into()),
            ..Default::default()
        };
        for jd in [
            "非双休",        // Python 原本就能处理
            "非单休，双休",  // 用户 m00692 要求修掉的误判
            "无大小周，双休",
            "不加班，双休",
            "拒绝996，双休",
            "没有单休，做五休二",
        ] {
            let j = job("某科技", "Java 工程师", jd);
            assert_eq!(reject(&j, &r), None, "「{jd}」不应被判为非双休");
        }
    }

    #[test]
    fn real_single_day_off_still_flagged() {
        let r = ExcludeRules {
            weekend: Some("double".into()),
            ..Default::default()
        };
        for jd in ["单休", "大小周", "每周 6 天", "做六休一", "996"] {
            let j = job("某科技", "Java 工程师", jd);
            assert!(reject(&j, &r).is_some(), "「{jd}」应当被判为非双休");
        }
    }

    #[test]
    fn negation_only_swallows_adjacent_phrase() {
        // 「非单休，但大小周」：否定只吞掉"非单休"，"大小周"仍要命中
        let r = ExcludeRules {
            weekend: Some("double".into()),
            ..Default::default()
        };
        let j = job("某科技", "Java 工程师", "非单休，但实行大小周");
        let reason = reject(&j, &r).expect("大小周仍应命中");
        assert!(reason.contains("大小周"), "实际：{reason}");
    }

    #[test]
    fn outsource_hit_detects_and_ignores_negation() {
        assert!(outsource_hit(Some("某科技"), Some("Java"), Some("外包岗位")));
        assert!(outsource_hit(None, Some("3个月短期项目制"), None));
        // 注意：这个函数不做否定剔除，与 Python `_company` 一致
        assert!(outsource_hit(None, None, Some("非外包")));
    }

    #[test]
    fn threshold_check_is_inclusive() {
        let p = crate::core::domain::profile::DEFAULT_PROFILE;
        assert!(passes_threshold(70, 70, &p));
        assert!(!passes_threshold(69, 70, &p));
    }
}
