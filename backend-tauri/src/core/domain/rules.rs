//! 硬性排除规则（`backend/app/scorer.py:228 RuleScorer.reject` 的平移）。
//!
//! 判断顺序**必须与 Python 严格一致**：最先命中的规则决定用户的"排除原因"文案，
//! 调整顺序会改变界面提示，属于静默行为变更。

use crate::core::domain::parse::{
    exp_required_re, hr_active_days, parse_salary, salary_out_of_range,
};
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
    /// 用户勾选的薪资区间（K，闭区间）：岗位薪资与**任何一档都不相交**即排除。
    /// 空表 = 未勾选，不限制。
    pub salary_ranges: Vec<(u32, u32)>,
    /// JD 硬卡这些学历 → 排除，如 ["硕士","博士"]
    pub exclude_degree: Vec<String>,
    /// 公司成立年限小于此年数 → 排除
    pub exclude_founded_below: Option<u32>,
    /// JD 要求经验超过此年限 → 排除
    pub exclude_resume_max_years: Option<u32>,
    /// HR 超过这么天内没活跃（或无法判断）就排除
    pub exclude_hr_active_days: Option<u32>,
    /// 休息制度档位字面量（`any`/`double`/`no_bigweek`/`single`，SPEC-004）；
    /// `None`/`"any"`/未知字面量 = 不限制
    pub weekend: Option<String>,
}

impl Default for ExcludeRules {
    fn default() -> Self {
        Self {
            name_words: VETO_NAME_WORDS.iter().map(|s| s.to_string()).collect(),
            exclude_salary_min: None,
            exclude_salary_max: None,
            salary_ranges: Vec::new(),
            exclude_degree: Vec::new(),
            exclude_founded_below: None,
            exclude_resume_max_years: None,
            exclude_hr_active_days: None,
            weekend: None,
        }
    }
}

/// 休息制度要求档位（SPEC-004：double ⊂ no_bigweek ⊂ single）。
///
/// 字面量集与前端 `WEEKEND_OPTIONS` 逐字同源（any/double/no_bigweek/single）；
/// `work_weekend` 是 JSON 透传、无编译期枚举校验，错一个字母就静默丢筛选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekendTier {
    /// 只投双休
    Double,
    /// 大小周 + 双休
    NoBigweek,
    /// 单休 + 大小周 + 双休（只排除无休）
    Single,
}

impl WeekendTier {
    /// 该档位是否排除给定休息制度族。
    fn excludes(self, family: RestFamily) -> bool {
        match self {
            Self::Double => true,
            Self::NoBigweek => family >= RestFamily::Single,
            Self::Single => family >= RestFamily::NoRest,
        }
    }
}

/// JD 命中词归入的休息制度族（SPEC-004 §四 归类表，Chief 已逐条确认）。
///
/// 声明序即轻→重，多重命中取最重族。歧义命中（995/周六加班/月休 1–2 天/
/// 排班轮休等）按较重档保守归入单休族，维持既有排除现状。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RestFamily {
    /// 大小周
    Bigweek,
    /// 单休
    Single,
    /// 无休
    NoRest,
}

impl ExcludeRules {
    /// 休息制度档位（SPEC-004）。`None`/`"any"`/未知字面量 = 不限制。
    pub fn weekend_tier(&self) -> Option<WeekendTier> {
        match self.weekend.as_deref() {
            Some("double") => Some(WeekendTier::Double),
            Some("no_bigweek") => Some(WeekendTier::NoBigweek),
            Some("single") => Some(WeekendTier::Single),
            _ => None,
        }
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
            r"(单休|大小周|大周|小周|六天工作制|六天制|每周\s*6\s*天|月休\s*[1-7]\s*天|周六(?:上班|加班|需上班|要上班)|周日(?:上班|加班)|做六休一|995|996|997|无休|上班时间[:：]?\s*单休|排班|轮休|调休制度|不固定休息|休息不固定)",
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
            r"(?:不是|非|无|没有|拒绝|不做|不接|不支持)\s*(?:单休|大小周|大周|小周|996|997|995|加班|六天工作制|六天制|无休)",
        )
        .unwrap()
    })
}

/// 把 JD/岗位名里的休息制度说法归类（SPEC-004 §四）。
///
/// 先剔除否定语境：「非单休」「无大小周」「非无休」都不算命中。
/// 多重命中取最重族（无休＞单休＞大小周），返回该族的首个命中词。
/// 无命中 = 未标注，调用方归一化为双休（三档均含）。
///
/// **相对 Python 的行为修正**：原实现只 `replace("非双休", ...)`，
/// 导致「非单休，双休」这类表述被判成非双休（用户 m00692 决定修掉）。
pub fn weekend_classify(text: &str) -> Option<(RestFamily, String)> {
    if text.is_empty() {
        return None;
    }
    let cleaned = weekend_neg_re().replace_all(text, "__NEG__").to_string();
    let mut worst: Option<(RestFamily, String)> = None;
    for cap in weekend_bad_re().captures_iter(&cleaned) {
        if let Some(m) = cap.get(1) {
            let hit = m.as_str();
            let family = rest_family(hit);
            if worst.as_ref().map_or(true, |(f, _)| family > *f) {
                worst = Some((family, hit.to_string()));
            }
        }
    }
    worst
}

/// 命中词 → 休息制度族（Chief 确认的归类表；表外说法保守归单休族）。
fn rest_family(hit: &str) -> RestFamily {
    if hit == "997" || hit == "无休" || hit.starts_with("周日") {
        RestFamily::NoRest
    } else if matches!(hit, "大小周" | "大周" | "小周") {
        RestFamily::Bigweek
    } else {
        RestFamily::Single
    }
}

/// 待遇语境词：所在分句讲的是员工待遇而非岗位周期，参与短期判定前整段丢弃。
const SHORT_TERM_NOISE: &[&str] = &[
    "试用期", "实习期", "转正", "年终奖", "绩效", "奖金", "提成", "补贴", "福利",
];

/// 岗位是否属于短期/项目制。
///
/// 先按句读切段并丢掉待遇语境段：旧实现把「试用期3-6个月」「年终奖2-6个月」
/// 判成项目制 → 公司质量 0 分 + 总分减半，正常岗位必被淘汰（用户 m00070 决定修掉）。
pub fn is_short_term(text: &str) -> bool {
    short_term_re().is_match(&strip_treatment_clauses(text))
}

/// 丢弃讲员工待遇的分句，只留下描述岗位本体的文本。
fn strip_treatment_clauses(text: &str) -> String {
    text.split_inclusive(['。', '！', '？', '；', ';', '，', ',', '\n'])
        .filter(|seg| !SHORT_TERM_NOISE.iter().any(|w| seg.contains(w)))
        .collect()
}

/// 外包/短期信号判定（`score_company` 与总分减半共用）。
///
/// 先剔除否定语境（「非外包」「非驻场」），与 [`reject_labeled`] 的 `outsource`
/// 分支同一口径。旧实现在这里不剔除，导致 JD 主动声明「非外包」的岗位公司质量
/// 0 分 + 总分减半必被淘汰（用户 m00070 决定修掉）。
pub fn outsource_hit(company: Option<&str>, title: Option<&str>, jd: Option<&str>) -> bool {
    let text = format!(
        "{} {} {}",
        company.unwrap_or(""),
        title.unwrap_or(""),
        jd.unwrap_or("")
    );
    let cleaned = outsource_neg_re().replace_all(&text, "${1}__NEG__").to_string();
    if OUTSOURCE_WORDS.iter().any(|w| cleaned.contains(w)) {
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
    // 1. 休息制度规则（SPEC-004 三档；未标注归一化为双休，三档均含）
    if let Some(tier) = rules.weekend_tier() {
        let probe = format!("{} {}", job.title.unwrap_or(""), job.jd.unwrap_or(""));
        if let Some((family, bad)) = weekend_classify(&probe) {
            if tier.excludes(family) {
                let label = match tier {
                    WeekendTier::Double => "非双休",
                    WeekendTier::NoBigweek => "非大小周/双休",
                    WeekendTier::Single => "无休",
                };
                return Some(("weekend", format!("{label}（JD/岗位名出现「{bad}」）")));
            }
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

    // 11. 勾选薪资区间：Boss 服务端只按官方粗档过滤（20-30K 会落到 20-50K 档），
    //     这里按岗位卡片上的真实薪资做精确求交，不相交即排除（同旧版口径）。
    if !rules.salary_ranges.is_empty() {
        if let Some((low, high)) = parse_salary(job.salary) {
            if salary_out_of_range(job.salary, &rules.salary_ranges) {
                return Some((
                    "salary_range",
                    format!(
                        "薪资 {low}-{high}K 不在勾选区间 {} 内",
                        describe_salary_ranges(&rules.salary_ranges)
                    ),
                ));
            }
        }
    }

    None
}

/// 勾选区间给人看的写法：`15-30K / 40-60K / 50K以上`。
fn describe_salary_ranges(ranges: &[(u32, u32)]) -> String {
    ranges
        .iter()
        .map(|&(lo, hi)| {
            if hi >= 9000 {
                format!("{lo}K以上")
            } else {
                format!("{lo}-{hi}K")
            }
        })
        .collect::<Vec<_>>()
        .join(" / ")
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
    fn salary_ranges_reject_non_overlapping_job() {
        let mut r = ExcludeRules::default();
        r.salary_ranges = vec![(15, 30)];
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.salary = Some("45-55K");
        let (rule, reason) = reject_labeled(&j, &r).expect("薪资不在勾选区间必须否决");
        assert_eq!(rule, "salary_range");
        assert!(reason.contains("不在勾选区间 15-30K"), "实际：{reason}");
    }

    #[test]
    fn salary_ranges_keep_overlapping_and_negotiable() {
        let mut r = ExcludeRules::default();
        r.salary_ranges = vec![(15, 30), (40, 60)];
        let mut j = job("某科技", "Java 工程师", "写代码");
        j.salary = Some("28-35K");
        assert_eq!(reject_labeled(&j, &r), None, "与 15-30K 有交集必须放行");
        j.salary = Some("面议");
        assert_eq!(reject_labeled(&j, &r), None, "面议不参与区间否决");
        r.salary_ranges = Vec::new();
        j.salary = Some("1-2K");
        assert_eq!(reject_labeled(&j, &r), None, "未勾选区间不限制");
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

    /// SPEC-004 §二 三档判定矩阵（用户裁决冻结）：double ⊂ no_bigweek ⊂ single，
    /// 未标注归一化为双休（三档均含），any/未设置全过。
    /// 行＝JD 启发式证据（归类表 Chief 已确认），列＝用户档位。
    #[test]
    fn weekend_three_tier_matrix() {
        // (jd, double 过, no_bigweek 过, single 过)
        let rows = [
            ("双休", true, true, true),
            ("大小周", false, true, true),
            ("单休", false, false, true),
            ("997", false, false, false),
            ("周日上班", false, false, false),
            ("无休", false, false, false),
            // 多重命中取最重族（无休＞单休＞大小周）：single 档也必须排除
            ("大小周，项目忙时997", false, false, false),
            // 未标注 → 归一化双休
            ("负责服务端开发，五险一金", true, true, true),
        ];
        for (jd, double_ok, no_bigweek_ok, single_ok) in rows {
            let j = job("某科技", "Java 工程师", jd);
            for (literal, expected) in [
                ("double", double_ok),
                ("no_bigweek", no_bigweek_ok),
                ("single", single_ok),
            ] {
                let r = ExcludeRules {
                    weekend: Some(literal.into()),
                    ..Default::default()
                };
                let got = reject_labeled(&j, &r);
                assert_eq!(got.is_none(), expected, "tier={literal} jd={jd} got={got:?}");
                if let Some((rule, _)) = &got {
                    assert_eq!(*rule, "weekend", "规则名必须稳定（决策日志契约）");
                }
            }
            // any 与未设置：不限制
            for literal in [Some("any".to_string()), None] {
                let r = ExcludeRules {
                    weekend: literal,
                    ..Default::default()
                };
                assert_eq!(reject(&j, &r), None, "any/未设置不应限制 jd={jd}");
            }
        }
    }

    /// 对含 `jd` 文本的岗位断言三档行为（true = 通过）。
    fn assert_weekend_tiers(jd: &str, double_ok: bool, no_bigweek_ok: bool, single_ok: bool) {
        let j = job("某科技", "Java 工程师", jd);
        for (literal, expected) in [
            ("double", double_ok),
            ("no_bigweek", no_bigweek_ok),
            ("single", single_ok),
        ] {
            let r = ExcludeRules {
                weekend: Some(literal.into()),
                ..Default::default()
            };
            assert_eq!(reject(&j, &r).is_none(), expected, "tier={literal} jd={jd}");
        }
    }

    /// SPEC-004 §四 归类表（Chief 逐条确认）：每个命中词的族归属与三档行为。
    /// 歧义命中按较重档保守归单休族（995/周六加班/月休 1–2 天/排班轮休等），维持现状。
    #[test]
    fn weekend_family_table_chief_confirmed() {
        // 大小周族：double 排除，no_bigweek/single 通过
        for word in ["大小周", "大周", "小周"] {
            assert_weekend_tiers(word, false, true, true);
        }
        // 单休族：double/no_bigweek 排除，single 通过
        for word in [
            "单休",
            "做六休一",
            "996",
            "995",
            "六天工作制",
            "六天制",
            "每周 6 天",
            "月休 4 天",
            "月休 1 天",
            "周六上班",
            "周六加班",
            "周六需上班",
            "排班",
            "轮休",
            "调休制度",
            "不固定休息",
            "休息不固定",
            "上班时间：单休",
        ] {
            assert_weekend_tiers(word, false, false, true);
        }
        // 无休族：三档全排除
        for word in ["997", "周日上班", "周日加班", "无休"] {
            assert_weekend_tiers(word, false, false, false);
        }
    }

    /// SPEC-004 §一 字面量集冻结：any/double/no_bigweek/single 两端逐字同源；
    /// 未知字面量（含大小写变体/中文）现状不限，本批不做旧值兼容。
    #[test]
    fn weekend_literal_set_frozen() {
        // 四字面量的行为差异：单休 JD 区分 double/no_bigweek 与 single
        assert_weekend_tiers("单休", false, false, true);
        // any 全过（连无休也过）
        let j = job("某科技", "Java 工程师", "997 无休");
        let r = ExcludeRules {
            weekend: Some("any".into()),
            ..Default::default()
        };
        assert_eq!(reject(&j, &r), None);
        // 未知字面量 → 不限（发现真实中文值须报 Chief，不私自兼容）
        for unknown in ["Double", "双休", "single_rest", ""] {
            let r = ExcludeRules {
                weekend: Some(unknown.into()),
                ..Default::default()
            };
            assert_eq!(reject(&j, &r), None, "未知字面量 {unknown:?} 应不限");
        }
    }

    /// 否定语境剔除对三档通用（SPEC-004：否定语义与档位无关）。
    #[test]
    fn weekend_negation_applies_to_all_tiers() {
        for jd in ["非单休，双休", "非无休，双休", "无大小周，双休"] {
            assert_weekend_tiers(jd, true, true, true);
        }
    }

    #[test]
    fn outsource_hit_detects_and_ignores_negation() {
        assert!(outsource_hit(Some("某科技"), Some("Java"), Some("外包岗位")));
        assert!(outsource_hit(None, Some("3个月短期项目制"), None));
        // 否定语境与 `reject_labeled` 的 outsource 分支同口径
        assert!(!outsource_hit(None, None, Some("非外包")));
        assert!(!outsource_hit(
            None,
            None,
            Some("本岗位非外包、非驻场，自有产品")
        ));
    }

    #[test]
    fn short_term_ignores_treatment_clauses() {
        // 员工待遇不是岗位周期
        assert!(!is_short_term("试用期3-6个月，五险一金"));
        assert!(!is_short_term("年终奖2-6个月"));
        // 真·项目制仍然命中
        assert!(is_short_term("项目周期3-6个月"));
        assert!(is_short_term("3个月短期项目制"));
        assert!(is_short_term("岗位名显示短期项目"));
    }

    #[test]
    fn threshold_check_is_inclusive() {
        let p = crate::core::domain::profile::DEFAULT_PROFILE;
        assert!(passes_threshold(70, 70, &p));
        assert!(!passes_threshold(69, 70, &p));
    }
}
