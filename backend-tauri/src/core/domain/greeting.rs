//! 打招呼语的生成：给某个岗位写一句发给 HR 的话。
//!
//! 两条数据来源：
//! 1. 用户在「配置 → 打招呼」里写的提示词——决定语气、长度、强调什么；
//! 2. 岗位本身（标题 / 公司 / 薪资 / 经验 / JD）。
//!
//! 再叠加「我掌握的技能」：优先用与岗位 JD 真重合的那几项，保证话术是这个岗位
//! 看得懂的，而不是一份通用模板。
//!
//! **大模型不是必需**。没配 key、网络失败、返回空内容时一律退回规则话术，
//! 并在 `note` 里写明原因——界面上留白比降级更糟（用户不知道是没生成还是还没点）。
//!
//! 与 `greeting.py` 的对应关系：本模块是它 214 行的 Rust 平移，改动处都在注释里
//! 标了「与 Python 不同」。平移它的理由不是"重写癖"，而是老版把这些规则埋在
//! FastAPI 路由函数里，无法单测；现在 `matched_skills` / `rule_greeting` 全是纯函数。

use crate::core::domain::scoring_llm::{LlmClient, LlmError};
use crate::core::domain::job::Job;

/// 默认提示词：用户不改也能直接生成。
pub const DEFAULT_PROMPT: &str = "你是求职者的助理，要根据岗位 JD 帮他写一句发给 HR 的打招呼消息。\n\
要求：\n\
1. 只说与这个岗位最匹配的 2~3 项技能或经历，不要罗列全部简历；\n\
2. 语气自然、礼貌，像真人发的第一句话，不要客套套话堆砌；\n\
3. 控制在 60~120 字，一到两句话，不要分点、不要换行、不要加称呼以外的废话；\n\
4. 直接输出消息正文，不要任何解释、前缀、引号或 markdown。";

/// 常见技能词表：用于在「我的技能一个都没命中」时，从 JD 里捞出岗位在意什么。
///
/// 顺序即优先级——`matched_skills` 的兜底分支按这个顺序取前几个。
const SKILL_WORDS: &[&str] = &[
    "Java",
    "Python",
    "Go",
    "Golang",
    "Node",
    "Node.js",
    "C++",
    "C#",
    "PHP",
    "JavaScript",
    "TypeScript",
    "Vue",
    "React",
    "Angular",
    "uni-app",
    "Electron",
    "Spring",
    "Spring Boot",
    "Spring Cloud",
    "SpringBoot",
    "MyBatis",
    "MyBatis-Plus",
    "MySQL",
    "Oracle",
    "PostgreSQL",
    "SQL Server",
    "MongoDB",
    "Redis",
    "Elasticsearch",
    "RabbitMQ",
    "Kafka",
    "RocketMQ",
    "Nginx",
    "Docker",
    "Kubernetes",
    "K8s",
    "Linux",
    "Git",
    "Jenkins",
    "CI/CD",
    "微服务",
    "分布式",
    "高并发",
    "多线程",
    "大模型",
    "LLM",
    "AIGC",
    "RAG",
    "Agent",
    "Prompt",
    "LangChain",
    "人工智能",
    "机器学习",
    "深度学习",
    "向量",
    "知识库",
    "智能客服",
    "全栈",
    "小程序",
    "H5",
    "低代码",
    "SaaS",
    "多租户",
    "支付",
    "对账",
    "数据采集",
    "爬虫",
    "自动化",
    "运维",
    "性能优化",
    "重构",
    "架构",
];

/// 一句话术 + 它的来历。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Greeting {
    pub text: String,
    /// 本次消耗的 token（规则话术恒为 0）
    pub tokens: u32,
    /// `"llm"` 或 `"rule"`
    pub source: String,
    /// 降级原因。为空表示没有降级。
    pub note: String,
}

/// 生成话术所需的「我是谁」。
///
/// 不从 `DEFAULT_PROFILE` 直接读：那份画像是编译期常量，用户在配置里手填的
/// 技能与年限必须能覆盖它，否则「配置 → 技能」这个输入框就成了摆设。
#[derive(Debug, Clone, Default)]
pub struct GreetingProfile {
    /// 工作年限（用于话术里的「有 N 年经验」）
    pub years: u32,
    /// 期望方向（用于 prompt 里的自我介绍）
    pub title: String,
    /// 手填技能，来自配置 `skills`；为空时退回画像自带的技能
    pub skills: Vec<String>,
    /// 画像自带的核心技能（`DEFAULT_PROFILE.core_skills`）
    pub core_skills: Vec<String>,
    /// 画像自带的加分技能（`DEFAULT_PROFILE.bonus_skills`）
    pub bonus_skills: Vec<String>,
}

impl GreetingProfile {
    /// 「我的技能」：手填的优先，其次核心，再次加分；去重且保持顺序。
    pub fn all_skills(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in self
            .skills
            .iter()
            .chain(self.core_skills.iter())
            .chain(self.bonus_skills.iter())
        {
            let t = s.trim();
            if !t.is_empty() && !out.iter().any(|x| x == t) {
                out.push(t.to_string());
            }
        }
        out
    }
}

/// `years` 为 0 时的说法。
///
/// 与 Python 不同：老版直接 `f"有{years}年经验"`，画像年限为空时会写出
/// 「有 0 年经验」这种自曝短板的话术。这里整段去掉。
fn years_phrase(years: u32) -> String {
    if years == 0 {
        String::new()
    } else {
        format!("有{years}年经验")
    }
}

/// 岗位 JD 里和我技能重合的部分（按我的技能顺序取，便于话术自然）。
pub fn matched_skills(job: &Job, profile: &GreetingProfile) -> Vec<String> {
    let mine = profile.all_skills();
    let blob = format!(
        "{} {} {} {}",
        job.title.clone().unwrap_or_default(),
        job.jd_text.clone().unwrap_or_default(),
        job.company_intro.clone().unwrap_or_default(),
        job.company_industry.clone().unwrap_or_default()
    )
    .to_lowercase();
    if blob.trim().is_empty() {
        return mine.into_iter().take(3).collect();
    }
    let hit: Vec<String> = mine
        .iter()
        .filter(|s| blob.contains(&s.to_lowercase()))
        .cloned()
        .collect();
    if !hit.is_empty() {
        return hit.into_iter().take(6).collect();
    }
    // 一个都没命中：退而求其次，用 JD 里出现过的通用技能词
    let found: Vec<String> = SKILL_WORDS
        .iter()
        .filter(|w| blob.contains(&w.to_lowercase()))
        .map(|w| w.to_string())
        .collect();
    if found.is_empty() {
        mine.into_iter().take(3).collect()
    } else {
        found.into_iter().take(4).collect()
    }
}

/// 构造 (system, user)。system 用用户提示词，user 装岗位与技能事实。
pub fn build_prompt(user_prompt: Option<&str>, job: &Job, profile: &GreetingProfile) -> (String, String) {
    let p = user_prompt.map(|s| s.trim()).unwrap_or("");
    let system = if p.is_empty() { DEFAULT_PROMPT } else { p }.to_string();

    let skills = matched_skills(job, profile);
    let mut jd = collapse_ws(job.jd_text.as_deref().unwrap_or(""));
    if jd.chars().count() > 1200 {
        jd = format!("{}…", jd.chars().take(1200).collect::<String>());
    }

    let mut facts: Vec<String> = Vec::new();
    let years = if profile.years == 0 {
        String::new()
    } else {
        format!("{} 年经验", profile.years)
    };
    facts.push(format!("求职者：{}，方向 {}", years, profile.title).replace("：，", "："));
    let mine = profile.all_skills();
    facts.push(format!(
        "掌握的技能：{}",
        if mine.is_empty() {
            "（未填写）".to_string()
        } else {
            mine.join("、")
        }
    ));
    facts.push(format!(
        "与该岗位匹配上的技能：{}",
        if skills.is_empty() {
            "（未直接命中，按上面的技能概括）".to_string()
        } else {
            skills.join("、")
        }
    ));
    facts.push(format!(
        "岗位标题：{}",
        job.title.clone().unwrap_or_default()
    ));
    facts.push(format!("公司：{}", job.company.clone().unwrap_or_default()));
    facts.push(format!(
        "薪资：{}",
        job.salary.clone().unwrap_or_else(|| "未标注".to_string())
    ));
    facts.push(format!(
        "经验要求：{}",
        job.experience.clone().unwrap_or_else(|| "未标注".to_string())
    ));
    if job.jd_text.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
        facts.push(format!("岗位详情（截断）：{jd}"));
    } else {
        facts.push("岗位详情：无（只能根据标题与公司信息写）".to_string());
    }
    (system, facts.join("\n"))
}

/// 把连续空白压成一个空格（含全角空格）。
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '\u{3000}' {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out.trim().to_string()
}

/// 清掉模型偶尔加的包装：```markdown 围栏、成对引号、第一行之后的内容。
pub fn clean(text: &str) -> String {
    let mut t = text.trim().to_string();
    // ```xxx ... ```
    if t.starts_with("```") {
        if let Some(nl) = t.find('\n') {
            t = t[nl + 1..].to_string();
        }
    }
    if t.trim_end().ends_with("```") {
        let trimmed = t.trim_end();
        t = trimmed[..trimmed.len() - 3].to_string();
    }
    let t = t
        .trim()
        .trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '“' | '”' | '‘' | '’' | '「' | '」' | '『' | '』'
            )
        })
        .trim()
        .to_string();
    t.lines().next().unwrap_or("").trim().to_string()
}

/// 大模型不可用时的兜底话术：技能 + 岗位名，尽量不像模板。
pub fn rule_greeting(job: &Job, profile: &GreetingProfile) -> String {
    let skills: Vec<String> = matched_skills(job, profile).into_iter().take(3).collect();
    let title = job.title.clone().unwrap_or_default();
    let title = title.trim();
    let tech = if skills.is_empty() {
        "后端与全栈".to_string()
    } else {
        skills.join("、")
    };
    let years = years_phrase(profile.years);
    if title.is_empty() {
        return format!(
            "您好，我在{tech}方向{years}，看到贵司有合适的岗位，想和您聊聊，方便发一份简历给您吗？"
        );
    }
    format!(
        "您好，我在{tech}方向{years}，看到贵司在招「{title}」，和我的技术栈比较匹配，想和您聊聊岗位细节，方便发一份简历给您吗？"
    )
}

/// 把「空内容」翻译成人能看懂的失败原因。
///
/// 尤其是推理型模型的思考挤占预算：实测 max_tokens=400 时
/// `reasoning_tokens` 可以正好等于 400，content 为空、finish_reason=length。
pub fn empty_content_reason(finish: &str) -> String {
    if finish == "length" {
        return "大模型返回了空内容（输出预算被思考过程耗尽；可在设置里换非推理模型，或调大最大输出）"
            .to_string();
    }
    format!("大模型返回了空内容（finish={}）", if finish.is_empty() { "?" } else { finish })
}

/// 生成结果 + 是否降级。
pub struct GreetingOutcome {
    pub greeting: Greeting,
}

/// 输出预算的两档尝试。
///
/// 推理型模型会先写 reasoning_content，思考 token 也算进 max_tokens。原来给 400 时
/// 思考就吃光了整个预算 → content 为空、finish=length。先给 1200，仍被截断再加到 3000。
const BUDGETS: [u32; 2] = [1200, 3000];

/// 用大模型写一句话术；失败一律降级到规则话术并给出原因。
///
/// `llm` 为 `None` 表示没配 key——这时**不报错**，直接走规则话术，
/// 因为「没配大模型」是合法状态，不是故障。
pub fn write_greeting(
    llm: Option<&dyn LlmClient>,
    user_prompt: Option<&str>,
    job: &Job,
    profile: &GreetingProfile,
) -> Greeting {
    let Some(client) = llm else {
        return Greeting {
            text: rule_greeting(job, profile),
            tokens: 0,
            source: "rule".to_string(),
            note: "未配置大模型，使用规则话术".to_string(),
        };
    };

    let (system, user) = build_prompt(user_prompt, job, profile);
    let mut last_finish = String::new();
    let mut last_err: Option<String> = None;

    for budget in BUDGETS {
        match client.complete(&system, &user, budget) {
            Ok((content, finish, tokens)) => {
                let text = clean(&content);
                if !text.is_empty() {
                    return Greeting {
                        text,
                        tokens,
                        source: "llm".to_string(),
                        note: String::new(),
                    };
                }
                last_finish = finish;
                // 没有正文，且不是被预算截断 → 模型确实什么都没写，重试也没用
                if last_finish != "length" {
                    break;
                }
            }
            // 模型回了但正文空：`Unparsable` 带 finish_reason。
            // 若被预算截断就换更大预算再试一次，否则重试无意义。
            Err(LlmError::Unparsable { finish_reason, .. }) => {
                last_finish = finish_reason;
                if last_finish != "length" {
                    break;
                }
            }
            Err(e) => {
                last_err = Some(e.tag());
                break;
            }
        }
    }

    let reason = match last_err {
        Some(e) => format!("大模型生成失败（{e}），已用规则话术代替"),
        None => format!(
            "{}，已用规则话术代替",
            empty_content_reason(&last_finish)
        ),
    };
    Greeting {
        text: rule_greeting(job, profile),
        tokens: 0,
        source: "rule".to_string(),
        note: reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个最小岗位。只有标题/公司/JD 参与话术，其余留空，
    /// 免得每个用例都写 30 行字段。
    fn job(title: &str, jd: &str) -> Job {
        Job {
            job_id: "j1".into(),
            platform: "boss".into(),
            title: Some(title.into()),
            company: Some("某某科技".into()),
            salary: Some("20-40K".into()),
            status: "pending".into(),
            created_at: 0,
            jd_text: if jd.is_empty() { None } else { Some(jd.into()) },
            experience: Some("5-10年".into()),
            rejected: 0,
            ..Default::default()
        }
    }

    fn profile() -> GreetingProfile {
        GreetingProfile {
            years: 10,
            title: "Java 后端 / 全栈工程师".into(),
            skills: vec!["Java".into(), "Spring Boot".into(), "MySQL".into()],
            core_skills: vec!["Redis".into()],
            bonus_skills: vec![],
        }
    }

    /// 技能去重且顺序为「手填 → 核心 → 加分」：话术里同一技能出现两次很像机器。
    #[test]
    fn all_skills_dedups_in_priority_order() {
        let p = GreetingProfile {
            skills: vec!["Java".into(), "Redis".into(), "  ".into()],
            core_skills: vec!["Redis".into(), "Kafka".into()],
            bonus_skills: vec!["Kafka".into()],
            ..Default::default()
        };
        assert_eq!(p.all_skills(), vec!["Java", "Redis", "Kafka"]);
    }

    /// JD 命中的技能优先于「我的全部技能」——话术要说岗位听得懂的。
    #[test]
    fn matched_skills_prefer_jd_hits() {
        let j = job("Java 后端", "熟悉 Spring Boot 与 MySQL，了解 Redis");
        let m = matched_skills(&j, &profile());
        assert!(m.contains(&"Spring Boot".to_string()));
        assert!(m.contains(&"MySQL".to_string()));
        // Redis 属于核心技能，也命中 JD
        assert!(m.contains(&"Redis".to_string()));
    }

    /// 一个都不命中时退回通用词表兜底，而不是返回空数组。
    #[test]
    fn matched_skills_falls_back_to_known_words() {
        let p = GreetingProfile {
            skills: vec!["COBOL".into()],
            ..Default::default()
        };
        let j = job("大模型工程师", "负责 AIGC 与 RAG 落地，熟悉 Docker");
        let m = matched_skills(&j, &p);
        assert!(!m.is_empty(), "一个技能没命中时必须有兜底，否则话术会变成通用模板");
        assert!(m.iter().any(|s| s == "AIGC" || s == "RAG" || s == "Docker"));
    }

    /// 规则话术必须带上岗位名与技能，否则等于没生成。
    #[test]
    fn rule_greeting_names_skill_and_title() {
        let j = job("Java 后端开发工程师", "熟悉 Spring Boot");
        let g = rule_greeting(&j, &profile());
        assert!(g.contains("Java 后端开发工程师"));
        assert!(g.contains("Spring Boot"));
        assert!(g.contains("10年"));
    }

    /// 年限为 0 时不能写出「有0年经验」——那是自曝短板。
    #[test]
    fn rule_greeting_omits_zero_years() {
        let p = GreetingProfile {
            years: 0,
            ..profile()
        };
        let g = rule_greeting(&job("Java", ""), &p);
        assert!(!g.contains("0年"), "年限缺失时不该出现 0 年：{g}");
    }

    /// 模型爱加 ``` 围栏与引号，清理后必须只剩正文一句话。
    #[test]
    fn clean_strips_fence_quotes_and_extra_lines() {
        assert_eq!(clean("```markdown\n您好，我有 10 年经验。\n```"), "您好，我有 10 年经验。");
        assert_eq!(clean("“您好，想聊聊”"), "您好，想聊聊");
        assert_eq!(clean("第一行\n第二行"), "第一行");
        assert_eq!(clean("  「您好」  "), "您好");
    }

    /// 没配大模型是合法状态：走规则话术，但 note 要说明原因，界面不能留白。
    #[test]
    fn no_llm_degrades_to_rule_with_note() {
        let g = write_greeting(None, None, &job("Java", ""), &profile());
        assert_eq!(g.source, "rule");
        assert_eq!(g.tokens, 0);
        assert!(!g.note.is_empty());
        assert!(g.text.contains("Java"));
    }

    /// prompt 必须装满事实：标题/公司/薪资/经验，缺一样模型就得瞎编。
    #[test]
    fn build_prompt_carries_job_facts() {
        let j = job("Java 后端", "熟悉 Spring Boot");
        let (system, user) = build_prompt(None, &j, &profile());
        assert_eq!(system, DEFAULT_PROMPT);
        assert!(user.contains("Java 后端"));
        assert!(user.contains("某某科技"));
        assert!(user.contains("20-40K"));
        assert!(user.contains("5-10年"));
        assert!(user.contains("10 年经验"));
    }

    /// 用户自己写的提示词要覆盖默认提示词，空字符串则应回落到默认。
    #[test]
    fn custom_prompt_overrides_default() {
        let j = job("Java", "");
        let (system, _) = build_prompt(Some("  活泼一点  "), &j, &profile());
        assert_eq!(system, "活泼一点");
        let (system, _) = build_prompt(Some("   "), &j, &profile());
        assert_eq!(system, DEFAULT_PROMPT);
    }

    /// 空内容的原因要说人话，并点出「预算被思考耗尽」这个真实病因。
    #[test]
    fn empty_reason_explains_budget_exhaustion() {
        let r = empty_content_reason("length");
        assert!(r.contains("预算"));
        assert!(empty_content_reason("").contains("finish=?"));
    }
}
