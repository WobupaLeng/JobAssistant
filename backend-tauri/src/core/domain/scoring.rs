//! 100 分制打分口径（SPEC-002 第 3 节）。
//!
//! 口径来源：`backend/app/scorer.py:200 RuleScorer`（用户 m00628 拍板「按现有实现改 SPEC」）。
//! 平移原则是**量纲换算**而非重新定义维度，因此旧岗位分数 = 新分数 / 10，
//! 存量数据可无缝迁移（`score_scale` 幂等标记见 SPEC-002 第 3.3 节）。
//!
//! 与 `scorer.py` 的**刻意偏差**（用户 m00070 决定修掉，不是平移遗漏）：
//! 技能/方向按词边界匹配、技能维度核心与加分分开归一、薪资按上限单调、
//! 外包否定语境与 `reject` 统一口径。修这些时不要「照 Python 恢复原样」。

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
    (Dimension::Skill, 50),
    (Dimension::Salary, 10),
    (Dimension::Experience, 15),
    (Dimension::Direction, 15),
    (Dimension::Company, 10),
];

/// 无 JD 时的**最低**保守分：`_skill`=12 + `_salary`=5 + `_experience`=10
/// + `_direction`=15（标题命中核心方向）+ `_company`=10 = 52。
///
/// 这只是**下界**而非恒定值；薪资与经验能解析出来时分数会更高，但取满也只有
/// 12 + 10 + 15 + 15 + 10 = 62，仍低于默认门槛 70。所以「无 JD 不入库」由
/// [`RuleScore::rejected_no_jd`] 显式保证，而不是靠分数碰巧偏低。
pub const NO_JD_SCORE_FLOOR: u8 = 52;

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

/// 规则打分的**唯一出口**：组装五维得分并夹到 `[0, 100]`。
///
/// 旧实现的 `max(1, ...)` 下限被取消——0 分是合法结果，
/// 强行抬到 1 会让「零匹配」与「差匹配」无法区分。
pub fn rule_score(skill: u8, salary: u8, experience: u8, direction: u8, company: u8) -> u8 {
    sum_dimensions(&[
        (Dimension::Skill, skill),
        (Dimension::Salary, salary),
        (Dimension::Experience, experience),
        (Dimension::Direction, direction),
        (Dimension::Company, company),
    ])
}

/// 门槛默认值，供 Rust 侧直接引用。
pub const MIN_SCORE_DEFAULT: u8 = crate::core::scoring::SCORE_MIN_DEFAULT;

// ======================================================================
// 五维打分（`scorer.py:350-423` 的逐项平移）
// ======================================================================

use crate::core::domain::parse::parse_salary;
use crate::core::domain::profile::Profile;
use regex::Regex;
use std::sync::OnceLock;

/// 技能维度内部拆分：核心技能 40 分（命中 [`SKILL_CORE_FULL_HITS`] 项即满）、
/// 加分技能 10 分（命中 [`SKILL_BONUS_FULL_HITS`] 项即满）。
///
/// 分开归一是因为旧实现按「命中条数 / 5」统一归一：5 个核心技能与 10 个通用工具
/// 等价，`Git/Linux/Docker/Html` 这类套话就能把技能分堆满（用户 m00070 决定修掉）。
const SKILL_CORE_POINTS: f64 = 40.0;
const SKILL_CORE_FULL_HITS: f64 = 4.0;
const SKILL_BONUS_POINTS: f64 = 10.0;
const SKILL_BONUS_FULL_HITS: f64 = 5.0;

/// 无 JD 时的技能分（刻意降权，见 [`score_skill`]）。
const SKILL_NO_JD: u8 = 12;

/// 关键词别名：主词匹配不到时再试一次（`go` 必须写全称 `golang` 才算命中）。
const TOKEN_ALIASES: &[(&str, &str)] = &[("go", "golang"), ("node", "nodejs")];

/// 关键词是否出现在文本里（`text_lower` 必须已转小写）。
///
/// 纯 ASCII 词走**词边界**匹配：`java` 不再命中 `javascript`，`go` 不再命中
/// `django`/`mongodb`/`google`；含中文的词没有词边界语义，仍按子串匹配。
fn token_hit(text_lower: &str, token: &str) -> bool {
    let token = token.to_lowercase();
    if token.is_empty() {
        return false;
    }
    let alias = TOKEN_ALIASES
        .iter()
        .find(|(k, _)| *k == token.as_str())
        .map(|(_, v)| *v);
    std::iter::once(token.as_str()).chain(alias).any(|c| {
        if c.is_ascii() {
            ascii_word_hit(text_lower, c)
        } else {
            text_lower.contains(c)
        }
    })
}

/// ASCII 词边界命中：命中位置前后都不能紧挨字母或数字。
fn ascii_word_hit(text_lower: &str, needle_lower: &str) -> bool {
    let bytes = text_lower.as_bytes();
    let needle_len = needle_lower.len();
    let mut from = 0;
    while let Some(rel) = text_lower[from..].find(needle_lower) {
        let start = from + rel;
        let end = start + needle_len;
        let head_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let tail_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if head_ok && tail_ok {
            return true;
        }
        // needle 全 ASCII：命中起点是字符边界，起点 +1 也是
        from = start + 1;
    }
    false
}

/// 技能维度（`_skill`，满分 50）。
///
/// 核心技能 40 分（4 项即满，每项 10）、加分技能 10 分（5 项即满，每项 2），
/// 四舍五入后相加：**零核心技能时最多 10 分**，通用工具堆不出高分。
///
/// 无 JD 返回 12：这是**刻意降权**，不是缺陷——没拿到详情页时既不该给满分，
/// 也不该判死，12 分保证它无法越过 70 分门槛，但不会把整个维度清零。
pub fn score_skill(jd: Option<&str>, profile: &Profile) -> u8 {
    let Some(jd) = jd else {
        return SKILL_NO_JD;
    };
    if jd.trim().is_empty() {
        // Python 侧 `if not jd` 对空串同样走降权分支
        return SKILL_NO_JD;
    }
    let text = jd.to_lowercase();
    let core_hits = profile
        .core_skills
        .iter()
        .copied()
        .filter(|s| token_hit(&text, s))
        .count() as f64;
    let bonus_hits = profile
        .bonus_skills
        .iter()
        .copied()
        .filter(|s| token_hit(&text, s))
        .count() as f64;
    let core_part = core_hits.min(SKILL_CORE_FULL_HITS) / SKILL_CORE_FULL_HITS * SKILL_CORE_POINTS;
    let bonus_part =
        bonus_hits.min(SKILL_BONUS_FULL_HITS) / SKILL_BONUS_FULL_HITS * SKILL_BONUS_POINTS;
    (core_part + bonus_part).round() as u8
}

/// 薪资维度（`_salary`，满分 10；用户 m01018 决定从 20 降到 10，差额给技能）。
///
/// 只看**岗位薪资上限**（能谈到多少），期望区间为 `[salary_low, salary_high]`：
/// - 上限 ≥ 期望上限 → 10（开得比期望高不扣分）；
/// - 上限落在期望区间 → `5 + 5 × (上限 − salary_low) / span`，下沿给 5；
/// - 上限够不到期望下限 → `5 × 上限 / salary_low`，越远越低。
///
/// 不再按「区间宽度」给分：宽度与实得无关，且会制造非单调——旧实现
/// 14-16K 得 1 分、30-60K 得 5 分、50-80K 得 10 分，开得更高反而更低。
/// 解析失败仍返回 5（保守值）。
pub fn score_salary(salary: Option<&str>, profile: &Profile) -> u8 {
    let Some((_, high_raw)) = parse_salary(salary) else {
        return 5;
    };
    let span = profile.salary_span();
    if high_raw == 0 || profile.salary_low == 0 || span == 0 {
        return 5;
    }
    if high_raw >= profile.salary_high {
        return 10;
    }
    if high_raw >= profile.salary_low {
        let into = (high_raw - profile.salary_low) as f64 / span as f64;
        return (5.0 + 5.0 * into.min(1.0)).round() as u8;
    }
    ((high_raw as f64 / profile.salary_low as f64) * 5.0)
        .min(5.0)
        .round() as u8
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
///
/// 与技能维度共用 [`token_hit`] 的词边界：`JavaScript 前端` 不会被 `java` 判成核心方向。
pub fn score_direction(title: Option<&str>) -> u8 {
    let t = title.unwrap_or("").to_lowercase();
    const CORE: [&str; 5] = ["java", "后端", "服务端", "全栈", "backend"];
    const ADJACENT: [&str; 4] = ["node", "go", "python", "前端"];
    if CORE.iter().any(|k| token_hit(&t, k)) {
        return 15;
    }
    if ADJACENT.iter().any(|k| token_hit(&t, k)) {
        return 10;
    }
    7
}

/// 公司质量（`_company`，满分 10）。
///
/// 外包/驻场不再在这里扣分：外包判定已完全交给「排除关键字」（含否定语境剔除），
/// 由 [`crate::core::domain::rules::reject_labeled`] 在打分前拦下。JD 含
/// 离职期/过渡岗/短期项目给 4 分。
pub fn score_company(_company: Option<&str>, _title: Option<&str>, jd: Option<&str>) -> u8 {
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
    /// 各维度明细，形如「技能 40/50 | 薪资 5/10 | …」
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
        format!("技能 {s_skill}/50"),
        format!("薪资 {s_salary}/10"),
        format!("经验 {s_exp}/15"),
        format!("方向 {s_dir}/15"),
        format!("公司质量 {s_company}/10"),
    ];

    let mut penalty = String::new();
    if no_jd {
        penalty.push_str("（详情页没拿到 JD，按保守值打分）");
    }

    let score = rule_score(s_skill, s_salary, s_exp, s_dir, s_company);
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
    fn weight_table_is_locked() {
        // 权重唯一来源。用户 m01018 把薪资权重从 20 降到 10（差额给技能），
        // 因此不再与 scorer.py 的旧比例逐项对齐；改这里必须同步 SPEC-002 §3.1。
        assert_eq!(Dimension::Skill.max_score(), 50);
        assert_eq!(Dimension::Salary.max_score(), 10);
        assert_eq!(Dimension::Experience.max_score(), 15);
        assert_eq!(Dimension::Direction.max_score(), 15);
        assert_eq!(Dimension::Company.max_score(), 10);
    }

    #[test]
    fn full_marks_yield_100() {
        assert_eq!(rule_score(50, 10, 15, 15, 10), 100);
    }

    #[test]
    fn zero_marks_yield_zero_not_one() {
        // 旧实现 max(1, ...) 会返回 1，这里必须是 0
        assert_eq!(rule_score(0, 0, 0, 0, 0), 0);
    }

    #[test]
    fn dimension_overflow_is_clamped() {
        assert_eq!(sum_dimensions(&[(Dimension::Skill, 200)]), 50);
        assert_eq!(rule_score(200, 200, 200, 200, 200), 100);
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
    fn skill_all_core_hits_fills_core_points() {
        // 用画像自身的技能拼 JD，避免本测试变成「画像内容快照」。
        let jd = DEFAULT_PROFILE.core_skills.join(" ");
        // 核心 40 分打满，加分项一个没提 → 40
        assert_eq!(score_skill(Some(&jd), &DEFAULT_PROFILE), 40);
    }

    #[test]
    fn skill_partial_hits() {
        // 取画像里前两项 core 技能：2/4 × 40 = 20
        let core_two = format!(
            "{} 与 {}",
            DEFAULT_PROFILE.core_skills[0], DEFAULT_PROFILE.core_skills[1]
        );
        assert_eq!(score_skill(Some(&core_two), &DEFAULT_PROFILE), 20);
        // 取画像里前两项 bonus 技能：2/5 × 10 = 4
        let bonus_two = format!(
            "{} {}",
            DEFAULT_PROFILE.bonus_skills[0], DEFAULT_PROFILE.bonus_skills[1]
        );
        assert_eq!(score_skill(Some(&bonus_two), &DEFAULT_PROFILE), 4);
    }

    #[test]
    fn skill_ignores_substring_false_positives() {
        // `javascript` 不算命中 `java`，纯前端 JD 技能分为 0
        assert_eq!(score_skill(Some("JavaScript 前端开发"), &DEFAULT_PROFILE), 0);
        // `django`/`mongodb` 不算命中 `go`
        assert_eq!(
            score_skill(Some("Django + MongoDB 后端"), &DEFAULT_PROFILE),
            0
        );
        // 写全称 `golang` 才按 GO 加分（10/5 = 2 分）
        assert_eq!(score_skill(Some("Golang 服务端"), &DEFAULT_PROFILE), 2);
    }

    #[test]
    fn skill_zero_core_is_capped_by_bonus_points() {
        // 零核心技能时加分项最多 10 分，通用工具堆不出技能分
        let jd = DEFAULT_PROFILE.bonus_skills.join(" ");
        assert_eq!(score_skill(Some(&jd), &DEFAULT_PROFILE), 10);
        // 两项核心技能（20 分）已经强于满分加分项
        let core_two = format!(
            "{} {}",
            DEFAULT_PROFILE.core_skills[0], DEFAULT_PROFILE.core_skills[1]
        );
        assert!(score_skill(Some(&core_two), &DEFAULT_PROFILE) > 10);
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
    fn skill_caps_at_50() {
        // core + bonus 全部命中远超满分，验证上限截断而非短路。
        let jd = format!(
            "{} {}",
            DEFAULT_PROFILE.core_skills.join(" "),
            DEFAULT_PROFILE.bonus_skills.join(" ")
        );
        assert_eq!(score_skill(Some(&jd), &DEFAULT_PROFILE), 50);
    }

    // ---- 薪资维度 ----

    #[test]
    fn salary_unparsable_returns_half_marks() {
        assert_eq!(score_salary(Some("面议"), &DEFAULT_PROFILE), 5);
        assert_eq!(score_salary(None, &DEFAULT_PROFILE), 5);
        // 四位数上限超出 `salary_re` 的 \d{1,3}，按解析失败处理
        assert_eq!(score_salary(Some("100-9999K"), &DEFAULT_PROFILE), 5);
    }

    #[test]
    fn salary_full_span_is_10() {
        // 上限覆盖到期望上限即满分
        assert_eq!(score_salary(Some("15-35K"), &DEFAULT_PROFILE), 10);
        assert_eq!(score_salary(Some("20-35K"), &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn salary_above_expectation_is_full_marks() {
        // 开得比期望高不扣分（旧实现给 5/10 分，方向反了）
        assert_eq!(score_salary(Some("30-60K"), &DEFAULT_PROFILE), 10);
        assert_eq!(score_salary(Some("50-80K"), &DEFAULT_PROFILE), 10);
    }

    #[test]
    fn salary_inside_range_scales_from_floor() {
        // 上限 15 = 期望下沿 → 5；上限 30 → 5 + 5×15/20 = 8.75 → 9
        assert_eq!(score_salary(Some("10-15K"), &DEFAULT_PROFILE), 5);
        assert_eq!(score_salary(Some("20-30K"), &DEFAULT_PROFILE), 9);
    }

    #[test]
    fn salary_below_range_decays() {
        // 上限 2K：5×2/15 = 0.67 → 1（旧实现给 10，名为惩罚实则满分）
        assert_eq!(score_salary(Some("1-2K"), &DEFAULT_PROFILE), 1);
        // 上限 8K：5×8/15 = 2.67 → 3
        assert_eq!(score_salary(Some("5-8K"), &DEFAULT_PROFILE), 3);
    }

    #[test]
    fn salary_is_monotone_in_upper_bound() {
        let mut prev = 0;
        for s in [
            "1-2K", "5-8K", "8-10K", "10-14K", "10-15K", "15-25K", "20-30K", "30-60K",
            "50-80K",
        ] {
            let v = score_salary(Some(s), &DEFAULT_PROFILE);
            assert!(v >= prev, "薪资 {s} 得 {v}，低于前一档 {prev}");
            prev = v;
        }
        assert_eq!(prev, 10);
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
        for t in [
            "Node 工程师",
            "Node.js 工程师",
            "Go 开发",
            "Golang 工程师",
            "Python 开发",
            "前端工程师",
        ] {
            assert_eq!(score_direction(Some(t)), 10, "「{t}」应为 10");
        }
    }

    #[test]
    fn direction_does_not_match_inside_words() {
        // `JavaScript` 不是 `java`：按「前端」相邻档给 10，而不是核心档 15
        assert_eq!(score_direction(Some("JavaScript 前端工程师")), 10);
        // `Django`/`MongoDB` 不是 `go`
        assert_eq!(score_direction(Some("Django 工程师")), 7);
        assert_eq!(score_direction(Some("MongoDB 运维")), 7);
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
    fn company_ignores_outsourcing_words() {
        // 外包判定改由「排除关键字」负责（默认词表含外包/驻场），公司维度不再扣分
        assert_eq!(score_company(Some("某人力外包"), Some("Java"), None), 10);
        assert_eq!(score_company(None, Some("Java 3个月项目制"), None), 10);
        assert_eq!(
            score_company(
                Some("某科技"),
                Some("Java 后端"),
                Some("本岗位非外包、非驻场，自有产品研发")
            ),
            10
        );
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
    fn no_jd_worst_case_is_the_floor() {
        // 薪资、经验都解析不出来 → 12 + 5 + 10 + 15 + 10 = 52
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
    fn no_jd_max_score_stays_below_threshold() {
        // 无 JD 取满也只有 12 + 10 + 15 + 15 + 10 = 62（薪资权重降到 10 之后），
        // 但仍保留 rejected_no_jd 显式否决：口径再调时不能靠分数碰巧偏低。
        let r = rule_score_all(
            Some("某某科技有限公司"),
            Some("Java 后端 全栈 服务端"),
            Some("15-35K"),
            Some("10-15年"),
            None,
            &DEFAULT_PROFILE,
        );
        assert!(
            r.score < MIN_SCORE_DEFAULT,
            "无 JD 取满也应低于门槛；实际 {}",
            r.score
        );
        assert!(!r.passes(MIN_SCORE_DEFAULT), "无 JD 一律不该入库");
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

    #[test]
    fn negated_outsourcing_jd_scores_normally() {
        // JD 主动声明「非外包、非驻场」：公司质量 10 分，不加不减
        let jd = format!(
            "本岗位非外包、非驻场，自有产品研发。{}",
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
        assert!(!r.reason.contains("总分减半"), "{}", r.reason);
        // 40 技能 + 10 薪资 + 15 经验 + 15 方向 + 10 公司 = 90
        assert_eq!(r.score, 90);
        assert!(r.passes(MIN_SCORE_DEFAULT));
    }

    #[test]
    fn probation_and_bonus_jd_is_not_short_term() {
        // 「试用期3-6个月」「年终奖2-6个月」是待遇文案，不是项目制
        let jd = format!(
            "{} 试用期3-6个月，年终奖2-6个月。",
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
        assert!(!r.reason.contains("总分减半"), "{}", r.reason);
        assert_eq!(r.score, 90);
        assert!(r.passes(MIN_SCORE_DEFAULT));
    }
}
