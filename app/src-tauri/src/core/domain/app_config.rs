//! 配置页读写的领域规则：键名白名单、默认值、密钥脱敏。
//!
//! 铁律（AGENTS.md 2.5）：**API Key 只落数据库 `app_config` 表，严禁明文回传**。
//! 因此「怎么掩码」不是界面细节，而是安全规则，放在 domain 里由单测钉死：
//! 任何一条路径漏了掩码，都会把 32 位密钥原样送到 WebView（再被 devtools、日志、
//! 截图带走）。

use serde::{Deserialize, Serialize};

/// 配置键名（与旧库 `app_config` 表的 key 完全一致，**不得改名**）。
///
/// 改名等于让老用户的配置全部失效：表里的行还在，只是没人再读它。
pub const KEY_API_KEY: &str = "api_key";
pub const KEY_BASE_URL: &str = "base_url";
pub const KEY_MODEL: &str = "model";
pub const KEY_THEME: &str = "theme";
pub const KEY_MAX_JD: &str = "max_jd";
pub const KEY_SCORE_MODE: &str = "score_mode";
pub const KEY_LLM_PREFILTER: &str = "llm_prefilter";
pub const KEY_GREET_PROMPT: &str = "greet_prompt";
pub const KEY_SKILLS: &str = "skills";
pub const KEY_EXCLUDE_KEYWORDS: &str = "exclude_keywords";
pub const KEY_EXCLUDE_RULES: &str = "exclude_rules";
pub const KEY_LAST_FILTER: &str = "last_filter";
pub const KEY_TOKENS_TOTAL: &str = "tokens_total";
pub const KEY_TOKENS_GREET: &str = "tokens_greet";
pub const KEY_TOKENS_SCORE: &str = "tokens_score";
pub const KEY_DATA_DIR: &str = "data_dir";
pub const KEY_RESUME_PATH: &str = "resume_path";
pub const KEY_PLATFORM_LOGIN: &str = "platform_login";

/// 抓取筛选面板「默认条件」的键前缀。
///
/// 抓取面板有 13 项条件（筛选面板 9 类 + 排除关键词 / HR 活跃度 / 工作制 /
/// JD 学历要求 / 成立年限），它们**不是**应用配置，而是「下次抓取用哪套条件」，
/// 统一挂在 `scrape_defaults.` 前缀下，既与业务配置区分开，也便于
/// `get_app_config` 一次聚合回一个嵌套对象。
pub const SCRAPE_DEFAULTS_PREFIX: &str = "scrape_defaults.";

/// 城市
pub const KEY_SCRAPE_CITY: &str = "scrape_defaults.city";
/// 薪资区间
pub const KEY_SCRAPE_SALARY: &str = "scrape_defaults.salary";
/// 工作经验
pub const KEY_SCRAPE_EXPERIENCE: &str = "scrape_defaults.experience";
/// 学历
pub const KEY_SCRAPE_DEGREE: &str = "scrape_defaults.degree";
/// 公司规模
pub const KEY_SCRAPE_COMPANY_SCALE: &str = "scrape_defaults.company_scale";
/// 融资阶段
pub const KEY_SCRAPE_FINANCE_STAGE: &str = "scrape_defaults.finance_stage";
/// 行业
pub const KEY_SCRAPE_INDUSTRY: &str = "scrape_defaults.industry";
/// 职位类型
pub const KEY_SCRAPE_JOB_TYPE: &str = "scrape_defaults.job_type";
/// 搜索最大翻页次数
pub const KEY_SCRAPE_MAX_PAGES: &str = "scrape_defaults.max_pages";
/// 排除关键词
pub const KEY_SCRAPE_EXCLUDE_KEYWORDS: &str = "scrape_defaults.exclude_keywords";
/// HR 活跃度
pub const KEY_SCRAPE_HR_ACTIVE: &str = "scrape_defaults.hr_active";
/// 工作制（周末 / 大小周）
pub const KEY_SCRAPE_WORK_WEEKEND: &str = "scrape_defaults.work_weekend";
/// JD 里的岗位要求（学历）
pub const KEY_SCRAPE_JD_DEGREE: &str = "scrape_defaults.jd_degree";
/// 成立年限
pub const KEY_SCRAPE_MIN_YEARS: &str = "scrape_defaults.min_years";

/// 用户可写的键白名单。
///
/// `set_app_config` 只接受这里面的键：接口收的是 `serde_json::Value`，
/// 不过滤就能被写入 `platform_login` 之类由程序自己维护的状态，
/// 让「登录态」变成前端一句话就能伪造的东西。
pub const WRITABLE_KEYS: &[&str] = &[
    KEY_API_KEY,
    KEY_BASE_URL,
    KEY_MODEL,
    KEY_THEME,
    KEY_MAX_JD,
    KEY_SCORE_MODE,
    KEY_LLM_PREFILTER,
    KEY_GREET_PROMPT,
    KEY_SKILLS,
    KEY_EXCLUDE_KEYWORDS,
    KEY_EXCLUDE_RULES,
    KEY_LAST_FILTER,
    KEY_RESUME_PATH,
    KEY_SCRAPE_CITY,
    KEY_SCRAPE_SALARY,
    KEY_SCRAPE_EXPERIENCE,
    KEY_SCRAPE_DEGREE,
    KEY_SCRAPE_COMPANY_SCALE,
    KEY_SCRAPE_FINANCE_STAGE,
    KEY_SCRAPE_INDUSTRY,
    KEY_SCRAPE_JOB_TYPE,
    KEY_SCRAPE_MAX_PAGES,
    KEY_SCRAPE_EXCLUDE_KEYWORDS,
    KEY_SCRAPE_HR_ACTIVE,
    KEY_SCRAPE_WORK_WEEKEND,
    KEY_SCRAPE_JD_DEGREE,
    KEY_SCRAPE_MIN_YEARS,
];

/// 分值门槛的键名。**刻意不在 [`WRITABLE_KEYS`] 里**：它由 `set_limit` 统一写，
/// 顺带校验「门槛不能超过满分」，放开了就绕过了校验。
pub const KEY_MIN_SCORE: &str = "min_score";

/// 默认值：老库里没写过的键，界面显示这些而不是空白输入框。
pub const DEFAULT_MAX_JD: u32 = 15;
pub const DEFAULT_THEME: &str = "light";
pub const DEFAULT_SCORE_MODE: &str = "rule";
pub const DEFAULT_LLM_PREFILTER: u8 = 50;
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-chat";

/// 把密钥掩码成只留尾 4 位。
///
/// - 空 / 纯空白 → 空串（界面据此显示「未配置」，不能显示 `****` 假装配了）
/// - 长度 ≤ 4 → 全掩，否则尾 4 位本身就是整个密钥
pub fn mask_api_key(raw: &str) -> String {
    let k = raw.trim();
    if k.is_empty() {
        return String::new();
    }
    if k.chars().count() <= 4 {
        return "****".to_string();
    }
    let tail: String = k
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("****{tail}")
}

/// 从已读出的键值对里挑出 API Key，只用于「是否配置」判定，**绝不外传**。
pub fn has_api_key(pairs: &[(String, String)]) -> bool {
    pairs
        .iter()
        .any(|(k, v)| k == KEY_API_KEY && !v.trim().is_empty())
}

/// `set_limit` 的入参（`max_jd` / `min_score`，均可省略）。
///
/// `alias` 不能删：前端发 `maxJd` / `minScore`，serde 对未知字段不报错，
/// 少了 alias 界面填的数字会被静默丢弃（AGENTS.md 3.2）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LimitArgs {
    #[serde(default, alias = "maxJd")]
    pub max_jd: Option<u32>,
    #[serde(default, alias = "minScore")]
    pub min_score: Option<u8>,
}

/// 校验并归一化 `max_jd`：0 视为「不限制」；超过 [`MAX_JD_CAP`] 直接拒绝。
///
/// 上界存在的理由：一次抓 10000 条 JD 会把 Boss 的详情页翻到底，
/// 既不可能跑完也百分百触发风控。
pub const MAX_JD_CAP: u32 = 500;

/// `set_limit` 的校验结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitValues {
    pub max_jd: u32,
    pub min_score: u8,
}

/// 合并「本次提交」与「当前值」，返回校验后的结果。
///
/// 两个参数都可以省略（前端只在对应输入框失焦时提交那一个字段），
/// 省略时沿用 `current`。
pub fn resolve_limit(
    args: &LimitArgs,
    current: LimitValues,
    score_max: u8,
) -> Result<LimitValues, String> {
    let max_jd = args.max_jd.unwrap_or(current.max_jd);
    if max_jd > MAX_JD_CAP {
        return Err(format!("有效 JD 上限最大 {MAX_JD_CAP}（当前填的是 {max_jd}）"));
    }
    let min_score = args.min_score.unwrap_or(current.min_score);
    if min_score > score_max {
        return Err(format!("分值门槛不能超过满分 {score_max}（当前填的是 {min_score}）"));
    }
    Ok(LimitValues { max_jd, min_score })
}

/// 抓取筛选面板「默认条件」的入参（`set_scrape_defaults`）。
///
/// 全部 `Option<serde_json::Value>`：
/// - `None`（字段缺席或显式 `null`）= **不动**，与 `set_app_config::apply_patch` 的
///   「只提交 diff」语义一致——面板上没被用户碰过的条件不得被覆盖；
/// - `Some(v)` = 写入 `scrape_defaults.<键>`，值按 JSON 文本落库（数组 / 数字原样保留）。
///
/// 每个字段都必须带 `alias`：前端发驼峰（`maxPages` / `companyScale` …），
/// serde 对未知字段**不报错**，漏了 alias 界面上设的值会被静默丢弃（AGENTS.md 3.2）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScrapeDefaultsArgs {
    #[serde(default, alias = "city")]
    pub city: Option<serde_json::Value>,
    #[serde(default, alias = "salary")]
    pub salary: Option<serde_json::Value>,
    #[serde(default, alias = "experience")]
    pub experience: Option<serde_json::Value>,
    #[serde(default, alias = "degree")]
    pub degree: Option<serde_json::Value>,
    #[serde(default, alias = "companyScale")]
    pub company_scale: Option<serde_json::Value>,
    #[serde(default, alias = "financeStage")]
    pub finance_stage: Option<serde_json::Value>,
    #[serde(default, alias = "industry")]
    pub industry: Option<serde_json::Value>,
    #[serde(default, alias = "jobType")]
    pub job_type: Option<serde_json::Value>,
    #[serde(default, alias = "maxPages")]
    pub max_pages: Option<serde_json::Value>,
    #[serde(default, alias = "excludeKeywords")]
    pub exclude_keywords: Option<serde_json::Value>,
    #[serde(default, alias = "hrActive")]
    pub hr_active: Option<serde_json::Value>,
    #[serde(default, alias = "workWeekend")]
    pub work_weekend: Option<serde_json::Value>,
    #[serde(default, alias = "jdDegree")]
    pub jd_degree: Option<serde_json::Value>,
    #[serde(default, alias = "minYears")]
    pub min_years: Option<serde_json::Value>,
}

/// 抓取默认条件的键白名单（相对名，不含前缀）。
///
/// `set_scrape_defaults` 只认这里面的名字：入参结构体虽然已经把字段钉死了，
/// 但写库走的是「相对名 → `scrape_defaults.<名>`」的拼装，白名单是拼装前的
/// 最后一道闸——将来有人加字段却忘了加白名单，会被这里拦住而不是写进任意键。
pub const SCRAPE_DEFAULT_KEYS: &[&str] = &[
    "city",
    "salary",
    "experience",
    "degree",
    "company_scale",
    "finance_stage",
    "industry",
    "job_type",
    "max_pages",
    "exclude_keywords",
    "hr_active",
    "work_weekend",
    "jd_degree",
    "min_years",
];

impl ScrapeDefaultsArgs {
    /// 摊平成 `(相对名, 值)`，**跳过所有 `None`**。
    ///
    /// 返回值保持固定顺序（与 [`SCRAPE_DEFAULT_KEYS`] 一致），便于单测断言。
    pub fn diffs(&self) -> Vec<(&'static str, &serde_json::Value)> {
        let pairs: [(&'static str, &Option<serde_json::Value>); 14] = [
            ("city", &self.city),
            ("salary", &self.salary),
            ("experience", &self.experience),
            ("degree", &self.degree),
            ("company_scale", &self.company_scale),
            ("finance_stage", &self.finance_stage),
            ("industry", &self.industry),
            ("job_type", &self.job_type),
            ("max_pages", &self.max_pages),
            ("exclude_keywords", &self.exclude_keywords),
            ("hr_active", &self.hr_active),
            ("work_weekend", &self.work_weekend),
            ("jd_degree", &self.jd_degree),
            ("min_years", &self.min_years),
        ];
        pairs
            .into_iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k, v)))
            .collect()
    }
}

/// 把相对名拼成落库键名，并做白名单校验。
///
/// 返回 `Err` 而不是静默跳过：调用方拼错键名时应当报错，否则表现为
/// 「保存成功但下次打开就没了」。
pub fn scrape_default_key(rel: &str) -> Result<String, String> {
    if !SCRAPE_DEFAULT_KEYS.contains(&rel) {
        return Err(format!("unknown scrape default key: {rel}"));
    }
    Ok(format!("{SCRAPE_DEFAULTS_PREFIX}{rel}"))
}

/// 把一个默认条件的 JSON 值转成落库文本。
///
/// 字符串按原文存（前端回读时它就是字符串，不会被多包一层引号）；
/// 其余类型（数字 / 布尔 / 数组 / 对象）存 JSON 文本，读侧用
/// `store_ports::parse_config_json` 还原——这样 `["北京"]` 与 `["北京","上海"]` 都能保真。
pub fn scrape_value_to_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// 从**整张表**的键值对里聚合出 `{ city: …, max_pages: … }`。
///
/// 只认 [`SCRAPE_DEFAULT_KEYS`] 里的相对名，且前缀必须严格等于
/// [`SCRAPE_DEFAULTS_PREFIX`]。一个都没配过时返回 `{}`——界面据此显示「未设置」
/// 而不是一屏假默认值。
pub fn collect_scrape_defaults(pairs: &[(String, String)]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (k, v) in pairs {
        let Some(rel) = k.strip_prefix(SCRAPE_DEFAULTS_PREFIX) else {
            continue;
        };
        if !SCRAPE_DEFAULT_KEYS.contains(&rel) {
            continue;
        }
        // 库里可能被人手工改坏，解析失败回退成原字符串而不是丢掉整条
        let parsed = serde_json::from_str::<serde_json::Value>(v)
            .unwrap_or_else(|_| serde_json::Value::String(v.clone()));
        obj.insert(rel.to_string(), parsed);
    }
    serde_json::Value::Object(obj)
}

/// 与 [`collect_scrape_defaults`] 配套的反向：从整张表里挑出全部
/// `scrape_defaults.*` 原始键值（供 `config_pairs` 过滤）。
pub fn is_scrape_default_key(key: &str) -> bool {
    key.strip_prefix(SCRAPE_DEFAULTS_PREFIX)
        .map(|rel| SCRAPE_DEFAULT_KEYS.contains(&rel))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_is_masked_to_tail_four() {
        assert_eq!(mask_api_key("sk-1234567890abcd"), "****abcd");
        // 短密钥全掩：尾 4 位就是整个密钥
        assert_eq!(mask_api_key("abc"), "****");
        assert_eq!(mask_api_key("abcd"), "****");
        // 空值返回空串，界面据此显示「未配置」
        assert_eq!(mask_api_key("   "), "");
        // 掩码结果里不得出现原文中间段
        let masked = mask_api_key("sk-secret-middle-9876");
        assert!(!masked.contains("secret"));
        assert!(masked.starts_with("****"));
    }

    #[test]
    fn has_api_key_ignores_blank_values() {
        let pairs = vec![(KEY_API_KEY.to_string(), "  ".to_string())];
        assert!(!has_api_key(&pairs));
        let pairs = vec![(KEY_API_KEY.to_string(), "sk-x".to_string())];
        assert!(has_api_key(&pairs));
        assert!(!has_api_key(&[]));
    }

    #[test]
    fn limit_args_accept_camel_and_snake_case() {
        // 前端发驼峰
        let camel: LimitArgs = serde_json::from_value(serde_json::json!({
            "maxJd": 30, "minScore": 75
        }))
        .expect("驼峰必须认得");
        assert_eq!(camel.max_jd, Some(30));
        assert_eq!(camel.min_score, Some(75));
        // 旧调用方发蛇形
        let snake: LimitArgs = serde_json::from_value(serde_json::json!({
            "max_jd": 20, "min_score": 70
        }))
        .expect("蛇形必须认得");
        assert_eq!(snake.max_jd, Some(20));
        assert_eq!(snake.min_score, Some(70));
        // 两者都能省略
        let empty: LimitArgs = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.max_jd.is_none() && empty.min_score.is_none());
    }

    #[test]
    fn resolve_limit_keeps_current_when_field_omitted() {
        let current = LimitValues {
            max_jd: 15,
            min_score: 70,
        };
        // 只改上限
        let out = resolve_limit(
            &LimitArgs {
                max_jd: Some(40),
                min_score: None,
            },
            current,
            100,
        )
        .unwrap();
        assert_eq!(out.max_jd, 40);
        assert_eq!(out.min_score, 70, "省略的字段必须沿用当前值");
        // 只改门槛
        let out = resolve_limit(
            &LimitArgs {
                max_jd: None,
                min_score: Some(80),
            },
            current,
            100,
        )
        .unwrap();
        assert_eq!(out.max_jd, 15);
        assert_eq!(out.min_score, 80);
    }

    #[test]
    fn resolve_limit_rejects_out_of_range() {
        let current = LimitValues {
            max_jd: 15,
            min_score: 70,
        };
        let too_many = resolve_limit(
            &LimitArgs {
                max_jd: Some(MAX_JD_CAP + 1),
                min_score: None,
            },
            current,
            100,
        );
        assert!(too_many.is_err());
        assert!(too_many.unwrap_err().contains("有效 JD 上限"));
        let too_high = resolve_limit(
            &LimitArgs {
                max_jd: None,
                min_score: Some(101),
            },
            current,
            100,
        );
        assert!(too_high.is_err());
        assert!(too_high.unwrap_err().contains("分值门槛"));
    }

    #[test]
    fn min_score_is_not_user_writable_directly() {
        // 门槛必须走 set_limit 的校验路径，不能被 patch 绕过
        assert!(!WRITABLE_KEYS.contains(&KEY_MIN_SCORE));
        // 登录态同样不由前端写
        assert!(!WRITABLE_KEYS.contains(&KEY_PLATFORM_LOGIN));
        assert!(WRITABLE_KEYS.contains(&KEY_API_KEY));
    }

    #[test]
    fn scrape_default_key_whitelist_and_prefix() {
        assert_eq!(
            scrape_default_key("max_pages").unwrap(),
            "scrape_defaults.max_pages"
        );
        assert_eq!(scrape_default_key("company_scale").unwrap(), KEY_SCRAPE_COMPANY_SCALE);
        // 白名单外一律拒
        assert!(scrape_default_key("platform_login").is_err());
        assert!(scrape_default_key("").is_err());
        assert!(scrape_default_key("scrape_defaults.city").is_err(), "相对名不含前缀");
        // 白名单里的键必须真的进了 WRITABLE_KEYS，否则 set_app_config 会拦掉
        for rel in SCRAPE_DEFAULT_KEYS {
            assert!(
                WRITABLE_KEYS.contains(&format!("{SCRAPE_DEFAULTS_PREFIX}{rel}").as_str()),
                "{rel} 未加入 WRITABLE_KEYS"
            );
            assert!(is_scrape_default_key(&format!("{SCRAPE_DEFAULTS_PREFIX}{rel}")));
        }
        assert!(!is_scrape_default_key("scrape_defaults.unknown"));
        assert!(!is_scrape_default_key("theme"));
    }

    #[test]
    fn collect_scrape_defaults_keeps_json_types_and_ignores_others() {
        let pairs = vec![
            ("scrape_defaults.city".to_string(), r#"["北京"]"#.to_string()),
            ("scrape_defaults.max_pages".to_string(), "3".to_string()),
            ("scrape_defaults.exclude_keywords".to_string(), "外包,培训".to_string()),
            ("theme".to_string(), "dark".to_string()),
            ("scrape_defaults.unknown".to_string(), "x".to_string()),
        ];
        let got = collect_scrape_defaults(&pairs);
        assert_eq!(
            got,
            serde_json::json!({
                "city": ["北京"],
                "max_pages": 3,
                // 非 JSON 的纯文本回退成字符串，不丢值
                "exclude_keywords": "外包,培训"
            })
        );
        // 一个都没配过时必须是空对象，界面据此显示「未设置」
        assert_eq!(collect_scrape_defaults(&[]), serde_json::json!({}));
    }
}
