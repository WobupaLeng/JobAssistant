//! 大模型打分（`backend/app/scorer.py:430-590` 的平移）。
//!
//! 本模块**只做纯逻辑**：提示词构造、模型输出容错解析、规则初筛决策。
//! 真正的 HTTP 调用在 `infra::llm`，经 [`LlmClient`] 端口注入——
//! 这样解析逻辑可以毫秒级单测，不需要网络也不需要 API Key。
//!
//! ## 与 Python 侧的关键差异
//!
//! Python 的 `parse_llm_score` 把分数 clamp 到 `1..=10`（十分制）。
//! 本项目统一百分制（SPEC-002 决策 D3），故改为 `0..=100`。
//! 其余容错策略（markdown 围栏、平衡 JSON、截断抠分）逐条保留。

use crate::core::domain::profile::Profile;
use crate::core::domain::job::Job;
use crate::core::scoring::{SCORE_MAX, SCORE_MIN_DEFAULT};

/// 默认系统提示词。
///
/// 与 Python `SYSTEM_PROMPT` 的差异：分数范围由 `1-10` 改为 `0-100`。
/// 理由见 SPEC-002 决策 D3——全链路一个刻度，避免历史上「百分制算完再压成十分制」
/// （`scorer.py:343` 的 `max(1, min(10, -(-total // 10)))`）造成的精度损失。
pub const SYSTEM_PROMPT: &str = "你是一名资深技术招聘评估专家。请根据求职者画像，判断岗位与求职者的匹配度。\n\
只输出 JSON，不要任何解释文字，格式：{\"score\": 0-100 的整数, \"reason\": \"一句话中文理由\"}\n\
评分标准：100 分完美匹配；0 分完全不匹配。薪资、技术栈、经验年限、岗位方向、\
公司性质（外包/驻场/短期项目要显著扣分）都要考虑。";

/// JD 送进提示词的最大字符数（Python 侧同为 6000）。
pub const JD_PROMPT_LIMIT: usize = 6000;

/// 理由字段的最大字符数（Python 侧 `[:300]`）。
pub const REASON_MAX_CHARS: usize = 300;

/// 大模型打分失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// 网络/HTTP 层失败
    Transport(String),
    /// 模型返回为空或无法解析出分数
    Unparsable { finish_reason: String, raw_preview: String },
    /// 未配置 API Key
    NotConfigured,
}

impl LlmError {
    /// 用于拼接进 reason 的短标签（对齐 Python 的 `[大模型失败:{类型名}]`）。
    pub fn tag(&self) -> String {
        match self {
            LlmError::Transport(e) => format!("[大模型失败:Transport] {}", e),
            LlmError::Unparsable { finish_reason, .. } => {
                format!("[大模型失败:Unparsable] finish={}", finish_reason)
            }
            LlmError::NotConfigured => "[大模型未配置]".to_string(),
        }
    }
}

/// 大模型客户端端口。真实实现在 `infra::llm`；测试用假实现。
pub trait LlmClient: Send + Sync {
    /// 完成一次对话，返回 `(content, finish_reason, total_tokens)`。
    ///
    /// `max_tokens` 由调用方决定——推理型模型的思考 token 也计入该预算，
    /// 这是 Python 侧用 1600/3200 两档重试的原因。
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<(String, String, u32), LlmError>;
}

/// 大模型打分结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmScore {
    /// 0-100 分
    pub score: u8,
    /// 一句话理由
    pub reason: String,
    /// 本次调用的 token 消耗
    pub tokens: u32,
}

/// 拼接用户提示词（对齐 Python `build_user_prompt`）。
pub fn build_user_prompt(job: &Job, profile: &Profile, filters: &str) -> String {
    let mut lines: Vec<String> = vec![
        format!(
            "【求职者画像】约 {} 年 {:?} 经验，期望 {}",
            profile.years, profile.city, profile.title
        ),
        format!(
            "期望薪资 {}-{}K，期望城市 {}，工作年限约 {} 年。",
            profile.salary_low, profile.salary_high, profile.city, profile.years
        ),
    ];

    // 技能先去重再拼接（Python 用 dict.fromkeys 保持顺序去重）
    let mut skills: Vec<&str> = Vec::new();
    for s in profile.core_skills.iter().chain(profile.bonus_skills.iter()) {
        if !skills.contains(s) {
            skills.push(s);
        }
    }
    lines.push(format!("技能：{}", skills.join("、")));

    if !filters.is_empty() {
        lines.push(String::new());
        lines.push(format!("【我的筛选条件（投递偏好）】{}", filters));
    }

    lines.extend([
        String::new(),
        "【岗位信息】".to_string(),
        format!("平台：{}", or_blank(&job.platform)),
        format!("职位：{}", or_none(&job.title)),
        format!("公司：{}", or_none(&job.company)),
        format!("薪资：{}", or_none(&job.salary)),
        format!("地点：{}", or_none(&job.location)),
        format!("经验要求：{}", or_none(&job.experience)),
        format!("学历要求：{}", or_none(&job.degree)),
        format!("行业：{}", or_none(&job.company_industry)),
        format!("公司规模：{}", or_none(&job.company_scale)),
        format!("融资阶段：{}", or_none(&job.company_stage)),
        format!("在招岗位数：{}", opt_num(&job.hiring_count)),
        format!("成立年限：{}", opt_num(&job.founded_years)),
        format!("HR 活跃度：{}", or_none(&job.hr_active_status)),
        String::new(),
        "【JD 全文】".to_string(),
        truncate_chars(
            job.jd_text.as_deref().unwrap_or("（该平台未提供 JD 详情）"),
            JD_PROMPT_LIMIT,
        ),
    ]);

    lines.join("\n")
}

fn or_none(v: &Option<String>) -> &str {
    match v {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => "（未提供）",
    }
}

/// `platform` 在 [`Job`] 里是必填 `String`（不是 `Option`），单独一个分支。
fn or_blank(v: &str) -> &str {
    if v.trim().is_empty() {
        "（未提供）"
    } else {
        v.trim()
    }
}

fn opt_num<T: std::fmt::Display>(v: &Option<T>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "（未提供）".to_string(),
    }
}

/// 按**字符**（而非字节）截断，避免切坏 UTF-8 中文。
fn truncate_chars(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    s.chars().take(limit).collect()
}

/// 从模型输出里抽出分数与理由（对齐 Python `parse_llm_score`）。
///
/// 容错层级，从严格到宽松：
/// 1. 去除 ```json ... ``` 围栏
/// 2. 从第一个 `{` 起找**平衡**的 JSON 对象并整块解析
/// 3. JSON 被截断（`finish_reason=length` 常见）时用正则从残缺文本抠分
pub fn parse_llm_score(raw: &str) -> Option<LlmScore> {
    if raw.trim().is_empty() {
        return None;
    }
    let text = strip_code_fence(raw.trim());

    let Some(start) = text.find('{') else {
        return None;
    };

    // 找第一个平衡的 JSON 对象
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    for idx in start..text.len() {
        match bytes[idx] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let blob = &text[start..=idx];
                    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(blob) {
                        return score_from_value(&obj);
                    }
                    // 平衡但不合法（如中文引号），落到截断抠分路径
                    break;
                }
            }
            _ => {}
        }
    }

    score_from_partial(&text[start..])
}

fn strip_code_fence(text: &str) -> String {
    let Some(open) = text.find("```") else {
        return text.to_string();
    };
    let after_open = &text[open + 3..];
    // 跳过语言标记（json / JSON / 空）
    let body_start = match after_open.find('\n') {
        Some(nl) if after_open[..nl].trim().len() <= 8 => open + 3 + nl + 1,
        _ => open + 3,
    };
    match text[body_start..].find("```") {
        Some(close_rel) => text[body_start..body_start + close_rel].trim().to_string(),
        None => text[body_start..].trim().to_string(),
    }
}

fn score_from_value(obj: &serde_json::Value) -> Option<LlmScore> {
    let raw_score = obj.get("score")?;
    // Python 用 int(round(float(...)))，兼容 85 / 85.0 / "85"
    let as_f64 = match raw_score {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    let rounded = as_f64.round();
    if !rounded.is_finite() {
        return None;
    }
    let reason = obj
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(REASON_MAX_CHARS)
        .collect::<String>();
    Some(LlmScore {
        score: clamp_score(rounded),
        reason,
        tokens: 0,
    })
}

/// 从被截断的 JSON 文本里抠分（对齐 Python `_score_from_partial`）。
fn score_from_partial(text: &str) -> Option<LlmScore> {
    let score = extract_int_field(text, "score")?;

    let mut reason = String::new();
    if let Some(raw_reason) = extract_str_field_suffix(text, "reason") {
        // 去掉截断残留的半个转义 / 引号
        let cleaned = raw_reason
            .replace("\\n", " ")
            .replace("\\\"", "\"")
            .trim_end_matches(['"', '\\', ',', ' '])
            .trim()
            .chars()
            .take(REASON_MAX_CHARS)
            .collect::<String>();
        reason = cleaned;
    }

    Some(LlmScore {
        score,
        reason,
        tokens: 0,
    })
}

/// 抠 `"score"[:：] 数字`，同时兼容中文冒号。
fn extract_int_field(text: &str, field: &str) -> Option<u8> {
    let key = format!("\"{}\"", field);
    let pos = text.find(&key)? + key.len();
    let rest = text[pos..].trim_start();
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('：'))?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f64>().ok().map(|v| clamp_score(v.round()))
}

/// 抠 `"reason"[:：] "..."` 之后直到文末的全部内容（截断态）。
fn extract_str_field_suffix<'a>(text: &'a str, field: &str) -> Option<&'a str> {
    let key = format!("\"{}\"", field);
    let pos = text.find(&key)? + key.len();
    let rest = text[pos..].trim_start();
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('：'))?;
    let rest = rest.trim_start().strip_prefix('"')?;
    Some(rest)
}

/// 把浮点分夹到 `0..=SCORE_MAX`。
///
/// 与 Python 的差异：Python clamp 到 `1..=10`（下限 1 是为了避免出现 0 分岗位），
/// 百分制下 `0` 分是有意义的（完全不匹配），故下限取 0。
fn clamp_score(v: f64) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= SCORE_MAX as f64 {
        SCORE_MAX
    } else {
        v as u8
    }
}

/// 规则初筛决策（对齐 Python `Scorer.score` 的初筛分支）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreDecision {
    /// 未启用大模型，直接用规则分
    UseRule,
    /// 规则分低于初筛线，不花 token，直接用规则分
    UseRuleBelowPrefilter { rule_score: u8, prefilter: u8 },
    /// 过了初筛，采用大模型分
    UseLlm,
}

/// 判断该不该调大模型。`prefilter == 0` 表示不设初筛线。
pub fn decide(use_llm: bool, rule_score: u8, prefilter: u8) -> ScoreDecision {
    if !use_llm {
        return ScoreDecision::UseRule;
    }
    if prefilter > 0 && rule_score < prefilter {
        return ScoreDecision::UseRuleBelowPrefilter {
            rule_score,
            prefilter,
        };
    }
    ScoreDecision::UseLlm
}

/// 调用大模型打分，带两档 token 预算重试（对齐 Python 的 `for budget in (1600, 3200)`）。
///
/// 之所以要重试：推理型模型（deepseek-flash / deepseek-reasoner）的思考 token
/// 也计入 `max_tokens`，800 会被思考吃光导致 `content` 为空。
pub fn score_with_llm(
    client: &dyn LlmClient,
    job: &Job,
    profile: &Profile,
    filters: &str,
) -> Result<LlmScore, LlmError> {
    let user = build_user_prompt(job, profile, filters);
    let mut last_err: Option<LlmError> = None;

    for budget in [1600u32, 3200u32] {
        match client.complete(SYSTEM_PROMPT, &user, budget) {
            Ok((content, finish, tokens)) => {
                let truncated = finish == "length";
                match parse_llm_score(&content) {
                    Some(mut s) => {
                        if truncated {
                            s.reason = with_truncation_note(&s.reason);
                        }
                        s.tokens = tokens;
                        return Ok(s);
                    }
                    None => {
                        // 内容为空且是被截断的，换更大预算再试一次
                        last_err = Some(LlmError::Unparsable {
                            finish_reason: if finish.is_empty() {
                                "?".to_string()
                            } else {
                                finish.clone()
                            },
                            raw_preview: content.chars().take(200).collect(),
                        });
                        if content.trim().is_empty() && truncated {
                            continue;
                        }
                        return Err(last_err.unwrap());
                    }
                }
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }

    Err(last_err.unwrap_or(LlmError::NotConfigured))
}

/// 给被截断的理由加上提示后缀（Python 侧同样处理）。
fn with_truncation_note(reason: &str) -> String {
    const NOTE: &str = "（输出被截断，理由可能不完整）";
    if reason.contains(NOTE) {
        return reason.to_string();
    }
    let combined = format!("{}{}", reason, NOTE);
    combined.chars().take(REASON_MAX_CHARS).collect()
}

/// 大模型打分开关与初筛线的默认值（对齐 Python `Scorer.llm_prefilter = 5` 的百分制换算）。
pub const LLM_PREFILTER_DEFAULT: u8 = 50;

/// 初筛线的合法上限（`min_score` 同源，均取 `SCORE_MAX`）。
pub const fn prefilter_max() -> u8 {
    SCORE_MAX
}

/// 默认门槛，导出给配置层复用，避免第二处硬编码。
pub const MIN_SCORE: u8 = SCORE_MIN_DEFAULT;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::profile::DEFAULT_PROFILE;

    fn sample_job() -> Job {
        let mut j = Job {
            job_id: "j1".into(),
            platform: "boss".into(),
            status: "pending".into(),
            ..Default::default()
        };
        j.title = Some("Java 后端工程师".into());
        j.company = Some("某科技有限公司".into());
        j.salary = Some("20-35K".into());
        j.experience = Some("5-10年".into());
        j.location = Some("深圳".into());
        j.jd_text = Some("Java Spring Cloud MySQL Redis".into());
        j
    }

    // ---- 解析 ----

    #[test]
    fn parse_plain_json() {
        let r = parse_llm_score(r#"{"score": 85, "reason": "技术栈高度匹配"}"#).unwrap();
        assert_eq!(r.score, 85);
        assert_eq!(r.reason, "技术栈高度匹配");
    }

    #[test]
    fn parse_with_markdown_fence() {
        let raw = "```json\n{\"score\": 72, \"reason\": \"薪资合适\"}\n```";
        let r = parse_llm_score(raw).unwrap();
        assert_eq!(r.score, 72);
        assert_eq!(r.reason, "薪资合适");
    }

    #[test]
    fn parse_with_leading_chatter() {
        let raw = "好的，我来评估。\n{\"score\": 60, \"reason\": \"一般\"}\n以上。";
        let r = parse_llm_score(raw).unwrap();
        assert_eq!(r.score, 60);
        assert_eq!(r.reason, "一般");
    }

    #[test]
    fn parse_float_and_string_score() {
        assert_eq!(parse_llm_score(r#"{"score": 84.6, "reason": "x"}"#).unwrap().score, 85);
        assert_eq!(parse_llm_score(r#"{"score": "77", "reason": "x"}"#).unwrap().score, 77);
    }

    #[test]
    fn parse_truncated_json_recovers_score() {
        // finish_reason=length 的典型形态：reason 写到一半就断了
        let raw = r#"{"score": 88, "reason": "技能高度匹配，薪资也在预"#;
        let r = parse_llm_score(raw).unwrap();
        assert_eq!(r.score, 88);
        assert!(r.reason.starts_with("技能高度匹配"));
    }

    #[test]
    fn parse_fullwidth_colon() {
        let raw = r#"{"score"：91, "reason"："匹配度高"#;
        let r = parse_llm_score(raw).unwrap();
        assert_eq!(r.score, 91);
    }

    #[test]
    fn parse_rejects_empty_and_garbage() {
        assert!(parse_llm_score("").is_none());
        assert!(parse_llm_score("   ").is_none());
        assert!(parse_llm_score("模型拒绝了请求").is_none());
        assert!(parse_llm_score(r#"{"reason": "没有分数"}"#).is_none());
    }

    #[test]
    fn parse_clamps_out_of_range() {
        assert_eq!(parse_llm_score(r#"{"score": 250, "reason": "x"}"#).unwrap().score, 100);
        assert_eq!(parse_llm_score(r#"{"score": -30, "reason": "x"}"#).unwrap().score, 0);
    }

    #[test]
    fn parse_reason_is_truncated_to_300_chars() {
        let long = "很".repeat(500);
        let raw = format!(r#"{{"score": 70, "reason": "{}"}}"#, long);
        let r = parse_llm_score(&raw).unwrap();
        assert_eq!(r.reason.chars().count(), REASON_MAX_CHARS);
    }

    // ---- 提示词 ----

    #[test]
    fn prompt_contains_profile_and_job_fields() {
        let p = build_user_prompt(&sample_job(), &DEFAULT_PROFILE, "");
        assert!(p.contains("求职者画像"));
        assert!(p.contains("20-35K"));
        assert!(p.contains("Java 后端工程师"));
        assert!(p.contains("某科技有限公司"));
        assert!(p.contains("【JD 全文】"));
        assert!(!p.contains("筛选条件"));
    }

    #[test]
    fn prompt_includes_filters_when_present() {
        let p = build_user_prompt(&sample_job(), &DEFAULT_PROFILE, "只看深圳");
        assert!(p.contains("【我的筛选条件（投递偏好）】只看深圳"));
    }

    #[test]
    fn prompt_marks_missing_jd() {
        let mut j = sample_job();
        j.jd_text = None;
        let p = build_user_prompt(&j, &DEFAULT_PROFILE, "");
        assert!(p.contains("（该平台未提供 JD 详情）"));
    }

    #[test]
    fn prompt_truncates_long_jd_by_chars() {
        let mut j = sample_job();
        j.jd_text = Some("中".repeat(9000));
        let p = build_user_prompt(&j, &DEFAULT_PROFILE, "");
        // JD 部分不应超过上限（整段提示词里 JD 之后的只有它自己）
        let jd_part = p.split("【JD 全文】").nth(1).unwrap();
        assert_eq!(jd_part.trim().chars().count(), JD_PROMPT_LIMIT);
    }

    #[test]
    fn system_prompt_declares_100_scale() {
        assert!(SYSTEM_PROMPT.contains("0-100"));
        assert!(!SYSTEM_PROMPT.contains("1-10"));
    }

    // ---- 初筛决策 ----

    #[test]
    fn decide_uses_rule_when_llm_disabled() {
        assert_eq!(decide(false, 90, 50), ScoreDecision::UseRule);
    }

    #[test]
    fn decide_skips_llm_below_prefilter() {
        assert_eq!(
            decide(true, 30, 50),
            ScoreDecision::UseRuleBelowPrefilter {
                rule_score: 30,
                prefilter: 50
            }
        );
    }

    #[test]
    fn decide_uses_llm_above_prefilter() {
        assert_eq!(decide(true, 60, 50), ScoreDecision::UseLlm);
    }

    #[test]
    fn decide_prefilter_zero_always_calls_llm() {
        assert_eq!(decide(true, 0, 0), ScoreDecision::UseLlm);
    }

    // ---- 带重试的调用 ----

    struct FakeClient {
        /// 每次调用的返回，按顺序消费
        responses: std::sync::Mutex<Vec<Result<(String, String, u32), LlmError>>>,
        calls: std::sync::Mutex<Vec<u32>>,
    }

    impl FakeClient {
        fn new(responses: Vec<Result<(String, String, u32), LlmError>>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses),
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn call_budgets(&self) -> Vec<u32> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl LlmClient for FakeClient {
        fn complete(
            &self,
            _system: &str,
            _user: &str,
            max_tokens: u32,
        ) -> Result<(String, String, u32), LlmError> {
            self.calls.lock().unwrap().push(max_tokens);
            self.responses.lock().unwrap().remove(0)
        }
    }

    #[test]
    fn llm_call_records_tokens() {
        let fake = FakeClient::new(vec![Ok((
            r#"{"score": 92, "reason": "完美匹配"}"#.into(),
            "stop".into(),
            1234,
        ))]);
        let r = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap();
        assert_eq!(r.score, 92);
        assert_eq!(r.tokens, 1234);
        assert_eq!(fake.call_budgets(), vec![1600]);
    }

    #[test]
    fn llm_retries_with_bigger_budget_when_content_empty_and_truncated() {
        let fake = FakeClient::new(vec![
            Ok(("".into(), "length".into(), 1600)),
            Ok((r#"{"score": 66, "reason": "还行"}"#.into(), "stop".into(), 2400)),
        ]);
        let r = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap();
        assert_eq!(r.score, 66);
        assert_eq!(fake.call_budgets(), vec![1600, 3200]);
    }

    #[test]
    fn llm_does_not_retry_when_content_present_but_truncated() {
        // 内容非空且能抠出分，直接用（Python 侧同样不重试）
        let fake = FakeClient::new(vec![Ok((
            r#"{"score": 74, "reason": "薪资偏"#.into(),
            "length".into(),
            1700,
        ))]);
        let r = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap();
        assert_eq!(r.score, 74);
        assert_eq!(fake.call_budgets(), vec![1600]);
    }

    #[test]
    fn truncated_reason_gets_note() {
        let fake = FakeClient::new(vec![Ok((
            r#"{"score": 74, "reason": "薪资偏"#.into(),
            "length".into(),
            1700,
        ))]);
        let r = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap();
        assert!(r.reason.contains("输出被截断"), "实际 {}", r.reason);
    }

    #[test]
    fn llm_transport_error_propagates() {
        let fake = FakeClient::new(vec![Err(LlmError::Transport("连接超时".into()))]);
        let err = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap_err();
        assert!(matches!(err, LlmError::Transport(_)));
        // 网络错误不重试（与 Python 的 try/except 行为一致：直接回退规则分）
        assert_eq!(fake.call_budgets().len(), 1);
    }

    #[test]
    fn unparsable_nonempty_content_does_not_retry() {
        let fake = FakeClient::new(vec![Ok(("抱歉我无法评估".into(), "stop".into(), 50))]);
        let err = score_with_llm(&fake, &sample_job(), &DEFAULT_PROFILE, "").unwrap_err();
        assert!(matches!(err, LlmError::Unparsable { .. }));
        assert_eq!(fake.call_budgets().len(), 1);
    }

    #[test]
    fn error_tag_is_short_and_readable() {
        assert_eq!(LlmError::NotConfigured.tag(), "[大模型未配置]");
        assert!(LlmError::Transport("x".into()).tag().contains("大模型失败"));
    }
}
