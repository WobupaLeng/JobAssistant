//! `scrape_defaults`（app_config 持久化）→ 抓取筛选快照 + 排除规则。
//!
//! **为什么在 core 而不是命令层**：这是「用户配置怎么解释成抓取条件与
//! 否决规则」的领域映射，命令层只负责把库里的键值对搬进来（AGENTS.md 2.2）。
//! 纯函数、零 IO：不读库、不起进程，单测可以直接钉住映射形状。
//!
//! 键名真源是 [`crate::core::domain::app_config::SCRAPE_DEFAULT_KEYS`]（蛇形，
//! 如 `company_scale` / `finance_stage`）；而下发给 sidecar 的协议键是
//! `scale` / `stage`（`scraper/sidecar.py::_run_scrape` 逐字读取的名字）。
//! 两套名字的换算只发生在这里与 [`super::launch::ScrapeJobSpec::start_command`]，
//! 别在第三处再写一遍。
//!
//! **否决/打分不下放**：这里产出的 [`ExcludeRules`] 只进 Rust 侧的
//! `IngestConfig`（`core/sidecar/ingest.rs`），绝不序列化进 sidecar 协议——
//! scraper 不打分、不落库、不持有业务状态。

use crate::core::domain::rules::ExcludeRules;

/// 翻页次数夹紧范围：与前端 `lib_ui/options.ts` 的
/// `MAX_PAGES_MIN` / `MAX_PAGES_MAX` 同源（1..=20）。
/// 库里可能被人手工改坏，越界值夹住而不是让整个抓取起不来。
const MAX_PAGES_MIN: u32 = 1;
const MAX_PAGES_MAX: u32 = 20;

/// 一次抓取的筛选条件快照（不含关键词/门槛/上限——它们在 `ScrapeJobSpec`
/// 的既有字段里）。全部缺省 = 「不限」，序列化时整组省略，与旧协议形状兼容。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrapeFilters {
    /// Boss 城市编码（如 `101020100` 上海）。`None` = sidecar 默认城市。
    pub city: Option<String>,
    /// 以下多选编码表见 `scraper/base.py::ScrapeConfig` 注释。
    pub salary: Vec<String>,
    pub experience: Vec<String>,
    pub degree: Vec<String>,
    /// 配置键是 `company_scale`，协议键是 `scale`。
    pub scale: Vec<String>,
    /// 配置键是 `finance_stage`，协议键是 `stage`。
    pub stage: Vec<String>,
    pub industry: Vec<String>,
    /// `1901` 全职 / `1902` 兼职 / `1903` 实习；`None` = 不限。
    pub job_type: Option<String>,
    /// 翻页次数；`None` = sidecar 默认（5）。
    pub max_pages: Option<u32>,
}

/// 单值键 → 非空字符串。数字也接受（旧数据里 `max_pages` 可能是 `3` 或 `"3"`）；
/// 空串/空白视为「未设置」。
fn text_of(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// 多选键 → 字符串数组。数组逐项取文本；字符串按逗号/顿号切开
/// （手工改库或旧版草稿可能存成 `"403,406"` 这种平文本）。
fn list_of(v: Option<&serde_json::Value>) -> Vec<String> {
    match v {
        Some(serde_json::Value::Array(items)) => {
            items.iter().filter_map(|i| text_of(Some(i))).collect()
        }
        Some(serde_json::Value::String(s)) => s
            .split([',', '，', '、'])
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// `collect_scrape_defaults` 的输出 → 抓取筛选快照。
///
/// 未知键忽略、脏值回退「不限」：配置表可能被人手工改坏，
/// 任何一项坏掉都不该让整次抓取起不来。
pub fn filters_from_defaults(d: &serde_json::Value) -> ScrapeFilters {
    ScrapeFilters {
        city: text_of(d.get("city")),
        salary: list_of(d.get("salary")),
        experience: list_of(d.get("experience")),
        degree: list_of(d.get("degree")),
        scale: list_of(d.get("company_scale")),
        stage: list_of(d.get("finance_stage")),
        industry: list_of(d.get("industry")),
        job_type: text_of(d.get("job_type")),
        max_pages: text_of(d.get("max_pages"))
            .and_then(|s| s.parse::<u32>().ok())
            .map(|n| n.clamp(MAX_PAGES_MIN, MAX_PAGES_MAX)),
    }
}

/// `collect_scrape_defaults` 的输出 → 排除规则（只进 Rust 侧 `IngestConfig`）。
///
/// - `exclude_keywords`：用户词表**替换**内置否决词（否则用户永远删不掉
///   默认词）；缺席或空白回退 `ExcludeRules::default()`（见 `from_keywords`）。
/// - `hr_active`：`"1"/"3"/"7"` → 超过 N 天未活跃（或未知）即排除；
///   `"any"`/空/脏值 → 不排除。**注意**：勾了这项而 sidecar 取不到活跃度
///   时会全量否决（`rules.rs` 既有语义，scraper/AGENTS.md 已警示）。
/// - `work_weekend`：原样透传（三档 `double`/`no_bigweek`/`single` 生效，
///   SPEC-004；`"any"` → 不限）。
/// - `min_years`：公司成立年限低于此值即排除。
/// - `short_term_title`：`"off"`/`"false"` → 不按「岗位名显示短期项目」排除；
///   缺席/其它值 = 开（等价旧行为）。
pub fn exclude_rules_from_defaults(d: &serde_json::Value) -> ExcludeRules {
    let mut r = match text_of(d.get("exclude_keywords")) {
        Some(raw) => ExcludeRules::from_keywords(&raw),
        None => ExcludeRules::default(),
    };
    r.exclude_hr_active_days = text_of(d.get("hr_active"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0);
    r.exclude_founded_below = text_of(d.get("min_years"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| *n > 0);
    r.weekend = text_of(d.get("work_weekend")).filter(|w| w != "any");
    r.exclude_short_term_title = text_of(d.get("short_term_title"))
        .map(|v| v != "off" && v != "false")
        .unwrap_or(true);
    r.salary_ranges = salary_ranges_of(d.get("salary"));
    r
}

/// Boss 官方薪资档位（编码 → K 区间）。与 `scraper/boss.py::_BOSS_TIER_CODES` 逐项同源：
/// 前端存 `sal:a-b`，旧库可能存裸编码（`406`），两种都要认。
const SALARY_TIERS: [(u32, u32, &str); 6] = [
    (0, 3, "402"),
    (3, 5, "403"),
    (5, 10, "404"),
    (10, 20, "405"),
    (20, 50, "406"),
    (50, 9999, "407"),
];

/// `salary` 多选键 → K 区间表。`sal:a-b` 原样取；裸编码查官方档位表；
/// 脏值丢弃（宁可放行，也不让整次抓取起不来）。
fn salary_ranges_of(v: Option<&serde_json::Value>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for raw in list_of(v) {
        let code = raw.trim();
        let range = if let Some(rest) = code.strip_prefix("sal:") {
            rest.split_once('-')
                .and_then(|(a, b)| {
                    Some((
                        a.trim().parse::<u32>().ok()?,
                        b.trim().parse::<u32>().ok()?,
                    ))
                })
                .filter(|(lo, hi)| hi > lo)
                .map(|(lo, hi)| (lo, hi.min(9999)))
        } else {
            SALARY_TIERS
                .iter()
                .find(|(_, _, c)| *c == code)
                .map(|&(lo, hi, _)| (lo, hi))
        };
        if let Some(range) = range {
            if !out.contains(&range) {
                out.push(range);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::domain::app_config::collect_scrape_defaults;
    use crate::core::domain::rules::{reject_labeled, JobInput};

    /// 从「库里的原始键值对」出发走完整链路：落库文本 → collect → 映射。
    /// 落库文本形状与 `scrape_value_to_text` 一致：字符串原文、数组 JSON。
    fn defaults_from_pairs() -> serde_json::Value {
        let pairs: Vec<(String, String)> = vec![
            ("scrape_defaults.city".into(), "101280600".into()),
            ("scrape_defaults.salary".into(), r#"["406","sal:25-9999"]"#.into()),
            ("scrape_defaults.experience".into(), r#"["104"]"#.into()),
            ("scrape_defaults.degree".into(), r#"["203"]"#.into()),
            ("scrape_defaults.company_scale".into(), r#"["303","304"]"#.into()),
            ("scrape_defaults.finance_stage".into(), r#"["803"]"#.into()),
            ("scrape_defaults.industry".into(), r#"["100020"]"#.into()),
            ("scrape_defaults.job_type".into(), "1901".into()),
            ("scrape_defaults.max_pages".into(), "3".into()),
            ("scrape_defaults.exclude_keywords".into(), "外包,培训".into()),
            ("scrape_defaults.hr_active".into(), "3".into()),
            ("scrape_defaults.work_weekend".into(), "double".into()),
            ("scrape_defaults.min_years".into(), "5".into()),
            // 非 scrape_defaults 键必须被忽略
            ("exclude_keywords".into(), "应用级旧键,不该被读".into()),
        ];
        collect_scrape_defaults(&pairs)
    }

    #[test]
    fn filters_map_config_keys_to_protocol_keys() {
        let f = filters_from_defaults(&defaults_from_pairs());
        assert_eq!(f.city.as_deref(), Some("101280600"));
        assert_eq!(f.salary, vec!["406", "sal:25-9999"]);
        assert_eq!(f.experience, vec!["104"]);
        assert_eq!(f.degree, vec!["203"]);
        // 配置键 company_scale/finance_stage → 协议键 scale/stage
        assert_eq!(f.scale, vec!["303", "304"]);
        assert_eq!(f.stage, vec!["803"]);
        assert_eq!(f.industry, vec!["100020"]);
        assert_eq!(f.job_type.as_deref(), Some("1901"));
        assert_eq!(f.max_pages, Some(3));
    }

    #[test]
    fn salary_multi_select_maps_to_exact_ranges() {
        // 前端存 sal:a-b；裸编码是旧库 / 手工改库的写法，两种都要认
        let d = serde_json::json!({"salary": ["sal:18-25", "406", "407", "sal:bad", "999", "sal:30-20"]});
        let r = exclude_rules_from_defaults(&d);
        assert_eq!(r.salary_ranges, vec![(18, 25), (20, 50), (50, 9999)]);
        // 与 filters 下发给 sidecar 的 salary 编码互不影响
        assert_eq!(filters_from_defaults(&d).salary.len(), 6);
    }

    #[test]
    fn empty_defaults_mean_no_filters_and_builtin_rules() {
        let d = collect_scrape_defaults(&[]);
        assert_eq!(filters_from_defaults(&d), ScrapeFilters::default());
        assert_eq!(exclude_rules_from_defaults(&d), ExcludeRules::default());
    }

    #[test]
    fn dirty_values_fall_back_to_unlimited() {
        let d = serde_json::json!({
            "city": "   ",
            "max_pages": "99",
            "salary": "403,406",
            "hr_active": "any",
            "work_weekend": "any",
            "min_years": "",
        });
        let f = filters_from_defaults(&d);
        assert_eq!(f.city, None);
        assert_eq!(f.max_pages, Some(20), "越界翻页数夹到上限");
        assert_eq!(f.salary, vec!["403", "406"], "平文本薪资也要能切开");
        let r = exclude_rules_from_defaults(&d);
        assert_eq!(r.exclude_hr_active_days, None);
        assert_eq!(r.weekend, None);
        assert_eq!(r.exclude_founded_below, None);
        let zero = filters_from_defaults(&serde_json::json!({"max_pages": 0}));
        assert_eq!(zero.max_pages, Some(1), "0 页夹到下限");
    }

    #[test]
    fn user_exclude_words_replace_builtin_and_veto_on_hit() {
        let d = defaults_from_pairs();
        let r = exclude_rules_from_defaults(&d);
        assert_eq!(r.name_words, vec!["外包".to_string(), "培训".to_string()]);
        assert_eq!(r.exclude_hr_active_days, Some(3));
        assert_eq!(r.weekend.as_deref(), Some("double"));
        assert_eq!(r.exclude_founded_below, Some(5));

        // 用户词命中即否决（ingest 的硬规则否决走的正是 reject_labeled）
        let job = JobInput {
            company: Some("某外包服务公司"),
            title: Some("Java 开发"),
            ..Default::default()
        };
        let hit = reject_labeled(&job, &r).expect("用户排除词命中必须否决");
        assert_eq!(hit.0, "name_word");

        // 内置词表已被替换：默认词「金融」不再触发名称否决
        let fin = JobInput {
            company: Some("某金融科技公司"),
            title: Some("Java 开发"),
            ..Default::default()
        };
        assert!(
            reject_labeled(&fin, &r).map(|(rule, _)| rule != "name_word").unwrap_or(true),
            "用户词表之外的内置默认词不得再触发名称否决"
        );
    }

    /// 真实读库路径（内存临时库，不碰 data/*.db）：
    /// `set_config` 落库 → `config_pairs` → `collect` → 映射，与命令层同链。
    #[test]
    fn store_pairs_feed_the_same_mapping() {
        use crate::core::domain::store_ports::ConfigStore;
        use crate::core::infra::SqliteStore;
        let store = SqliteStore::open_in_memory().expect("内存库");
        store.set_config("scrape_defaults.city", Some("101280600")).unwrap();
        store.set_config("scrape_defaults.exclude_keywords", Some("外包,培训")).unwrap();
        store.set_config("scrape_defaults.max_pages", Some("3")).unwrap();
        let d = collect_scrape_defaults(&store.config_pairs().unwrap());
        let f = filters_from_defaults(&d);
        assert_eq!(f.city.as_deref(), Some("101280600"));
        assert_eq!(f.max_pages, Some(3));
        let r = exclude_rules_from_defaults(&d);
        assert_eq!(r.name_words, vec!["外包".to_string(), "培训".to_string()]);
    }

    #[test]
    fn blank_exclude_words_fall_back_to_builtin_veto() {
        let r = exclude_rules_from_defaults(&serde_json::json!({"exclude_keywords": "  ,、 "}));
        assert_eq!(r, ExcludeRules::default(), "空白词表回退内置否决词");
    }

    /// SPEC-004：三档字面量原样透传到 `ExcludeRules.weekend`（字面量集断言的
    /// 后端透传端）。透传是档位生效的唯一入口，这里丢值就静默不限。
    #[test]
    fn weekend_tier_literals_pass_through_verbatim() {
        for literal in ["double", "no_bigweek", "single"] {
            let r = exclude_rules_from_defaults(&serde_json::json!({"work_weekend": literal}));
            assert_eq!(r.weekend.as_deref(), Some(literal), "档位字面量必须原样透传");
        }
        let any = exclude_rules_from_defaults(&serde_json::json!({"work_weekend": "any"}));
        assert_eq!(any.weekend, None, "any → 不限");
    }

    /// 短期项目开关：缺席/脏值按默认开（等价旧行为），`off`/`false` 才关掉。
    #[test]
    fn short_term_title_switch_defaults_on() {
        let on = exclude_rules_from_defaults(&serde_json::json!({}));
        assert!(on.exclude_short_term_title);
        for off in ["off", "false"] {
            let r = exclude_rules_from_defaults(&serde_json::json!({"short_term_title": off}));
            assert!(!r.exclude_short_term_title, "{off} 应关掉短期排除");
        }
        let dirty = exclude_rules_from_defaults(&serde_json::json!({"short_term_title": "oops"}));
        assert!(dirty.exclude_short_term_title, "脏值按默认开");
    }
}
