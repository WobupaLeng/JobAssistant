//! 统一打分入口（`backend/app/scorer.py:597-669` `Scorer.score` 的平移）。
//!
//! Python 侧那段代码里 `Scorer.score` 被定义了两次，后者覆盖前者——本模块按
//! **后者**（带「规则初筛 X 分」后缀的完整版）实现，因为那是一直在跑的那个。
//!
//! ## 决策顺序（不可重排）
//!
//! 1. 先算规则分（快、免费）
//! 2. 未启用大模型 → 直接用规则分
//! 3. 规则分 < 初筛线 → 直接判掉，**不花 token**
//! 4. 否则调大模型，**只采用大模型的分**
//! 5. 大模型失败 → 回退规则分，流程不断

use crate::core::domain::job::Job;
use crate::core::domain::profile::Profile;
use crate::core::domain::scoring::{rule_score_all, RuleScore};
use crate::core::domain::scoring_llm::{
    decide, score_with_llm, LlmClient, ScoreDecision, LLM_PREFILTER_DEFAULT,
};

/// 最终打分结果（对齐 Python `ScoreResult`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreOutcome {
    /// 0-100 分
    pub score: u8,
    /// `"rule"` 或 `"llm"`
    pub source: &'static str,
    /// 一句话理由
    pub reason: String,
    /// 本次消耗的 token（规则打分为 0）
    pub tokens: u32,
    /// 无 JD 时为 true，即使分数够也不入库
    pub rejected_no_jd: bool,
}

impl ScoreOutcome {
    /// 该岗位是否应当入库。
    pub fn passes(&self, min_score: u8) -> bool {
        !self.rejected_no_jd && self.score >= min_score
    }
}

/// 打分器配置。
#[derive(Debug, Clone)]
pub struct ScoreConfig {
    /// 是否启用大模型
    pub use_llm: bool,
    /// 规则初筛线：规则分低于它的不调大模型（0 = 不设初筛线）
    pub llm_prefilter: u8,
    /// 用户自定义筛选条件，会拼进提示词
    pub filters: String,
}

impl Default for ScoreConfig {
    fn default() -> Self {
        Self {
            // 默认规则打分：快、不花钱。界面上开关打开后才走大模型。
            use_llm: false,
            llm_prefilter: LLM_PREFILTER_DEFAULT,
            filters: String::new(),
        }
    }
}

/// 统一打分入口。
pub fn score_job(
    job: &Job,
    profile: &Profile,
    config: &ScoreConfig,
    llm: Option<&dyn LlmClient>,
) -> ScoreOutcome {
    let rule: RuleScore = rule_score_all_from_job(job, profile);

    // 某些调用方可能传 use_llm=true 但没给客户端，等同未启用
    let llm = match (config.use_llm, llm) {
        (true, Some(client)) => Some(client),
        _ => None,
    };

    // 无 JD 与硬规则否决都是「不花 token 就能定论」的结论：
    // 若先问大模型，一条注定不入库的岗位会白白消耗一次调用。
    // 与 Python 侧的区别是：那里把无 JD 留到最后才拦，这里提前到调用前，
    // 结论不变（`rejected_no_jd` 会覆盖最终分数），只省掉无意义的请求。
    if rule.rejected_no_jd {
        return rule_outcome(rule);
    }

    match decide(llm.is_some(), rule.score, config.llm_prefilter) {
        ScoreDecision::UseRule => rule_outcome(rule),

        ScoreDecision::UseRuleBelowPrefilter {
            rule_score,
            prefilter,
        } => {
            let mut out = rule_outcome(rule);
            out.reason = format!(
                "[规则初筛 {} 分 < {}，未调用大模型] {}",
                rule_score, prefilter, out.reason
            );
            out
        }

        ScoreDecision::UseLlm => {
            let client = match llm {
                Some(c) => c,
                // decide() 已保证此处有客户端；分支仅作防御
                None => return rule_outcome(rule),
            };
            match score_with_llm(client, job, profile, &config.filters) {
                Ok(s) => {
                    // 与 Python 一致：大模型分覆盖规则分，但把规则分写进理由便于对照
                    let reason = if rule.score != s.score {
                        format!("{}（规则初筛 {} 分）", s.reason, rule.score)
                    } else {
                        s.reason
                    };
                    ScoreOutcome {
                        score: s.score,
                        source: "llm",
                        reason,
                        tokens: s.tokens,
                        rejected_no_jd: rule.rejected_no_jd,
                    }
                }
                // 网络/额度/解析失败都回退规则分，保证抓取流程不中断
                Err(e) => {
                    let mut out = rule_outcome(rule);
                    out.reason = format!("{} {}", e.tag(), out.reason);
                    out
                }
            }
        }
    }
}

fn rule_outcome(r: RuleScore) -> ScoreOutcome {
    ScoreOutcome {
        score: r.score,
        source: "rule",
        reason: r.reason,
        tokens: 0,
        rejected_no_jd: r.rejected_no_jd,
    }
}

/// 从 `Job` 取字段调用规则打分（`rule_score_all` 接收的是散字段）。
fn rule_score_all_from_job(job: &Job, profile: &Profile) -> RuleScore {
    rule_score_all(
        job.company.as_deref(),
        job.title.as_deref(),
        job.salary.as_deref(),
        job.experience.as_deref(),
        job.jd_text.as_deref(),
        profile,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::profile::DEFAULT_PROFILE;
    use crate::core::domain::scoring::MIN_SCORE_DEFAULT;
    use crate::core::domain::scoring_llm::LlmError;

    fn ideal_job() -> Job {
        let jd = format!("{} 负责后端服务开发", DEFAULT_PROFILE.core_skills.join(" "));
        let mut j = Job {
            job_id: "j1".into(),
            platform: "boss".into(),
            status: "pending".into(),
            ..Default::default()
        };
        j.title = Some("Java 后端工程师".into());
        j.company = Some("某某科技有限公司".into());
        j.salary = Some("20-35K".into());
        j.experience = Some("5-10年".into());
        j.jd_text = Some(jd);
        j
    }

    struct Fake {
        reply: Result<(String, String, u32), LlmError>,
        calls: std::sync::Mutex<u32>,
    }

    impl Fake {
        fn ok(score: u8) -> Self {
            Self {
                reply: Ok((
                    format!(r#"{{"score": {}, "reason": "模型判断"}}"#, score),
                    "stop".into(),
                    999,
                )),
                calls: std::sync::Mutex::new(0),
            }
        }
        fn fail() -> Self {
            Self {
                reply: Err(LlmError::Transport("超时".into())),
                calls: std::sync::Mutex::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    impl LlmClient for Fake {
        fn complete(
            &self,
            _s: &str,
            _u: &str,
            _m: u32,
        ) -> Result<(String, String, u32), LlmError> {
            *self.calls.lock().unwrap() += 1;
            self.reply.clone()
        }
    }

    #[test]
    fn default_config_uses_rule_only() {
        let fake = Fake::ok(95);
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &ScoreConfig::default(), Some(&fake));
        assert_eq!(out.source, "rule");
        assert_eq!(out.tokens, 0);
        assert_eq!(fake.call_count(), 0, "未启用大模型时不得发请求");
    }

    #[test]
    fn use_llm_false_ignores_client() {
        let fake = Fake::ok(95);
        let cfg = ScoreConfig {
            use_llm: false,
            ..Default::default()
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "rule");
        assert_eq!(fake.call_count(), 0);
    }

    #[test]
    fn below_prefilter_skips_llm_call() {
        let fake = Fake::ok(95);
        // 初筛线设到 101 不可表示，用 100 保证规则分必低于它
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 100,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "rule");
        assert_eq!(fake.call_count(), 0, "低于初筛线不得花 token");
        assert!(out.reason.contains("规则初筛"), "实际 {}", out.reason);
    }

    #[test]
    fn above_prefilter_uses_llm_score() {
        let fake = Fake::ok(95);
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 1,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "llm");
        assert_eq!(out.score, 95);
        assert_eq!(out.tokens, 999);
        assert_eq!(fake.call_count(), 1);
    }

    #[test]
    fn llm_reason_keeps_rule_score_for_comparison() {
        // 取一个与被打分岗位规则分必然不同的分数，验证后缀追加
        let rule_baseline = score_job(&ideal_job(), &DEFAULT_PROFILE, &ScoreConfig::default(), None);
        let llm_score = if rule_baseline.score >= 100 {
            50
        } else {
            rule_baseline.score + 1
        };
        let fake = Fake::ok(llm_score);
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 1,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "llm");
        assert_eq!(out.score, llm_score);
        assert!(out.reason.contains("规则初筛"), "实际 {}", out.reason);
    }

    #[test]
    fn equal_scores_do_not_append_rule_suffix() {
        // 与 Python 一致：模型分与规则分相同就不追加后缀，避免理由冗余
        let rule_baseline = score_job(&ideal_job(), &DEFAULT_PROFILE, &ScoreConfig::default(), None);
        let fake = Fake::ok(rule_baseline.score);
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 1,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "llm");
        assert!(!out.reason.contains("规则初筛"), "实际 {}", out.reason);
    }

    #[test]
    fn llm_failure_falls_back_to_rule() {
        let fake = Fake::fail();
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 1,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert_eq!(out.source, "rule", "失败必须回退，不能中断流程");
        assert!(out.reason.contains("大模型失败"), "实际 {}", out.reason);
        assert!(out.passes(MIN_SCORE_DEFAULT));
    }

    #[test]
    fn no_client_with_use_llm_falls_back_to_rule() {
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 1,
            filters: String::new(),
        };
        let out = score_job(&ideal_job(), &DEFAULT_PROFILE, &cfg, None);
        assert_eq!(out.source, "rule");
    }

    #[test]
    fn no_jd_is_rejected_regardless_of_score() {
        let mut j = ideal_job();
        j.jd_text = None;
        let out = score_job(&j, &DEFAULT_PROFILE, &ScoreConfig::default(), None);
        assert!(out.rejected_no_jd);
        assert!(!out.passes(MIN_SCORE_DEFAULT), "无 JD 一律不入库");
    }

    #[test]
    fn no_jd_flag_survives_llm_path() {
        // 即使大模型给了高分，无 JD 的显式否决也必须保留
        let mut j = ideal_job();
        j.jd_text = None;
        let fake = Fake::ok(99);
        let cfg = ScoreConfig {
            use_llm: true,
            llm_prefilter: 0,
            filters: String::new(),
        };
        let out = score_job(&j, &DEFAULT_PROFILE, &cfg, Some(&fake));
        assert!(out.rejected_no_jd);
        assert!(!out.passes(MIN_SCORE_DEFAULT));
        // 无 JD 是不花 token 就能定论的结论，必须在大模型调用之前就返回
        assert_eq!(fake.call_count(), 0, "无 JD 岗位不该消耗大模型 token");
    }
}
