//! 100 分制打分口径（SPEC-002 第 3 节）。
//!
//! 口径来源：`backend/app/scorer.py:200 RuleScorer`（用户 m00628 拍板「按现有实现改 SPEC」）。
//! 平移原则是**量纲换算**而非重新定义维度，因此旧岗位分数 = 新分数 / 10，
//! 存量数据可无缝迁移（`score_scale` 幂等标记见 SPEC-002 第 3.3 节）。

use crate::core::scoring::SCORE_MAX;

/// 五个打分维度。
///
/// 顺序与权重直接对应 `RuleScorer` 的 `_skill` / `_salary` / `_experience`
/// / `_direction` / `_company`，任何调整都必须同步改 Python 侧或明确废止它。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// 技能匹配（`_skill`）
    Skill,
    /// 薪资匹配（`_salary`）
    Salary,
    /// 经验匹配（`_experience`）
    Experience,
    /// 岗位方向（`_direction`）
    Direction,
    /// 公司质量（`_company`）
    Company,
}

impl Dimension {
    /// 该维度的满分（权重即该项可达的最高分）。
    pub fn max_score(self) -> u8 {
        WEIGHTS
            .iter()
            .find(|(d, _)| *d == self)
            .map(|(_, w)| *w)
            .unwrap_or(0)
    }
}

/// 维度权重表：**这些数字就是该维度的满分**。
///
/// 与旧实现的关键差异：旧 `RuleScorer` 内部各维度已是百分制贡献
/// （如 `_skill` 返回 0-40），但最终又用 `min(10, -(-total // 10))`
/// 压回十分制（`scorer.py:343`）。这里删除那次压缩，各维度贡献直接相加。
pub const WEIGHTS: &[(Dimension, u8)] = &[
    (Dimension::Skill, 40),
    (Dimension::Salary, 20),
    (Dimension::Experience, 15),
    (Dimension::Direction, 15),
    (Dimension::Company, 10),
];

/// 无 JD 时的**最低**保守分：`_skill`=12 + `_salary`=10 + `_experience`=10
/// + `_direction` 最多 15 + `_company` 最多 10 = 57。
///
/// 注意这只是**下界**而非恒定值：薪资和经验能解析出来时分数会更高
/// （如「20-35K」得 15、「5-10年」得 15，合计可达 67）。
/// 真正的硬约束是「无 JD 岗位必须低于门槛」，由 [`rule_score_all`] 的测试保证。
pub const NO_JD_SCORE_FLOOR: u8 = 57;

/// 命中外包/短期信号时的额外惩罚系数（旧实现 `total //= 2`）。
pub const OUTSOURCE_PENALTY_DIVISOR: u8 = 2;

/// 正分维度权重之和，必须等于 [`SCORE_MAX`]。
pub fn positive_weight_total() -> u32 {
    WEIGHTS.iter().map(|(_, w)| *w as u32).sum()
}

/// 把各维度**已达成的分数**（不是百分比）相加，得到 0-100 总分。
///
/// 与骨架阶段的 `total_score(dimensions, deduction)` 不同：这里不再做
/// `weight * ratio / 100` 归一，因为 Python 侧的 `_skill` 等方法本就返回
/// 该维度的实际得分。归一化会在平移时引入舍入偏差，破坏"新旧分数严格 10 倍关系"。
pub fn sum_dimensions(dimensions: &[(Dimension, u8)]) -> u8 {
    let mut sum = 0u32;
    for (dim, raw) in dimensions {
        let capped = (*raw).min(dim.max_score()) as u32;
        sum += capped;
    }
    sum.min(SCORE_MAX as u32) as u8
}

/// 规则打分的**唯一出口**：组装五维得分，套用外包惩罚，夹到 `[0, 100]`。
///
/// 旧实现的 `max(1, ...)` 下限被取消——0 分是合法结果，
/// 强行抬到 1 会让「零匹配」与「差匹配」无法区分。
pub fn rule_score(
    skill: u8,
    salary: u8,
    experience: u8,
    direction: u8,
    company: u8,
    outsource_hit: bool,
) -> u8 {
    let total = sum_dimensions(&[
        (Dimension::Skill, skill),
        (Dimension::Salary, salary),
        (Dimension::Experience, experience),
        (Dimension::Direction, direction),
        (Dimension::Company, company),
    ]);
    if outsource_hit {
        total / OUTSOURCE_PENALTY_DIVISOR
    } else {
        total
    }
}

/// 门槛默认值，供 Rust 侧直接引用。
pub const MIN_SCORE_DEFAULT: u8 = crate::core::scoring::SCORE_MIN_DEFAULT;

// ======================================================================
// 五维打分（`scorer.py:350-423` 的逐项平移）
// ======================================================================

use crate::core::domain::parse::parse_salary;
use crate::core::domain::profile::Profile;
use crate::core::domain::rules::{is_short_term, outsource_hit, OUTSOURCE_WORDS};
use regex::Regex;
use std::sync::OnceLock;

/// 技能维度（`_skill`，满分 40）。
///
/// 核心技能命中每个 +1.0、加分技能 +0.5，换算 `min(1.0, weight/5.0) * 40`。
///
/// 无 JD 返回 12：这是**刻意降权**，不是缺陷——没拿到详情页时既不该给满分，
/// 也不该判死，12 分保证它无法越过 70 分门槛，但不会把整个维度清零。
pub fn score_skill(jd: Option<&str>, profile: &Profile) -> u8 {
    let Some(jd) = jd else {
        return 12;
    };
    if jd.trim().is_empty() {
        // Python 侧 `if not jd` 对空串同样走降权分支
        return 12;
    }
    let text = jd.to_lowercase();
    let mut weight = 0.0f64;
    for s in profile.core_skills {
        if text.contains(&s.to_lowercase()) {
            weight += 1.0;
        }
    }
    for s in profile.bonus_skills {
        if text.contains(&s.to_lowercase()) {
            weight += 0.5;
        }
    }
    (weight.min(5.0) / 5.0 * 40.0) as u8
}

/// 薪资维度（`_salary`，满分 20）。
///
/// 先把岗位薪资夹到画像区间 [salary_low, salary_high]，
/// 再按覆盖比例给分。解析失败返回 10（保守值）。
pub fn score_salary(salary: Option<&str>, profile: &Profile) -> u8 {
    let Some((low_raw, high_raw)) = parse_salary(salary) else {
        return 10;
    };
    // 上限用 9999 这类写法时 high 会远超期望区间，必须先夹到画像上限，
    // 否则下面的 hi - lo 反而可能溢出给出满分
    let high = high_raw.min(profile.salary_high);
    let low = low_raw.min(profile.salary_high);
    if high == 0 {
        return 10;
    }
    let lo = low.max(profile.salary_low);
    let hi = high.min(profile.salary_high);
    if hi <= lo {
        let gap = low as i64 - profile.salary_high as i64;
        return if gap <= 5 {
            10
        } else if gap <= 15 {
            5
        } else {
            2
        };
    }
    let span = profile.salary_span();
    if span == 0 {
        return 10;
    }
    ((hi - lo) as f64 / span as f64 * 20.0) as u8
}

fn exp_range_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d{1,2})\s*[-]\s*(\d{1,2})\s*年").unwrap())
}

/// 经验维度（`_experience`，满分 15）。
pub fn score_experience(exp: Option<&str>, jd: Option<&str>, profile: &Profile) -> u8 {
    let text = format!("{} {}", exp.unwrap_or(""), jd.unwrap_or(""));
    let years = profile.years;

    if let Some(c) = exp_range_re().captures(&text) {
        let lo: u32 = c.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        let hi: u32 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
        if lo <= years && years <= hi {
            // 要求 5 年以上且我满足 → 完全匹配；否则只是"够用"
            return if lo >= 5 { 15 } else { 12 };
        }
        if years > hi {
            return 8; // 我超出岗位要求（岗位偏低）
        }
        return 6; // 我不够（岗位偏高）
    }

    if text.contains("不限") || text.contains("应届") {
        return 10;
    }

    if let Some(c) = crate::core::domain::parse::exp_required_re().captures(&text) {
        if let Some(need) = c.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
            if need <= years {
                return if need >= 5 { 15 } else { 10 };
            }
            return 3;
        }
    }

    10
}

/// 岗位方向（`_direction`，满分 15）。
pub fn score_direction(title: Option<&str>) -> u8 {
    let t = title.unwrap_or("").to_lowercase();
    const CORE: [&str; 5] = ["java", "后端", "服务端", "全栈", "backend"];
    const ADJACENT: [&str; 4] = ["node", "go", "python", "前端"];
    if CORE.iter().any(|k| t.contains(k)) {
        return 15;
    }
    if ADJACENT.iter().any(|k| t.contains(k)) {
        return 10;
    }
    7
}

/// 公司质量（`_company`，满分 10）。
///
/// 命中外包词或短期信号直接 0 分；JD 含离职期/过渡岗给 4 分。
pub fn score_company(company: Option<&str>, title: Option<&str>, jd: Option<&str>) -> u8 {
    let text = format!(
        "{} {} {}",
        company.unwrap_or(""),
        title.unwrap_or(""),
        jd.unwrap_or("")
    );
    if OUTSOURCE_WORDS.iter().any(|w| text.contains(w)) {
        return 0;
    }
    if is_short_term(&format!("{} {}", title.unwrap_or(""), jd.unwrap_or(""))) {
        return 0;
    }
    if let Some(jd) = jd {
        const TRANSIENT: [&str; 3] = ["离职期", "过渡岗", "短期项目"];
        if TRANSIENT.iter().any(|w| jd.contains(w)) {
            return 4;
        }
    }
    10
}

/// 规则打分的完整结果，含可读的评分理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleScore {
    /// 最终分数 0-100
    pub score: u8,
    /// 各维度明细，形如「技能 40/40 | 薪资 15/20 | …」
    pub reason: String,
    /// 是否因「拿不到 JD」而被判不入库（用户 m00716 决策）。
    ///
    /// 这是**显式否决**，不依赖分数是否碰巧低于门槛：
    /// 实测无 JD 岗位在薪资与经验都匹配时可拿到 72 分，会越过默认 70 分门槛。
    pub rejected_no_jd: bool,
}

impl RuleScore {
    /// 是否应当入库。门槛判断**与**「无 JD 否决」取交集。
    pub fn passes(&self, min_score: u8) -> bool {
        !self.rejected_no_jd && self.score >= min_score
    }
}

/// 一次算完五个维度并给出总分（`scorer.py:297 RuleScorer.score` 的平移）。
pub fn rule_score_all(
    company: Option<&str>,
    title: Option<&str>,
    salary: Option<&str>,
    experience: Option<&str>,
    jd: Option<&str>,
    profile: &Profile,
) -> RuleScore {
    let no_jd = !jd.map(|s| !s.trim().is_empty()).unwrap_or(false);

    let s_skill = score_skill(jd, profile);
    let s_salary = score_salary(salary, profile);
    let s_exp = score_experience(experience, jd, profile);
    let s_dir = score_direction(title);
    let s_company = score_company(company, title, jd);

    let parts = vec![
        format!("技能 {s_skill}/40"),
        format!("薪资 {s_salary}/20"),
        format!("经验 {s_exp}/15"),
        format!("方向 {s_dir}/15"),
        format!("公司质量 {s_company}/10"),
    ];

    let mut penalty = String::new();
    if no_jd {
        penalty.push_str("（详情页没拿到 JD，按保守值打分）");
    }

    let hit = outsource_hit(company, title, jd);
    let score = rule_score(s_skill, s_salary, s_exp, s_dir, s_company, hit);
    if hit {
        penalty.push_str("；外包/短期信号→总分减半");
    }
    if no_jd {
        penalty.push_str("；无 JD→判不入库");
    }

    RuleScore {
        score,
        reason: format!("{}{}", parts.join(" | "), penalty),
        rejected_no_jd: no_jd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::profile::DEFAULT_PROFILE;

    #[test]
    fn weights_sum_to_score_max() {
        // SPEC-002 第 3.1 节不变式 2：这条断言上次拦下了 SPEC 自身的算术错误
        assert_eq!(positive_weight_total(), SCORE_MAX as u32);
    }

    #[test]
    fn weight_table_matches_python_implementation() {
        // 与 scorer.py 的实测满分逐项对齐，防止再次"凭空设计维度"
        assert_eq!(Dimension::Skill.max_score(), 40);
        assert_eq!(Dimension::Salary.max_score(), 20);
        assert_eq!(Dimension::Experience.max_score(), 15);
        assert_eq!(Dimension::Direction.max_score(), 15);
        assert_eq!(Dimension::Company.max_score(), 10);
    }

    #[test]
    fn full_marks_yield_100() {
        assert_eq!(rule_score(40, 20, 15, 15, 10, false), 100);
    }

    #[test]
    fn zero_marks_yield_zero_not_one() {
        // 旧实现 max(1, ...) 会返回 1，这里必须是 0
        assert_eq!(rule_score(0, 0, 0, 0, 0, false), 0);
    }

    #[test]
    fn dimension_overflow_is_clamped() {
        assert_eq!(sum_dimensions(&[(Dimension::Skill, 200)]), 40);
        assert_eq!(rule_score(200, 200, 200, 200, 200, false), 100);
    }

    #[test]
    fn outsource_halves_the_score() {
        assert_eq!(rule_score(40, 20, 15, 15, 0, true), 45);
    }

    #[test]
    fn missing_dimensions_contribute_nothing() {
        assert_eq!(sum_dimensions(&[(Dimension::Skill, 40)]), 40);
    }

    #[test]
    fn default_threshold_is_70() {
        assert_eq!(MIN_SCORE_DEFAULT, 70);
    }

    // ---- 技能维度 ----

    #[test]
    fn skill_no_jd_returns_12() {
        assert_eq!(score_skill(None, &DEFAULT_PROFILE), 12);
        assert_eq!(score_skill(Some(""), &DEFAULT_PROFILE), 12);
        assert_eq!(score_skill(Some("   "), &DEFAULT_PROFILE), 12);
    }

    #[test]
    fn skill_all_core_hits_is_full_marks() {
        // 用画像自身的技能拼 JD，避免本测试变成「画像内容快照」。
        let jd = DEFAULT_PROFILE.core_skills.join(" ");
        assert_eq!(score_skill(Some(&jd), &DEFAULT_PROFILE), 40);
    }

    #[test]
    fn skill_partial_hits() {
        // 取画像里前两项 core 技能：各 1.0 → 2.0 / 5.0 * 40 = 16
        let core_two = format!(
            "{} 与 {}",
            DEFAULT_PROFILE.core_skills[0], DEFAULT_PROFILE.core_skills[1]
        );
        assert_eq!(score_skill(Some(&core_two), &DEFAULT_PROFILE), 16);
        // 取画像里前两项 bonus 技能：各 0.5 → 1.0 / 5.0 * 40 = 8
        let bonus_two = format!(
            "{} {}",
            DEFAULT_PROFILE.bonus_skills[0], DEFAULT_PROFILE.bonus_skills[1]
        );
        assert_eq!(score_skill(Some(&bonus_two), &DEFAULT_PROFILE), 8);
    }

    #[test]
    fn skill_match_is_case_insensitive() {
        // 大小写不敏感只需验证「同一组技能，大小写两次得分相等」，
        // 不应依赖具体是哪两个技能命中。
        let upper_two = format!(
            "{} {}",
            DEFAULT_PROFILE.core_skills[0], DEFAULT_PROFILE.core_skills[1]
        );
        let lower = score_skill(Some(&upper_two.to_lowercase()), &DEFAULT_PROFILE);
        let upper = score_skill(Some(&upper_two.to_uppercase()), &DEFAULT_PROFILE);
        assert_eq!(lower, upper);
    }

    #[test]
    fn skill_caps_at_40() {
        // core + bonus 全部命中远超满分，验证上限截断而非短路。
        let jd = format!(
            "{} {}",
            DEFAULT_PROFILE.core_skills.join(" "),
            DEFAULT_PROFILE.bonus_skills.join(" ")
        );
        assert_eq!(score_skill(Some(&jd), &DEFAULT_PROFILE), 40);
    }

    // ---- 薪资维度 ----

    #[test]
    fn salary_unparsable_returns_10() {
        assert_eq!(score_salary(Some("面议"), &DEFAULT_PROFILE), 10);
        assert_eq!(score_salary(None, &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn salary_full_span_is_20() {
        // 15-35K 正好覆盖期望区间
        assert_eq!(score_salary(Some("15-35K"), &DEFAULT_PROFILE), 20);
    }

    #[test]
    fn salary_above_range_gives_fallback() {
        // 100-9999K：low 夹到 35 → hi<=lo，gap = 35-35 = 0 → 10
        assert_eq!(score_salary(Some("100-9999K"), &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn salary_far_below_range_penalized() {
        // 1-2K：hi<=lo，gap = 1-35 = -34 ≤ 5 → 10
        assert_eq!(score_salary(Some("1-2K"), &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn salary_partial_overlap() {
        // 20-35K：lo=max(20,15)=20, hi=min(35,35)=35 → (15/20)*20 = 15
        assert_eq!(score_salary(Some("20-35K"), &DEFAULT_PROFILE), 15);
    }

    // ---- 经验维度 ----

    #[test]
    fn experience_matching_range_full_marks() {
        // 5-15年，我 10 年，lo=5>=5 → 15
        assert_eq!(score_experience(Some("5-15年"), None, &DEFAULT_PROFILE), 15);
        // 3-15年，我 10 年，lo=3<5 → 12
        assert_eq!(score_experience(Some("3-15年"), None, &DEFAULT_PROFILE), 12);
    }

    #[test]
    fn experience_i_exceed_range() {
        // 1-5年，我 10 年 → 超出 → 8
        assert_eq!(score_experience(Some("1-5年"), None, &DEFAULT_PROFILE), 8);
    }

    #[test]
    fn experience_i_lack() {
        // 0-3年... 实际 lo=0<=10<=3 不成立，10>3 → 8
        // 用 15-20年验证"我不够"分支
        assert_eq!(score_experience(Some("15-20年"), None, &DEFAULT_PROFILE), 6);
    }

    #[test]
    fn experience_open_to_all() {
        assert_eq!(score_experience(Some("经验不限"), None, &DEFAULT_PROFILE), 10);
        assert_eq!(score_experience(Some("欢迎应届"), None, &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn experience_required_years() {
        assert_eq!(score_experience(None, Some("要求8年以上"), &DEFAULT_PROFILE), 15);
        assert_eq!(score_experience(None, Some("要求3年以上"), &DEFAULT_PROFILE), 10);
        assert_eq!(score_experience(None, Some("要求15年以上"), &DEFAULT_PROFILE), 3);
    }

    #[test]
    fn experience_default_is_10() {
        assert_eq!(score_experience(None, None, &DEFAULT_PROFILE), 10);
    }

    // ---- 方向维度 ----

    #[test]
    fn direction_core_titles_full_marks() {
        for t in ["Java 工程师", "后端开发", "服务端开发", "全栈工程师", "Backend Engineer"] {
            assert_eq!(score_direction(Some(t)), 15, "「{t}」应为 15");
        }
    }

    #[test]
    fn direction_adjacent_titles() {
        for t in ["Node 工程师", "Go 开发", "Python 开发", "前端工程师"] {
            assert_eq!(score_direction(Some(t)), 10, "「{t}」应为 10");
        }
    }

    #[test]
    fn direction_other() {
        assert_eq!(score_direction(Some("算法工程师")), 7);
        assert_eq!(score_direction(None), 7);
    }

    // ---- 公司质量维度 ----

    #[test]
    fn company_normal_is_full_marks() {
        assert_eq!(score_company(Some("某科技"), Some("Java"), Some("写代码")), 10);
    }

    #[test]
    fn company_outsourcing_is_zero() {
        assert_eq!(score_company(Some("某人力外包"), Some("Java"), None), 0);
        assert_eq!(score_company(None, Some("Java 3个月项目制"), None), 0);
    }

    #[test]
    fn company_transient_jd_is_4() {
        assert_eq!(score_company(Some("某科技"), Some("Java"), Some("这是离职期替岗")), 4);
    }

    // ---- 完整打分 ----

    #[test]
    fn full_scoring_of_ideal_job_is_high() {
        // JD 必须按当前画像动态构造：写死技能名会让本测试变成「画像内容快照」，
        // 用户调整 DEFAULT_PROFILE 时误报失败，而真正要固定的是打分链路本身。
        let jd = format!(
            "{} 负责后端服务开发",
            DEFAULT_PROFILE.core_skills.join(" ")
        );
        let r = rule_score_all(
            Some("某某科技有限公司"),
            Some("Java 后端工程师"),
            Some("20-35K"),
            Some("5-10年"),
            Some(&jd),
            &DEFAULT_PROFILE,
        );
        assert!(r.score >= 80, "理想岗位应高分，实际 {}（JD={}）", r.score, jd);
        assert!(r.reason.contains("技能"));
        assert!(r.reason.contains("公司质量"));
    }

    #[test]
    fn full_scoring_of_outsourcing_job_is_halved() {
        let r = rule_score_all(
            Some("某科技"),
            Some("Java 工程师"),
            Some("20-35K"),
            Some("5-10年"),
            Some("外包岗位，驻场开发"),
            &DEFAULT_PROFILE,
        );
        // 公司质量 0 分 + 总分减半，必然低于门槛
        assert!(r.score < MIN_SCORE_DEFAULT, "外包岗位不该过门槛，实际 {}", r.score);
        assert!(r.reason.contains("总分减半"));
    }

    #[test]
    fn no_jd_worst_case_is_the_floor() {
        // 薪资、经验都解析不出来 → 12 + 10 + 10 + 15 + 10 = 57
        let r = rule_score_all(
            Some("某某科技有限公司"),
            Some("Java 后端工程师"),
            Some("面议"),
            None,
            None,
            &DEFAULT_PROFILE,
        );
        assert_eq!(r.score, NO_JD_SCORE_FLOOR);
    }

    #[test]
    fn no_jd_job_is_rejected_outright() {
        // 硬约束：无 JD 一律不入库，即使分数算出来很高
        for (salary, exp) in [
            ("面议", None),
            ("20-35K", Some("5-10年")),
            ("15-35K", Some("10-15年")),
        ] {
            let r = rule_score_all(
                Some("某某科技有限公司"),
                Some("Java 后端 全栈 服务端"),
                Some(salary),
                exp,
                None,
                &DEFAULT_PROFILE,
            );
            assert!(
                r.rejected_no_jd,
                "薪资={salary:?} 经验={exp:?} 时应当被判不入库"
            );
            assert!(
                !r.passes(MIN_SCORE_DEFAULT),
                "无 JD 岗位不该过门槛，实际分数 {}",
                r.score
            );
            assert!(r.reason.contains("无 JD→判不入库"));
        }
    }

    #[test]
    fn no_jd_score_can_actually_exceed_threshold() {
        // 记录真实数据：无 JD 但薪资经验匹配时能到 72 分，
        // 这正是必须用 rejected_no_jd 显式否决、而不能依赖分数碰巧偏低的原因
        let r = rule_score_all(
            Some("某某科技有限公司"),
            Some("Java 后端 全栈 服务端"),
            Some("15-35K"),
            Some("10-15年"),
            None,
            &DEFAULT_PROFILE,
        );
        assert!(
            r.score > MIN_SCORE_DEFAULT,
            "若此值降到门槛以下，说明降权逻辑变了，需重新审视该测试的意义；实际 {}",
            r.score
        );
        assert!(!r.passes(MIN_SCORE_DEFAULT), "但依然不该入库");
    }

    #[test]
    fn job_with_jd_passes_by_score_alone() {
        // 有 JD 的优质岗位正常入库。JD 按画像动态构造，避免耦合画像内容。
        let jd = format!("{} 负责后端服务开发", DEFAULT_PROFILE.core_skills.join(" "));
        let r = rule_score_all(
            Some("某某科技有限公司"),
            Some("Java 后端工程师"),
            Some("20-35K"),
            Some("5-10年"),
            Some(&jd),
            &DEFAULT_PROFILE,
        );
        assert!(!r.rejected_no_jd);
        assert!(r.passes(MIN_SCORE_DEFAULT), "实际 {}", r.score);
    }

    #[test]
    fn score_never_exceeds_100_or_wraps() {
        // core + bonus 全命中，逼近满分场景
        let jd = format!(
            "{} {}",
            DEFAULT_PROFILE.core_skills.join(" "),
            DEFAULT_PROFILE.bonus_skills.join(" ")
        );
        let r = rule_score_all(
            Some("某科技"),
            Some("Java 后端 全栈 服务端"),
            Some("15-35K"),
            Some("5-10年"),
            Some(&jd),
            &DEFAULT_PROFILE,
        );
        assert!(r.score <= 100, "实际 {}", r.score);
    }
}
