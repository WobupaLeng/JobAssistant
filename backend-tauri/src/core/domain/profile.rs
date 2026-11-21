//! 求职者画像（`backend/app/scorer.py:18 PROFILE` 的平移）。
//!
//! 目前是硬编码常量，与 Python 侧一致。后续接入 `app_config` 表后
//! 应改为从存储读取——这里保留为默认值，避免一次性改动面过大。

/// 求职者画像。字段名与 Python `PROFILE` 一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// 工作年限
    pub years: u32,
    /// 期望岗位标题（用于 LLM prompt，不参与规则打分）
    pub title: &'static str,
    /// 期望薪资下限（K）
    pub salary_low: u32,
    /// 期望薪资上限（K）
    pub salary_high: u32,
    /// 期望城市（用于 LLM prompt）
    pub city: &'static str,
    /// 核心技能：命中每个 +1.0
    pub core_skills: &'static [&'static str],
    /// 加分技能：命中每个 +0.5
    pub bonus_skills: &'static [&'static str],
}

impl Profile {
    /// 薪资跨度的分母（`salary_high - salary_low`）。
    ///
    /// 单独抽出来是因为 Python 侧 `_salary` 里 `span` 参与除法，
    /// 且要求 `salary_high > salary_low`，否则会除零。
    pub fn salary_span(&self) -> u32 {
        self.salary_high.saturating_sub(self.salary_low)
    }
}

/// 默认画像，与 `scorer.py:18` 完全一致。
pub const DEFAULT_PROFILE: Profile = Profile {
    years: 10,
    title: "Java 后端 / 全栈工程师",
    salary_low: 15,
    salary_high: 35,
    city: "深圳",
    core_skills: &[
        "Java",
        "Spring Cloud",
        "MyBatis",
        "MySQL",
        "Redis",
        "RabbitMQ",
    ],
    bonus_skills: &[
        "Vue",
        // Html 属「会一点前端」，放加分项而非核心项：
        // core 每项 +1.0，放到 core 里会让纯静态页面岗位也拿满技能分。
        "Html",
        "Python",
        "CSS/JS",
        "Electron",
        "uni-app",
        "GO",
        "Docker",
        "Linux",
        "Git",
        "Kafka",
        "SpringBoot",
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_matches_python() {
        // 画像数值变动会直接改变所有岗位分数，必须锁死。
        // 注意：skills 的「内容」允许用户按需调整，这里只锁结构不锁具体技能名。
        assert_eq!(DEFAULT_PROFILE.years, 10);
        assert_eq!(DEFAULT_PROFILE.salary_low, 15);
        assert_eq!(DEFAULT_PROFILE.salary_high, 35);
        assert_eq!(DEFAULT_PROFILE.core_skills.len(), 6);
        assert_eq!(DEFAULT_PROFILE.bonus_skills.len(), 12);
    }

    #[test]
    fn profile_skills_have_no_duplicates() {
        // core / bonus 内部及交叉重复都会让同一技能被计两次分，
        // 而 skills_score 只统计命中项，重复项是纯噪声。
        let mut all: Vec<&str> = DEFAULT_PROFILE
            .core_skills
            .iter()
            .chain(DEFAULT_PROFILE.bonus_skills.iter())
            .copied()
            .collect();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total, "画像技能列表存在重复项");
    }

    #[test]
    fn salary_span_is_20() {
        assert_eq!(DEFAULT_PROFILE.salary_span(), 20);
    }

    #[test]
    fn salary_span_never_divides_by_zero() {
        let mut p = DEFAULT_PROFILE;
        p.salary_low = 35;
        p.salary_high = 15;
        assert_eq!(p.salary_span(), 0);
    }
}
