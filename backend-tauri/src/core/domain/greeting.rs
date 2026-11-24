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
///
/// 第 2 条是硬约束而非风格建议：早先只写「说最匹配的 2~3 项技能」，模型就把
/// **岗位 JD 里的技术**说成求职者会的（事实错误），问一次「你会 K8s 吗」就露馅。
pub const DEFAULT_PROMPT: &str = "你是求职者的助理，要根据岗位 JD 帮他写一句发给 HR 的打招呼消息。\n\
要求：\n\
1. 技能**只能**从「你确实会的技能」里挑 2~3 项，优先挑与岗位要求有交集的那几项；\
不得声称掌握该清单以外的任何技术；\n\
2. 「岗位要求」只是参考，用来挑重点——里面的技术你没在清单里写过，就不许说会；\n\
3. 技能名同义、缩写、大小写差异都算同一项（如 SpringBoot＝Spring Boot、k8s＝Kubernetes），\
不要因为写法不同就当成两项；\n\
4. 若给了「HR 刚说的话」，要针对那句话回应（他问什么就答什么），另加这句话本身的需求；\n\
5. 语气自然、礼貌，像真人发的第一句话，不要客套套话堆砌；\n\
6. 控制在 60~120 字，一到两句话，不要分点、不要换行、不要加称呼以外的废话；\n\
7. 直接输出消息正文，不要任何解释、前缀、引号或 markdown。";

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

/// 技能词表里常见的同义写法。
///
/// 只在 `matched_skills` 里用：用户写「Spring Cloud」、JD 写「SpringCloud」，
/// 或者反过来，都应当算命中。**不参与提示词**，只用来判断「JD 提没提这一项」。
const SKILL_ALIASES: &[(&str, &[&str])] = &[
    ("spring cloud", &["springcloud", "spring-cloud", "微服务框架"]),
    ("spring boot", &["springboot", "spring-boot"]),
    ("k8s", &["kubernetes"]),
    ("kubernetes", &["k8s"]),
    ("node.js", &["nodejs", "node"]),
    ("nodejs", &["node.js", "node"]),
    ("mybatis-plus", &["mybatisplus", "mybatis plus"]),
    ("rag", &["检索增强", "知识库检索", "向量检索"]),
    ("检索增强", &["rag"]),
    ("ai 客服", &["智能客服", "ai客服", "在线客服机器人"]),
    ("ai客服", &["ai 客服", "智能客服"]),
    ("智能客服", &["ai 客服", "ai客服"]),
    ("大模型", &["llm", "大语言模型", "gpt", "llm api"]),
    ("llm", &["大模型", "大语言模型"]),
    ("大模型 api", &["openai api", "llm api", "大模型接口", "api 调用"]),
    ("多轮上下文", &["多轮对话", "上下文管理", "会话记忆"]),
    ("多轮对话", &["多轮上下文"]),
    ("异步任务", &["消息队列", "任务队列", "celery", "异步处理"]),
    ("多租户", &["saas", "多租户架构"]),
    ("saas", &["多租户"]),
    ("ecmascript", &["es6", "es2015"]),
    ("vue", &["vue3", "vue2", "vue.js"]),
    ("vue.js", &["vue", "vue3", "vue2"]),
    ("小程序", &["uni-app", "uniapp", "微信小程序"]),
    ("uni-app", &["uniapp", "小程序"]),
    ("docker", &["容器化", "容器技术"]),
    ("git", &["gitlab", "版本控制"]),
    ("mysql", &["关系型数据库", "sql 数据库"]),
    ("redis", &["缓存"]),
    ("echarts", &["数据可视化", "图表"]),
    ("laravel", &["php 框架"]),
];

/// 判断 JD 里有没有提到某一项技能。
///
/// 规则（按序，任一命中也算）：
/// 1. 归一化后字面包含（去掉空白、转小写）；
/// 2. 拆斜杠后的每一段都出现（用户写 `Linux/Docker/Git` 且 JD 里三样都有）；
/// 3. 斜杠段至少一半出现（用户把几项写成一个组合时常见的写法）；
/// 4. 同义词命中（`SKILL_ALIASES`）。
///
/// **不再往返回里塞 JD 里的通用词**——那会把「岗位要什么」当成「我会什么」，
/// 提示词里写成「匹配上的技能」后模型就会替你吹牛（见 `matched_skills` 注释）。
pub fn skill_hits(blob_lower: &str, skill: &str) -> bool {
    let s = normalize(skill);
    if s.is_empty() {
        return false;
    }
    if blob_lower.contains(&s) {
        return true;
    }
    // 组合写法：`Linux/Docker/Git`、`Vue/JS`。全中优先，其次过半。
    let parts: Vec<String> = skill
        .split(|c: char| matches!(c, '/' | '\\' | '|' | '+' | '＋'))
        .map(normalize)
        .filter(|p| p.chars().count() >= 2)
        .collect();
    if parts.len() >= 2 {
        let hit = parts.iter().filter(|p| blob_lower.contains(p.as_str())).count();
        if hit == parts.len() || hit * 2 >= parts.len() {
            return true;
        }
    }
    for (key, aliases) in SKILL_ALIASES {
        let k = normalize(key);
        let self_hit = s == k || k.contains(&s) || s.contains(&k);
        if !self_hit {
            continue;
        }
        if aliases.iter().any(|a| blob_lower.contains(&normalize(a))) {
            return true;
        }
    }
    false
}

/// 归一化：小写 + 去掉所有空白（含全角空格），把全角括号/斜杠/加号折成半角。
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '\u{3000}' {
            continue;
        }
        let ch = match ch {
            '（' => '(',
            '）' => ')',
            '／' => '/',
            '＋' | '﹢' => '+',
            other => other,
        };
        out.extend(ch.to_lowercase());
    }
    out
}

/// 岗位 JD 里和我技能重合的部分（按我的技能顺序取，便于话术自然），**只用我自己的写法**。
///
/// 这里曾经在「一个都没命中」时退回 [`SKILL_WORDS`]，从 JD 里捞出岗位在乎的技术。
/// 那等于把「岗位要什么」当成「我会什么」写进提示词（「匹配上的技能：K8s、高并发」），
/// 模型据此替你宣称熟悉这些技术——这是事实错误，不是措辞问题。
/// 现在只做退一步的事：返回我的技能里最靠前的几项（不假装它们命中了 JD）。
pub fn matched_skills(job: &Job, profile: &GreetingProfile) -> Vec<String> {
    let mine = profile.all_skills();
    let blob = skill_blob(job);
    if blob.trim().is_empty() {
        return mine.into_iter().take(3).collect();
    }
    let hit: Vec<String> = mine
        .iter()
        .filter(|s| skill_hits(&blob, s))
        .cloned()
        .collect();
    if !hit.is_empty() {
        return hit.into_iter().take(6).collect();
    }
    mine.into_iter().take(3).collect()
}

/// JD 里提到、而我还**没写进技能表**的技能词：只在提示词里当「岗位要求」用，不许当成「我会」。
pub fn job_skill_words(job: &Job, profile: &GreetingProfile) -> Vec<String> {
    let blob = skill_blob(job);
    if blob.trim().is_empty() {
        return Vec::new();
    }
    let mine = profile.all_skills();
    let mut out: Vec<String> = Vec::new();
    for w in SKILL_WORDS {
        let n = normalize(w);
        if n.chars().count() < 2 || !blob.contains(&n) {
            continue;
        }
        // 我已经有的技能（含别名）不再作为「岗位要求」重复出现。
        if mine.iter().any(|m| skill_hits(&blob, m) && normalize(m).contains(&n)) {
            continue;
        }
        if !out.iter().any(|x: &String| normalize(x) == n) {
            out.push(w.to_string());
        }
    }
    out.into_iter().take(8).collect()
}

/// 拼「岗位里出现的全部文本」并归一化——标题、JD、公司介绍、行业。
fn skill_blob(job: &Job) -> String {
    normalize(&format!(
        "{} {} {} {}",
        job.title.clone().unwrap_or_default(),
        job.jd_text.clone().unwrap_or_default(),
        job.company_intro.clone().unwrap_or_default(),
        job.company_industry.clone().unwrap_or_default()
    ))
}

/// 构造 (system, user)。system 用用户提示词，user 装岗位与技能事实。
pub fn build_prompt(user_prompt: Option<&str>, job: &Job, profile: &GreetingProfile) -> (String, String) {
    build_prompt_with(user_prompt, job, profile, None)
}

/// 带「HR 刚说的话」的版本：用于「根据 HR 回复生成一句话」。
///
/// `reply` 为空等价于 [`build_prompt`]。有回复时在事实里加一段「HR 刚说的话」，
/// system 末尾再补一条针对性的约束——用户自定义提示词时也要带上，
/// 否则用户写「活泼一点」就会把默认约束整段顶掉，模型又开始自由发挥。
pub fn build_prompt_with(
    user_prompt: Option<&str>,
    job: &Job,
    profile: &GreetingProfile,
    reply: Option<&str>,
) -> (String, String) {
    let p = user_prompt.map(|s| s.trim()).unwrap_or("");
    let mut system = if p.is_empty() { DEFAULT_PROMPT } else { p }.to_string();
    let reply = reply.map(collapse_ws).unwrap_or_default();
    if !reply.is_empty() {
        system.push_str(
            "\n补充要求（会话中已收到 HR 的回复）：\n\
             - 这是**接话**，不是重新自我介绍：不要重复寒暄与「您好我做过 N 年…」的完整自述；\n\
             - 先回应 HR 刚说的那句话（他问了什么就答什么），再顺势提 1~2 项与岗位相关的技能；\n\
             - 技能仍然只能来自「你确实会的技能」，HR 提到的技术你没写过就不许说会。",
        );
    }

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
        "你确实会的技能（只能从这里挑，此外一律不许说会）：{}",
        if mine.is_empty() {
            "（未填写）".to_string()
        } else {
            mine.join("、")
        }
    ));
    let skills = matched_skills(job, profile);
    let job_words = job_skill_words(job, profile);
    facts.push(format!(
        "其中与岗位要求有交集的技能：{}",
        if skills.is_empty() {
            "（未直接命中，可从上面的技能里挑最相关的）".to_string()
        } else {
            skills.join("、")
        }
    ));
    if !job_words.is_empty() {
        facts.push(format!(
            "岗位要求/涉及的技术（我方未声明掌握，只作参考，禁用来自称）：{}",
            job_words.join("、")
        ));
    }
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
    if !reply.is_empty() {
        facts.push(format!("HR 刚说的话：{reply}"));
    }
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
    rule_greeting_with(job, profile, "")
}

/// [`rule_greeting`] 的「接 HR 回复」版本：有回复时换成接话句式，不再重复寒暄与自我介绍。
pub fn rule_greeting_with(job: &Job, profile: &GreetingProfile, reply: &str) -> String {
    let skills: Vec<String> = matched_skills(job, profile).into_iter().take(3).collect();
    let title = job.title.clone().unwrap_or_default();
    let title = title.trim();
    let tech = if skills.is_empty() {
        "后端与全栈".to_string()
    } else {
        skills.join("、")
    };
    let years = years_phrase(profile.years);
    if !reply.trim().is_empty() {
        // 规则兜底读不懂 HR 说了什么，只保证「不是又一遍开场白」，并把选择权交回用户。
        return if title.is_empty() {
            format!("收到您的回复，我这边{tech}相关的经验可以对得上，方便约个时间细聊吗？")
        } else {
            format!(
                "收到您的回复，我这边{tech}方向和「{title}」比较匹配，方便约个时间细聊吗？"
            )
        };
    }
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
    write_greeting_with(llm, user_prompt, job, profile, None)
}

/// [`write_greeting`] 的「接 HR 回复」版本：`reply` 非空时按 `build_prompt_with` 构造提示词。
pub fn write_greeting_with(
    llm: Option<&dyn LlmClient>,
    user_prompt: Option<&str>,
    job: &Job,
    profile: &GreetingProfile,
    reply: Option<&str>,
) -> Greeting {
    let reply = reply.map(|r| r.trim()).unwrap_or("");
    let Some(client) = llm else {
        return Greeting {
            text: rule_greeting_with(job, profile, reply),
            tokens: 0,
            source: "rule".to_string(),
            note: "未配置大模型，使用规则话术".to_string(),
        };
    };

    let (system, user) = build_prompt_with(user_prompt, job, profile, Some(reply));
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
        text: rule_greeting_with(job, profile, reply),
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

    /// 一个都不命中时**不许**把 JD 里的技术说成「我的技能」：只能从我的技能里挑。
    ///
    /// 这条是事实正确性约束：早先兜底返回 JD 出现过的通用词（K8s、高并发…），
    /// 提示词再写成「与该岗位匹配上的技能」，模型就替你宣称会这些技术。
    #[test]
    fn matched_skills_never_borrows_jd_words() {
        let p = GreetingProfile {
            skills: vec!["COBOL".into()],
            ..Default::default()
        };
        let j = job("大模型工程师", "负责 AIGC 与 RAG 落地，熟悉 Docker");
        let m = matched_skills(&j, &p);
        assert!(!m.is_empty(), "没命中时也要给出「我的技能」最靠前的几项");
        assert!(!m.iter().any(|s| s == "AIGC" || s == "RAG" || s == "Docker"),
            "JD 里的技术不得冒充我的技能：{m:?}");
        assert!(m.iter().all(|s| s == "COBOL"));
    }

    /// 组合写法与别名：用户按人写「Spring Boot/Cloud」「Linux/Docker/Git」，
    /// JD 写「SpringCloud」也能命中——纯字符串包含永远命不中，这正是话术跑偏的根因。
    #[test]
    fn matched_skills_understands_aliases_and_slash_groups() {
        let p = GreetingProfile {
            skills: vec![
                "Java".into(),
                "Spring Boot/Cloud".into(),
                "Linux/Docker/Git".into(),
                "AI 客服".into(),
            ],
            ..Default::default()
        };
        let j = job("后端", "要求 Java 基础扎实，熟悉 SpringCloud 微服务与消息队列，有智能客服项目经验");
        let m = matched_skills(&j, &p);
        assert!(m.contains(&"Java".to_string()), "{m:?}");
        assert!(m.contains(&"Spring Boot/Cloud".to_string()), "别名缺失：{m:?}");
        assert!(m.contains(&"AI 客服".to_string()), "同义词缺失：{m:?}");
        assert!(!m.contains(&"Linux/Docker/Git".to_string()), "JD 没提就不该命中：{m:?}");
    }

    /// 斜杠组合过半命中才算：只写了「Docker」不该把「Linux/Docker/Git」整项算命中。
    #[test]
    fn slash_group_needs_half_or_more() {
        let blob = normalize("熟悉 Docker 与 K8s");
        assert!(!skill_hits(&blob, "Linux/Docker/Git"));
        assert!(skill_hits(&blob, "Docker/K8s"));
        assert!(skill_hits(&blob, "Docker"));
    }

    /// 提示词必须把「我确实会的」和「岗位要求」分开写，后者明确禁用来自称。
    #[test]
    fn build_prompt_separates_mine_from_job_demand() {
        let p = GreetingProfile {
            skills: vec!["Java".into(), "Redis".into()],
            ..Default::default()
        };
        let j = job("Java 后端", "熟悉 Java、Redis，会用 Kubernetes 与 Kafka 加分");
        let (_, user) = build_prompt(None, &j, &p);
        assert!(user.contains("你确实会的技能（只能从这里挑"), "{user}");
        assert!(user.contains("岗位要求/涉及的技术（我方未声明掌握"), "{user}");
        let demand = user.split("岗位要求/涉及的技术").nth(1).unwrap_or("");
        let demand = demand.split("岗位标题").next().unwrap_or("");
        assert!(demand.contains("Kubernetes") || demand.contains("K8s"), "{user}");
        assert!(!demand.contains("Redis"), "已有的技能不该出现在「岗位要求」里：{demand}");
        assert!(!user.contains("与该岗位匹配上的技能"), "旧措辞会把 JD 说成我的技能");
    }

    /// 带 HR 回复时：system 要加「接话不是自我介绍」，事实里要原文带上 HR 的话。
    #[test]
    fn build_prompt_with_carries_reply() {
        let j = job("Java 后端", "熟悉 Spring Boot");
        let (system, user) =
            build_prompt_with(None, &j, &profile(), Some("  你的 Redis 做过什么？\n 比如缓存穿透  "));
        assert!(system.contains("接话"), "{system}");
        assert!(system.contains("HR 的回复"), "{system}");
        assert!(user.contains("HR 刚说的话：你的 Redis 做过什么？ 比如缓存穿透"), "{user}");
        // 没有回复时不得给提示词加噪音。
        let (system2, user2) = build_prompt_with(None, &j, &profile(), Some("   "));
        assert_eq!(system2, DEFAULT_PROMPT);
        assert!(!user2.contains("HR 刚说的话"));
    }

    /// 规则兜底也要区分「首次打招呼」与「接话」：有回复时不能再从头自我介绍。
    #[test]
    fn rule_greeting_with_reply_is_not_an_opening() {
        let j = job("Java 后端开发工程师", "");
        let open = rule_greeting(&j, &profile());
        let back = rule_greeting_with(&j, &profile(), "你好，简历发我看看");
        assert!(open.starts_with("您好"));
        assert!(back.starts_with("收到您的回复"), "{back}");
        assert!(!back.contains("看到贵司在招"), "接话不该再介绍一次岗位：{back}");
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
