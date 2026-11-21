//! 抓取筛选条件（旧版图 3）。
//!
//! # 落库与回读（集成批次已接通 `set_scrape_defaults`）
//!
//! `app/src-tauri/src/commands_config.rs::set_scrape_defaults` 收
//! `core/domain/app_config.rs::ScrapeDefaultsArgs` 的 14 个字段，
//! 逐项落成 `scrape_defaults.<键>`，并经 `get_app_config().scrape_defaults`
//! 回读。对应关系（面板控件 → 驼峰入参名）：
//!
//! | 控件 | 入参名 | 形态 |
//! |---|---|---|
//! | 城市 | `city` | 编码串 |
//! | 薪资 | `salary` | 编码数组 |
//! | 工作经验 | `experience` | 编码数组 |
//! | 学历 | `degree` | 编码数组 |
//! | 公司规模 | `companyScale` | 编码数组 |
//! | 融资阶段 | `financeStage` | 编码数组 |
//! | 行业 | `industry` | 编码数组 |
//! | 职位类型 | `jobType` | 编码串（空串 = 不限） |
//! | 搜索最大翻页次数 | `maxPages` | 数字 |
//! | 排除关键词 | `excludeKeywords` | 自由文本 |
//! | HR 活跃度 | `hrActive` | `any` / `1` / `3` / `7` |
//! | 工作制 | `workWeekend` | `any` / `double` / `no_bigweek` |
//! | JD 岗位要求排除 | `jdDegree` | 字符串数组 |
//! | 成立年限少于 | `minYears` | 数字 |
//!
//! **写入只提交变化过的字段**：`ScrapeDefaultsArgs` 的每个字段都是
//! `Option<Value>`，缺席 = 「不动」。整份重发会把「用户这次没碰的那项」
//! 一并写成当前控件值，在多窗口/两次面板实例之间互相覆盖。
//!
//! 回填顺序：先 `get_app_config().scrape_defaults`，**取不到才退回**
//! `localStorage` 的 `aja.scrape_filter_draft`（拨给本轮之前的旧草稿）。
//! 两个都读不到才用常量默认值。
//!
//! **搜索关键词不在这 14 项里，也不落默认值**：它是每次抓取的临时输入，
//! 由页面层 `ScrapePanel` 持有、直接进 `ScrapeArgs.keywords`；`ScrapeDefaultsArgs`
//! 本身没有这个字段，所以「上次搜的关键词」不会在下次开面板时自动回填。
//!
//! # 仍有的本地缓冲
//
// 薪资自定义的两个输入框（`salary_low` / `salary_high`）只是「待添加」的草稿，
// 不进后端、也不进 localStorage：它们的产物是加了 `sal:…` 编码之后的 `salary` 数组。

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::options::*;

/// 本地留档的 key。**只作为回填的兜底**：后端 `scrape_defaults` 读到值时不看它。
pub const DRAFT_KEY: &str = "aja.scrape_filter_draft";

/// 改动后的下发延迟。
///
/// 排除词 / 成立年限 / 翻页次数都是逐字符改的，`input` 事件每次按键都会触发；
/// 400ms 能把一次连续输入压成一次 IPC + 一次写库，又短到用户感觉不出延迟。
pub const SAVE_DEBOUNCE_MS: i32 = 400;

/// 九类筛选条件的本地状态（一个整体，方便序列化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterDraft {
    pub city: String,
    pub job_type: String,
    pub max_pages: u32,
    pub salaries: Vec<String>,
    pub salary_low: String,
    pub salary_high: String,
    pub experiences: Vec<String>,
    pub degrees: Vec<String>,
    pub scales: Vec<String>,
    pub stages: Vec<String>,
    pub industries: Vec<String>,
    pub exclude: String,
    pub hr_active: String,
    pub weekend: String,
    pub veto_degrees: Vec<String>,
    pub founded: String,
}

impl Default for FilterDraft {
    fn default() -> Self {
        Self {
            city: CITY_DEFAULT.to_string(),
            job_type: JOB_TYPE_DEFAULT.to_string(),
            max_pages: MAX_PAGES_DEFAULT,
            salaries: Vec::new(),
            salary_low: String::new(),
            salary_high: String::new(),
            experiences: Vec::new(),
            degrees: Vec::new(),
            scales: Vec::new(),
            stages: Vec::new(),
            industries: Vec::new(),
            exclude: DEFAULT_EXCLUDE.to_string(),
            hr_active: "any".to_string(),
            weekend: "any".to_string(),
            veto_degrees: Vec::new(),
            founded: String::new(),
        }
    }
}

// ---------------------------------------------------------------- 序列化
//
// 不引 serde_json 之外的依赖（Cargo.toml 是只读的，也不该为一份草稿加依赖），
// 直接手写一个行式格式：`key=value`，多值用 `,` 拼接。
// 单值里可能出现 `=` 或换行（排除词 textarea 是自由文本），所以：
// - 只按 **第一个** `=` 切分；
// - 值里的 `\` 转义成 `\\`、换行转义成 `\n`。

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn join_ids(v: &[String]) -> String {
    v.join(",")
}

fn split_ids(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

impl FilterDraft {
    pub fn encode(&self) -> String {
        vec![
            format!("city={}", esc(&self.city)),
            format!("job_type={}", esc(&self.job_type)),
            format!("max_pages={}", self.max_pages),
            format!("salaries={}", esc(&join_ids(&self.salaries))),
            format!("salary_low={}", esc(&self.salary_low)),
            format!("salary_high={}", esc(&self.salary_high)),
            format!("experiences={}", esc(&join_ids(&self.experiences))),
            format!("degrees={}", esc(&join_ids(&self.degrees))),
            format!("scales={}", esc(&join_ids(&self.scales))),
            format!("stages={}", esc(&join_ids(&self.stages))),
            format!("industries={}", esc(&join_ids(&self.industries))),
            format!("exclude={}", esc(&self.exclude)),
            format!("hr_active={}", esc(&self.hr_active)),
            format!("weekend={}", esc(&self.weekend)),
            format!("veto_degrees={}", esc(&join_ids(&self.veto_degrees))),
            format!("founded={}", esc(&self.founded)),
        ]
        .join("\n")
    }

    /// 解析。缺字段 / 值非法一律回默认值——草稿是缓存，坏掉就当没存过，
    /// 绝不能因为一份脏数据让面板打不开。
    pub fn decode(raw: &str) -> Self {
        let mut d = FilterDraft::default();
        for line in raw.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = unesc(v);
            match k.trim() {
                "city" if CITIES.iter().any(|(c, _)| *c == v) => d.city = v,
                "job_type" if JOB_TYPE_OPTIONS.iter().any(|(c, _)| *c == v) => d.job_type = v,
                "max_pages" => {
                    d.max_pages = v
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .map(|n| n.clamp(MAX_PAGES_MIN, MAX_PAGES_MAX))
                        .unwrap_or(MAX_PAGES_DEFAULT)
                }
                // 白名单过滤：删掉常量表里已不存在的旧编码
                "salaries" => {
                    d.salaries = split_ids(&v)
                        .into_iter()
                        .filter(|c| {
                            SALARY_OPTIONS.iter().any(|(x, _)| x == c) || is_custom_salary(c)
                        })
                        .collect()
                }
                "salary_low" => d.salary_low = v,
                "salary_high" => d.salary_high = v,
                "experiences" => {
                    d.experiences =
                        keep_known(split_ids(&v), EXPERIENCE_OPTIONS.iter().map(|(c, _)| *c))
                }
                "degrees" => {
                    d.degrees = keep_known(split_ids(&v), DEGREE_OPTIONS.iter().map(|(c, _)| *c))
                }
                "scales" => {
                    d.scales = keep_known(split_ids(&v), SCALE_OPTIONS.iter().map(|(c, _)| *c))
                }
                "stages" => {
                    d.stages = keep_known(split_ids(&v), STAGE_OPTIONS.iter().map(|(c, _)| *c))
                }
                "industries" => {
                    d.industries =
                        keep_known(split_ids(&v), INDUSTRY_OPTIONS.iter().map(|(c, _)| *c))
                }
                "exclude" => d.exclude = v,
                "hr_active" if HR_ACTIVE_OPTIONS.iter().any(|o| o.value == v) => d.hr_active = v,
                "weekend" if WEEKEND_OPTIONS.iter().any(|o| o.value == v) => d.weekend = v,
                "veto_degrees" => {
                    d.veto_degrees = keep_known(split_ids(&v), DEGREE_VETO_OPTIONS.iter().copied())
                }
                "founded" => d.founded = v,
                _ => {}
            }
        }
        d
    }
}

fn keep_known(items: Vec<String>, known: impl Iterator<Item = &'static str>) -> Vec<String> {
    let set: Vec<&'static str> = known.collect();
    // 按常量表顺序输出，保证回填后的 chip 顺序与第一次点选无关
    set.into_iter()
        .filter(|k| items.iter().any(|x| x == k))
        .map(|k| k.to_string())
        .collect()
}

// ---------------------------------------------------------------- localStorage

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn load_draft() -> FilterDraft {
    storage()
        .and_then(|s| s.get_item(DRAFT_KEY).ok().flatten())
        .map(|raw| FilterDraft::decode(&raw))
        .unwrap_or_default()
}

/// 写回 localStorage。**只作兜底**：后端 `scrape_defaults` 有值时以它为准。
///
/// 写不进去（隐私模式 / 配额满）不打断用户操作，静默即可。
pub fn save_draft(d: &FilterDraft) {
    if let Some(s) = storage() {
        let _ = s.set_item(DRAFT_KEY, &d.encode());
    }
}

// ---------------------------------------------------------------- 下发 / 回填
//
// 纯函数集中在这里，`cargo test`（host 目标）能覆盖映射与 diff 的全部规则；
// WebView 只负责「什么时候调」——定时器、IPC、信号都不在本单元。

/// 后端 `scrape_defaults.<相对名>` → 驼峰入参名。
pub const BACKEND_KEYS: [(&str, &str); 14] = [
    ("city", "city"),
    ("salary", "salary"),
    ("experience", "experience"),
    ("degree", "degree"),
    ("company_scale", "companyScale"),
    ("finance_stage", "financeStage"),
    ("industry", "industry"),
    ("job_type", "jobType"),
    ("max_pages", "maxPages"),
    ("exclude_keywords", "excludeKeywords"),
    ("hr_active", "hrActive"),
    ("work_weekend", "workWeekend"),
    ("jd_degree", "jdDegree"),
    ("min_years", "minYears"),
];

/// 面板控件当前值 → `ScrapeDefaultsArgs` 形状的 14 个字段。
///
/// 顺序与 [`BACKEND_KEYS`] 一致，方便 [`diff_args`] 按位置比较。
/// 数组在控件里是 `Vec<String>`，这里统一成 `Value::Array`——
/// 后端按 JSON 文本落库，读侧再还原成数组，空数组与「没选」是同一件事。
fn draft_values(d: &FilterDraft) -> Vec<serde_json::Value> {
    use serde_json::Value;
    let arr = |v: &[String]| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect());
    vec![
        Value::String(d.city.clone()),
        arr(&d.salaries),
        arr(&d.experiences),
        arr(&d.degrees),
        arr(&d.scales),
        arr(&d.stages),
        arr(&d.industries),
        Value::String(d.job_type.clone()),
        Value::Number(serde_json::Number::from(d.max_pages)),
        Value::String(d.exclude.clone()),
        Value::String(d.hr_active.clone()),
        Value::String(d.weekend.clone()),
        arr(&d.veto_degrees),
        // 留空 = 不排除：不发这个字段，让后端保留上一次的值。
        // （发 0 会被理解为「成立 0 年以内全部排除」，那是另一条规则。）
        d.founded
            .trim()
            .parse::<u32>()
            .ok()
            .map(|n| Value::Number(serde_json::Number::from(n)))
            .unwrap_or(Value::Null),
    ]
}

/// 「当前控件值」与「最近一次已知的后端值」的差异，**只含变化过的字段**。
///
/// `known` 是 `get_app_config().scrape_defaults` 的原样回执（`{}` = 一个都没配过）。
/// 已知值缺失的相对名（`known` 里没有这个键）一律视为「变化」——用户第一次改动
/// 一个从未落库的项时，这一项必须被写进去。
///
/// 返回的是 `ScrapeDefaultsArgs` 的 JSON 形状（驼峰键，缺席 = 不动），
/// 空对象表示没有任何变化，调用方**不要发 IPC**。
pub fn diff_args(known: &serde_json::Value, d: &FilterDraft) -> serde_json::Value {
    use serde_json::{Map, Value};
    let known_obj = known.as_object();
    let mut out = Map::new();
    for ((rel, camel), cur) in BACKEND_KEYS.iter().zip(draft_values(d)) {
        // 留空 = 不排除：前端刻意不发，也不能因为「已知里没有」就被判成变化。
        if *rel == "min_years" && cur.is_null() {
            continue;
        }
        let prev = known_obj.and_then(|o| o.get(*rel));
        if prev == Some(&cur) {
            continue;
        }
        out.insert((*camel).to_string(), cur);
    }
    Value::Object(out)
}

/// 把 `get_app_config().scrape_defaults` 回填到一份草稿上（就地覆盖）。
///
/// 规则与 [`FilterDraft::decode`] 一样偏保守：**只收认识的值**，脏数据
/// （编码不在常量表里、类型不对、超出范围）一律保留原值——草稿是缓存，
/// 一份坏配置不该让面板打不开或把用户的选择清空。
/// 返回真正被采纳的字段数（`0` = 后端一个可用值都没有，调用方应退回 localStorage）。
pub fn apply_backend(d: &mut FilterDraft, backend: &serde_json::Value) -> usize {
    use serde_json::Value;
    let Some(obj) = backend.as_object() else {
        return 0;
    };
    let mut n = 0usize;

    let str_of = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            // 数字编码（`101010100`）被某些路径按数字存过
            Value::Number(num) => Some(num.to_string()),
            _ => None,
        }
    };
    let strs_of = |v: &Value| -> Option<Vec<String>> {
        let arr = v.as_array()?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(str_of(item)?);
        }
        Some(out)
    };

    for (rel, val) in obj {
        match rel.as_str() {
            "city" => {
                if let Some(v) = str_of(val).filter(|v| CITIES.iter().any(|(c, _)| *c == v)) {
                    d.city = v;
                    n += 1;
                }
            }
            // 空串是「不限」，是合法值
            "job_type" => {
                if let Some(v) = str_of(val)
                    .filter(|v| v.is_empty() || JOB_TYPE_OPTIONS.iter().any(|(c, _)| *c == v))
                {
                    d.job_type = v;
                    n += 1;
                }
            }
            "max_pages" => {
                if let Some(v) = val
                    .as_u64()
                    .map(|v| (v as u32).clamp(MAX_PAGES_MIN, MAX_PAGES_MAX))
                {
                    d.max_pages = v;
                    n += 1;
                }
            }
            "salary" => {
                if let Some(v) = strs_of(val).map(|v| {
                    v.into_iter()
                        .filter(|c| {
                            SALARY_OPTIONS.iter().any(|(x, _)| x == c) || is_custom_salary(c)
                        })
                        .collect::<Vec<_>>()
                }) {
                    d.salaries = v;
                    n += 1;
                }
            }
            "experience" => {
                if let Some(v) =
                    strs_of(val).map(|v| keep_known(v, EXPERIENCE_OPTIONS.iter().map(|(c, _)| *c)))
                {
                    d.experiences = v;
                    n += 1;
                }
            }
            "degree" => {
                if let Some(v) =
                    strs_of(val).map(|v| keep_known(v, DEGREE_OPTIONS.iter().map(|(c, _)| *c)))
                {
                    d.degrees = v;
                    n += 1;
                }
            }
            "company_scale" => {
                if let Some(v) =
                    strs_of(val).map(|v| keep_known(v, SCALE_OPTIONS.iter().map(|(c, _)| *c)))
                {
                    d.scales = v;
                    n += 1;
                }
            }
            "finance_stage" => {
                if let Some(v) =
                    strs_of(val).map(|v| keep_known(v, STAGE_OPTIONS.iter().map(|(c, _)| *c)))
                {
                    d.stages = v;
                    n += 1;
                }
            }
            "industry" => {
                // 行业允许自定义编码（面板上有「自定义编码回车添加」），所以只做类型过滤
                if let Some(v) = strs_of(val) {
                    d.industries = v;
                    n += 1;
                }
            }
            "exclude_keywords" => {
                if let Some(v) = str_of(val) {
                    d.exclude = v;
                    n += 1;
                }
            }
            "hr_active" => {
                if let Some(v) = str_of(val).filter(|v| HR_ACTIVE_OPTIONS.iter().any(|o| o.value == v))
                {
                    d.hr_active = v;
                    n += 1;
                }
            }
            "work_weekend" => {
                if let Some(v) = str_of(val).filter(|v| WEEKEND_OPTIONS.iter().any(|o| o.value == v))
                {
                    d.weekend = v;
                    n += 1;
                }
            }
            "jd_degree" => {
                if let Some(v) = strs_of(val)
                    .map(|v| keep_known(v, DEGREE_VETO_OPTIONS.iter().copied()))
                {
                    d.veto_degrees = v;
                    n += 1;
                }
            }
            "min_years" => {
                if let Some(v) = val.as_u64() {
                    d.founded = v.to_string();
                    n += 1;
                } else if let Some(v) = str_of(val).filter(|v| !v.trim().is_empty()) {
                    // 历史数据可能按字符串存过；非数字就当没配
                    if v.trim().parse::<u32>().is_ok() {
                        d.founded = v.trim().to_string();
                        n += 1;
                    }
                }
            }
            _ => {}
        }
    }
    n
}

// ---------------------------------------------------------------- 多选切换

/// 多选切换：已有则删，没有则插入到**常量表顺序**的位置。
///
/// 保持常量表顺序而不是「点选顺序」有两个好处：chip 不会因为点选先后乱跳，
/// 回填（`keep_known`）与实际点选的结果是同一套顺序，界面不会跳一下。
pub fn toggle_code(sel: Vec<String>, code: &str, table: &[Opt]) -> Vec<String> {
    if sel.iter().any(|x| x == code) {
        return sel.into_iter().filter(|x| x != code).collect();
    }
    let mut next: Vec<String> = sel;
    next.push(code.to_string());
    next.sort_by_key(|c| table.iter().position(|(k, _)| k == c).unwrap_or(usize::MAX));
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_roundtrip_keeps_every_field() {
        let d = FilterDraft {
            city: "101010100".into(),
            job_type: "1902".into(),
            max_pages: 7,
            salaries: vec!["sal:8-12".into(), "sal:25-40".into()],
            salary_low: "25".into(),
            salary_high: "40".into(),
            experiences: vec!["104".into()],
            degrees: vec!["203".into(), "204".into()],
            scales: vec!["303".into()],
            stages: vec!["807".into()],
            industries: vec!["100020".into()],
            exclude: "外包, 驻场\n第二行".into(),
            hr_active: "3".into(),
            weekend: "double".into(),
            veto_degrees: vec!["本科".into()],
            founded: "5".into(),
        };
        let back = FilterDraft::decode(&d.encode());
        assert_eq!(back, d);
    }

    #[test]
    fn decode_tolerates_garbage() {
        let d = FilterDraft::decode("city=999999\nmax_pages=999\nhr_active=oops\n没等号\n=");
        let default = FilterDraft::default();
        assert_eq!(d.city, default.city);
        assert_eq!(d.max_pages, MAX_PAGES_MAX);
        assert_eq!(d.hr_active, default.hr_active);
        // 空串 / 缺失 → 全默认
        assert_eq!(FilterDraft::decode(""), default);
    }

    #[test]
    fn decode_drops_unknown_codes_and_restores_table_order() {
        // 乱序 + 含一个已不存在的旧编码
        let d = FilterDraft::decode("degrees=204,209,999");
        assert_eq!(d.degrees, vec!["204", "209"]);
        // 按常量表顺序：202 在 204 之前
        let d2 = FilterDraft::decode("degrees=204,202");
        assert_eq!(d2.degrees, vec!["202", "204"]);
    }

    #[test]
    fn toggle_keeps_table_order() {
        let mut sel = Vec::new();
        sel = toggle_code(sel, "204", &DEGREE_OPTIONS);
        sel = toggle_code(sel, "202", &DEGREE_OPTIONS);
        // 先点硕士再点大专，结果仍按常量表顺序
        assert_eq!(sel, vec!["202", "204"]);
        // 再点一次取消
        sel = toggle_code(sel, "202", &DEGREE_OPTIONS);
        assert_eq!(sel, vec!["204"]);
    }

    #[test]
    fn backend_keys_cover_all_fourteen_args_with_camel_case() {
        // 14 项全部下发；驼峰名必须与 `ScrapeDefaultsArgs` 的 alias 逐字一致，
        // 否则 serde 不报错、直接丢弃（AGENTS.md §3.2 第 1 条）
        assert_eq!(BACKEND_KEYS.len(), 14);
        let names: Vec<&str> = BACKEND_KEYS.iter().map(|(_, c)| *c).collect();
        for want in [
            "companyScale",
            "financeStage",
            "jobType",
            "maxPages",
            "excludeKeywords",
            "hrActive",
            "workWeekend",
            "jdDegree",
            "minYears",
        ] {
            assert!(names.contains(&want), "缺少驼峰名 {want}");
        }
        for camel in &names {
            assert!(!camel.contains('_'), "驼峰名不得带下划线：{camel}");
        }
    }

    #[test]
    fn diff_args_emits_only_changed_fields() {
        let d = FilterDraft::default();
        // 一个都没配过：所有项都算「变化」，14 项全发
        let first = diff_args(&serde_json::json!({}), &d);
        assert_eq!(
            first.as_object().unwrap().len(),
            13,
            "14 项里 minYears 留空不发，其余 13 项"
        );

        // 与后端完全一致：空对象，调用方不该发 IPC。
        // 基线按真实路径构造——首轮下发成功后，前端把已发的差异并进 `known`，
        // `known` 里因此没有 `min_years` 这个键（留空本来就不发），
        // 而 `diff_args` 对留空的 minYears 直接跳过，所以这里必须是空。
        let known = serde_json::json!({
            "city": d.city,
            "salary": d.salaries,
            "experience": d.experiences,
            "degree": d.degrees,
            "company_scale": d.scales,
            "finance_stage": d.stages,
            "industry": d.industries,
            "job_type": d.job_type,
            "max_pages": d.max_pages,
            "exclude_keywords": d.exclude,
            "hr_active": d.hr_active,
            "work_weekend": d.weekend,
            "jd_degree": d.veto_degrees,
        });
        assert!(
            diff_args(&known, &d).as_object().unwrap().is_empty(),
            "与后端一致时不得产生任何下发"
        );

        // 只改城市：只有 city 出去
        let mut d2 = d.clone();
        d2.city = "101010100".to_string();
        let out = diff_args(&known, &d2);
        assert_eq!(out.as_object().unwrap().len(), 1);
        assert_eq!(out["city"], serde_json::json!("101010100"));
    }

    #[test]
    fn diff_args_skips_blank_min_years() {
        let d = FilterDraft::default();
        let known = serde_json::json!({ "min_years": 5 });
        // 控件清空 = 不排除：不发，后端保留 5（发 0 会变成「成立 0 年以内全排除」）
        let out = diff_args(&known, &d);
        assert!(
            !out.as_object().unwrap().contains_key("minYears"),
            "清空留空时不得下发 minYears"
        );
    }

    #[test]
    fn apply_backend_fills_and_rejects_garbage() {
        let mut d = FilterDraft::default();
        let n = apply_backend(
            &mut d,
            &serde_json::json!({
                "city": "101010100",
                "job_type": "",
                "max_pages": 7,
                "degrees": "ignored-top-level",
                "degree": ["202", "999"],
                "hr_active": "oops",
                "min_years": "5",
            }),
        );
        assert_eq!(d.city, "101010100");
        assert_eq!(d.job_type, "", "空串是「不限」，属于合法值");
        assert_eq!(d.max_pages, 7);
        assert_eq!(d.degrees, vec!["202"], "不认识的编码要丢掉");
        assert_eq!(d.hr_active, "any", "非法值保留原样");
        assert_eq!(d.founded, "5", "字符串形态的 min_years 也要收");
        assert_eq!(n, 5);
    }

    #[test]
    fn apply_backend_returns_zero_for_empty_config() {
        let mut d = FilterDraft::default();
        // 一个都没配过：回填 0 项 → 调用方退回 localStorage
        assert_eq!(apply_backend(&mut d, &serde_json::json!({})), 0);
        assert_eq!(apply_backend(&mut d, &serde_json::Value::Null), 0);
    }
}

// ---------------------------------------------------------------- 视图

/// 一行标题 + 一排 chip 的多选区（旧版 `ChipRow`）。
///
/// 放在本文件而不是 `components/`：`components/**` 只读，且这个形状只有筛选面板用。
#[component]
fn ChipRow(
    title: &'static str,
    hint: &'static str,
    options: Vec<Opt>,
    sel: RwSignal<Vec<String>>,
    tid_prefix: &'static str,
    clear_tid: Option<&'static str>,
    on_change: Callback<()>,
) -> impl IntoView {
    let table: Vec<Opt> = options.clone();
    view! {
        <div class="filter-row">
            <div class="filter-row-head">
                <span class="filter-row-title" title=hint>{title}</span>
                {clear_tid.map(|ct| view! {
                    <button
                        type="button"
                        class="filter-clear"
                        data-testid=ct
                        on:click=move |_| {
                            sel.set(Vec::new());
                            on_change.run(());
                        }
                    >
                        "清空"
                    </button>
                })}
            </div>
            <div class="filter-chips">
                {options.into_iter().map(|(code, name)| {
                    let table = table.clone();
                    view! {
                        <button
                            type="button"
                            class=move || if sel.get().iter().any(|x| x == code) {
                                "chip as-btn is-on"
                            } else {
                                "chip as-btn"
                            }
                            data-testid=format!("{tid_prefix}-{}", tid_safe(code))
                            title=name
                            on:click=move |_| {
                                let cur = sel.get_untracked();
                                sel.set(toggle_code(cur, code, &table));
                                on_change.run(());
                            }
                        >
                            {name}
                        </button>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

/// 关键词输入（回车添加，可一次敲多个）。
#[component]
fn KeywordBox(keywords: RwSignal<String>, fail: Callback<String>) -> impl IntoView {
    // 输入框自己持有「正在敲、还没回车」的文本，与 `keywords` 分开：
    // 混在一起会让每次按键都重排整串 chip，光标会跳。
    let draft = RwSignal::new(String::new());

    let commit = move || {
        use crate::lib_ui::text::split_list;
        let raw = draft.get_untracked();
        let mut add = split_list(&raw);
        if add.is_empty() {
            return;
        }
        let mut cur = crate::lib_ui::text::split_list(&keywords.get_untracked());
        let before = cur.len();
        for k in add.drain(..) {
            if !cur.iter().any(|x| *x == k) {
                cur.push(k);
            }
        }
        draft.set(String::new());
        if cur.len() == before {
            fail.run("这个关键词已经在列表里了".to_string());
            return;
        }
        keywords.set(cur.join(", "));
    };

    view! {
        <div class="filter-kw">
            <div class="filter-row-head">
                <span class="filter-row-title">"关键词（可多个，逐个搜索）"</span>
            </div>
            <div class="filter-chips">
                {move || {
                    use crate::lib_ui::text::split_list;
                    split_list(&keywords.get()).into_iter().map(|kw| {
                        let kw_txt = kw.clone();
                        let kw_title = kw.clone();
                        let kw_tid = kw.clone();
                        view! {
                            <span class="chip is-on" data-testid="scrape-filter-kw-chip" title=kw_title>
                                {kw_txt}
                                <button
                                    type="button"
                                    class="chip-x"
                                    data-testid=format!("scrape-filter-kw-del-{}", kw_tid)
                                    title="删除这个关键词"
                                    on:click=move |_| {
                                        use crate::lib_ui::text::split_list;
                                        let mut cur = split_list(&keywords.get_untracked());
                                        cur.retain(|x| *x != kw);
                                        keywords.set(cur.join(", "));
                                    }
                                >
                                    "×"
                                </button>
                            </span>
                        }
                    }).collect_view()
                }}
            </div>
            <div class="filter-kw-input">
                // 这一个 input 同时承担两个契约名：`tools/gui_e2e.py` 的
                // `scrape-keywords`（16 项不可改名之一）与 SPEC 4.4 的
                // `scrape-filter-kw-input`。第二个用 data-alias 记录，
                // 免得后人以为漏了一个控件、又造一个新的变成两个输入框。
                <input
                    class="input"
                    data-testid="scrape-keywords"
                    data-alias="scrape-filter-kw-input"
                    placeholder="输入后回车添加（可空格/逗号分隔多个）"
                    prop:value=move || draft.get()
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            ev.prevent_default();
                            commit();
                        }
                    }
                />
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="scrape-filter-kw-add-btn"
                    on:click=move |_| commit()
                >
                    "添加"
                </button>
            </div>
        </div>
    }
}

/// 从事件里取输入框的值。
///
/// `dyn_cast` 来自 `JsCast`。项目里 `web-sys` 开了 `HtmlInputElement` 等特性，
/// 但 `EventTarget` 的 `dyn_cast` 需要 trait 在作用域内——所以这里显式引入，
/// 且只引一次（在嵌套块里再 `use` 一次编译器会报 unused）。
fn event_target_value(ev: &leptos::ev::Event) -> String {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok().map(|i| i.value()))
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlTextAreaElement>()
                    .ok()
                    .map(|i| i.value())
            })
        })
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlSelectElement>()
                    .ok()
                    .map(|i| i.value())
            })
        })
        .unwrap_or_default()
}

/// 筛选面板主体（展开时才渲染）。`notify`/`fail` 供「读取期望职位」用。
#[component]
pub fn ScrapeFilterPanel(
    keywords: RwSignal<String>,
    notify: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let draft0 = load_draft();
    let city = RwSignal::new(draft0.city.clone());
    let job_type = RwSignal::new(draft0.job_type.clone());
    let max_pages = RwSignal::new(draft0.max_pages);
    let salaries = RwSignal::new(draft0.salaries.clone());
    let salary_low = RwSignal::new(draft0.salary_low.clone());
    let salary_high = RwSignal::new(draft0.salary_high.clone());
    let experiences = RwSignal::new(draft0.experiences.clone());
    let degrees = RwSignal::new(draft0.degrees.clone());
    let scales = RwSignal::new(draft0.scales.clone());
    let stages = RwSignal::new(draft0.stages.clone());
    let industries = RwSignal::new(draft0.industries.clone());
    let exclude = RwSignal::new(draft0.exclude.clone());
    let hr_active = RwSignal::new(draft0.hr_active.clone());
    let weekend = RwSignal::new(draft0.weekend.clone());
    let veto_degrees = RwSignal::new(draft0.veto_degrees.clone());
    let founded = RwSignal::new(draft0.founded.clone());
    let industry_custom = RwSignal::new(String::new());

    // 期望职位：本面板自己拉，不依赖 `expectations.rs` 的按钮
    let expects = RwSignal::new(Vec::<crate::api::types::Expectation>::new());

    // 把 14 个控件信号读成一份草稿（`salary_low/high` 只是待添加的草稿框，
    // 不进后端也不进 localStorage——它们的产物是 `salary` 数组）
    let snapshot = move || FilterDraft {
        city: city.get_untracked(),
        job_type: job_type.get_untracked(),
        max_pages: max_pages.get_untracked(),
        salaries: salaries.get_untracked(),
        salary_low: String::new(),
        salary_high: String::new(),
        experiences: experiences.get_untracked(),
        degrees: degrees.get_untracked(),
        scales: scales.get_untracked(),
        stages: stages.get_untracked(),
        industries: industries.get_untracked(),
        exclude: exclude.get_untracked(),
        hr_active: hr_active.get_untracked(),
        weekend: weekend.get_untracked(),
        veto_degrees: veto_degrees.get_untracked(),
        founded: founded.get_untracked(),
    };

    // 「最近一次已知的后端值」。回填成功后是 `get_app_config().scrape_defaults`，
    // 之后每次下发成功就并进已发的差异——`diff_args` 拿它做基线，保证
    // 「只发变化过的字段」在连改多次时依然成立。
    let known = StoredValue::new(serde_json::json!({}));
    // 下发节流句柄：每次改动取消上一个定时器，只有停手 400ms 才真的发
    let pending = StoredValue::new(None::<TimeoutHandle>);

    // 任一控件变化 → 落 localStorage 兜底，并（防抖）把差异下发后端
    let save = Callback::new(move |_: ()| {
        let d = snapshot();
        save_draft(&d);

        if let Some(h) = pending.get_value() {
            h.clear();
        }
        let handle = set_timeout_with_handle(
            move || {
                let args = diff_args(&known.get_value(), &d);
                if args.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                    return; // 没有任何变化：不发 IPC，也不写库
                }
                spawn_local(async move {
                    match crate::api::set_scrape_defaults(args.clone()).await {
                        Ok(_) => {
                            // 已发的差异并进基线：下一轮只会算出真正的新变化
                            known.update_value(|k| {
                                let (Some(dst), Some(src)) = (k.as_object_mut(), args.as_object())
                                else {
                                    return;
                                };
                                for (rel, camel) in BACKEND_KEYS {
                                    if let Some(v) = src.get(camel) {
                                        dst.insert(rel.to_string(), v.clone());
                                    }
                                }
                            });
                        }
                        Err(e) => fail.run(format!("保存抓取默认条件失败：{e}")),
                    }
                });
            },
            std::time::Duration::from_millis(SAVE_DEBOUNCE_MS as u64),
        );
        // 定时器设不出来只可能是 JS 环境异常；此时本面板照常可用，
        // 只是不再自动下发（localStorage 兜底仍在），不必拦用户
        if let Ok(h) = handle {
            pending.set_value(Some(h));
        }
    });
    on_cleanup(move || {
        if let Some(h) = pending.get_value() {
            h.clear();
        }
    });

    // 初始化回填：先问后端（`scrape_defaults`），一项可用值都没有才退回 localStorage
    {
        use leptos::task::spawn_local;
        let f = fail;
        spawn_local(async move {
            // `AppConfigReply.scrape_defaults` 已与命令层同源（`api/types.rs`）。
            // 取不到就退回 `{}`：后端从没配过时它也返回 `{}`，两条路径同义。
            let backend = crate::api::get_app_config()
                .await
                .ok()
                .map(|r| r.scrape_defaults)
                .filter(|v| v.is_object())
                .unwrap_or_else(|| serde_json::json!({}));
            let mut d = snapshot();
            let n = apply_backend(&mut d, &backend);
            if n == 0 {
                // 后端从没配过：保留 `load_draft()` 起底的本地草稿
                known.set_value(backend);
                return;
            }
            city.set(d.city.clone());
            job_type.set(d.job_type.clone());
            max_pages.set(d.max_pages);
            salaries.set(d.salaries.clone());
            experiences.set(d.experiences.clone());
            degrees.set(d.degrees.clone());
            scales.set(d.scales.clone());
            stages.set(d.stages.clone());
            industries.set(d.industries.clone());
            exclude.set(d.exclude.clone());
            hr_active.set(d.hr_active.clone());
            weekend.set(d.weekend.clone());
            veto_degrees.set(d.veto_degrees.clone());
            founded.set(d.founded.clone());
            save_draft(&d);
            known.set_value(backend);
            let _ = &f;
        });
    }

    let on_load_expect = move |_| {
        use leptos::task::spawn_local;
        let f = fail;
        spawn_local(async move {
            match crate::api::list_expectations().await {
                Ok(v) => {
                    let n = v.len();
                    expects.set(v);
                    notify.run(format!("已读取 {n} 条求职期望"));
                }
                Err(e) => f.run(format!("读取期望职位失败：{e}")),
            }
        });
    };

    let add_salary = move |_| {
        let Some(code) = salary_code(&salary_low.get_untracked(), &salary_high.get_untracked())
        else {
            fail.run("请先填薪资下限或上限".to_string());
            return;
        };
        let mut cur = salaries.get_untracked();
        if !cur.iter().any(|x| *x == code) {
            cur.push(code);
            salaries.set(cur);
        }
        salary_low.set(String::new());
        salary_high.set(String::new());
        save.run(());
    };

    view! {
        <div class="filter-body" data-testid="scrape-filter-body">
            <KeywordBox keywords=keywords fail=fail/>

            <div class="filter-expect">
                <button class="chip as-btn" data-testid="scrape-filter-expect-btn" on:click=on_load_expect>
                    "读取期望职位"
                </button>
                {move || {
                    let items = expects.get();
                    if items.is_empty() {
                        return None;
                    }
                    Some(items.into_iter().enumerate().map(|(i, e)| {
                        let name = e.name.clone();
                        view! {
                            <button
                                class="chip as-btn"
                                data-testid=format!("scrape-filter-expect-{i}")
                                title=format!("{} · {}", e.city, e.salary)
                                on:click=move |_| {
                                    use crate::lib_ui::text::split_list;
                                    let mut cur = split_list(&keywords.get_untracked());
                                    if !cur.iter().any(|x| *x == name) {
                                        cur.push(name.clone());
                                        keywords.set(cur.join(", "));
                                    }
                                }
                            >
                                {e.name}
                            </button>
                        }
                    }).collect_view())
                }}
                <span class="spacer"></span>
                <span class="dim filter-mini">"推荐"</span>
                {RECOMMEND_KEYWORDS.iter().enumerate().map(|(i, kw)| {
                    let kw = *kw;
                    view! {
                        <button
                            class="chip as-btn"
                            data-testid=format!("scrape-filter-recommend-{i}")
                            on:click=move |_| {
                                use crate::lib_ui::text::split_list;
                                let mut cur = split_list(&keywords.get_untracked());
                                if !cur.iter().any(|x| *x == kw) {
                                    cur.push(kw.to_string());
                                    keywords.set(cur.join(", "));
                                }
                            }
                        >
                            {kw}
                        </button>
                    }
                }).collect_view()}
            </div>

            // ---------- 三列：城市 / 职位类型 / 翻页次数 ----------
            <div class="filter-grid3">
                <label class="field">
                    <div class="label">"城市"</div>
                    <select
                        class="input"
                        data-testid="scrape-filter-city"
                        prop:value=move || city.get()
                        on:change=move |ev| {
                            city.set(event_target_value(&ev));
                            save.run(());
                        }
                    >
                        {CITIES.iter().map(|(code, name)| view! {
                            <option value=*code>{*name}</option>
                        }).collect_view()}
                    </select>
                </label>
                <label class="field">
                    <div class="label">"职位类型"</div>
                    <select
                        class="input"
                        data-testid="scrape-filter-job-type"
                        prop:value=move || job_type.get()
                        on:change=move |ev| {
                            job_type.set(event_target_value(&ev));
                            save.run(());
                        }
                    >
                        <option value="">"不限"</option>
                        {JOB_TYPE_OPTIONS.iter().map(|(code, name)| view! {
                            <option value=*code>{*name}</option>
                        }).collect_view()}
                    </select>
                </label>
                <label class="field">
                    <div class="label">
                        "搜索最大翻页次数"
                        <span class="field-hint">"1-20，越大越慢"</span>
                    </div>
                    <input
                        class="input input-num"
                        type="number"
                        min=MAX_PAGES_MIN
                        max=MAX_PAGES_MAX
                        data-testid="scrape-filter-max-pages"
                        prop:value=move || max_pages.get().to_string()
                        on:change=move |ev| {
                            let n = event_target_value(&ev).trim().parse::<u32>().unwrap_or(MAX_PAGES_DEFAULT);
                            max_pages.set(n.clamp(MAX_PAGES_MIN, MAX_PAGES_MAX));
                            save.run(());
                        }
                    />
                </label>
            </div>

            // ---------- 两列：薪资 / 经验 / 学历 / 规模 / 阶段 ----------
            <div class="filter-grid2">
                <div class="filter-row">
                    <div class="filter-row-head">
                        <span class="filter-row-title" title="多选，命中任一档即算命中">"薪资"</span>
                        <button
                            type="button"
                            class="filter-clear"
                            data-testid="scrape-filter-salary-clear"
                            on:click=move |_| {
                                salaries.set(Vec::new());
                                save.run(());
                            }
                        >
                            "清空"
                        </button>
                    </div>
                    <div class="filter-chips">
                        {SALARY_OPTIONS.iter().map(|(code, name)| {
                            let code = *code;
                            view! {
                                <button
                                    type="button"
                                    class=move || if salaries.get().iter().any(|x| x == code) {
                                        "chip as-btn is-on"
                                    } else {
                                        "chip as-btn"
                                    }
                                    data-testid=format!("scrape-filter-salary-{}", tid_safe(code))
                                    on:click=move |_| {
                                        let cur = salaries.get_untracked();
                                        if cur.iter().any(|x| *x == code) {
                                            salaries.set(cur.into_iter().filter(|x| *x != code).collect());
                                        } else {
                                            let mut n = cur;
                                            n.push(code.to_string());
                                            salaries.set(n);
                                        }
                                        save.run(());
                                    }
                                >
                                    {*name}
                                </button>
                            }
                        }).collect_view()}
                        // 自定义档（带 × 删除）
                        {move || salaries.get().into_iter().filter(|c| is_custom_salary(c)).map(|c| {
                            let label = salary_label(&c);
                            let tid = format!("scrape-filter-salary-{}", tid_safe(&c));
                            let del_tid = format!("scrape-filter-salary-del-{}", tid_safe(&c));
                            let del = c.clone();
                            view! {
                                <span class="chip is-on" data-testid=tid>
                                    {label}
                                    <button
                                        type="button"
                                        class="chip-x"
                                        data-testid=del_tid
                                        title="删除这个自定义薪资档"
                                        on:click=move |_| {
                                            salaries.update(|v| v.retain(|x| *x != del));
                                            save.run(());
                                        }
                                    >
                                        "×"
                                    </button>
                                </span>
                            }
                        }).collect_view()}
                    </div>
                    <div class="filter-salary-custom">
                        <input
                            class="input input-num"
                            type="number"
                            min="0"
                            placeholder="下限K"
                            data-testid="scrape-filter-salary-low"
                            prop:value=move || salary_low.get()
                            on:input=move |ev| salary_low.set(event_target_value(&ev))
                        />
                        <span class="dim">"-"</span>
                        <input
                            class="input input-num"
                            type="number"
                            min="0"
                            placeholder="上限K"
                            data-testid="scrape-filter-salary-high"
                            prop:value=move || salary_high.get()
                            on:input=move |ev| salary_high.set(event_target_value(&ev))
                        />
                        <button class="chip as-btn" data-testid="scrape-filter-salary-add" on:click=add_salary>
                            "添加"
                        </button>
                    </div>
                </div>

                <ChipRow
                    title="工作经验"
                    hint="多选，命中任一档即算命中"
                    options=EXPERIENCE_OPTIONS.to_vec()
                    sel=experiences
                    tid_prefix="scrape-filter-exp"
                    clear_tid=Some("scrape-filter-exp-clear")
                    on_change=save
                />
                <ChipRow
                    title="学历"
                    hint="多选，命中任一档即算命中"
                    options=DEGREE_OPTIONS.to_vec()
                    sel=degrees
                    tid_prefix="scrape-filter-degree"
                    clear_tid=Some("scrape-filter-degree-clear")
                    on_change=save
                />
                <ChipRow
                    title="公司规模"
                    hint="多选，命中任一档即算命中"
                    options=SCALE_OPTIONS.to_vec()
                    sel=scales
                    tid_prefix="scrape-filter-scale"
                    clear_tid=Some("scrape-filter-scale-clear")
                    on_change=save
                />
                <ChipRow
                    title="融资阶段"
                    hint="多选，命中任一档即算命中"
                    options=STAGE_OPTIONS.to_vec()
                    sel=stages
                    tid_prefix="scrape-filter-stage"
                    clear_tid=None
                    on_change=save
                />
            </div>

            // ---------- 行业（18 + 不限 + 自定义编码）----------
            <div class="filter-row">
                <div class="filter-row-head">
                    <span class="filter-row-title" title="多选，命中任一档即算命中">"行业"</span>
                    <button
                        type="button"
                        class=move || if industries.get().is_empty() { "chip as-btn is-on" } else { "chip as-btn" }
                        data-testid="scrape-filter-industry-any"
                        on:click=move |_| {
                            industries.set(Vec::new());
                            save.run(());
                        }
                    >
                        "行业不限"
                    </button>
                </div>
                <div class="filter-chips">
                    {INDUSTRY_OPTIONS.iter().map(|(code, name)| {
                        let code = *code;
                        let table = INDUSTRY_OPTIONS;
                        view! {
                            <button
                                type="button"
                                class=move || if industries.get().iter().any(|x| x == code) {
                                    "chip as-btn is-on"
                                } else {
                                    "chip as-btn"
                                }
                                data-testid=format!("scrape-filter-industry-{}", tid_safe(code))
                                title=*name
                                on:click=move |_| {
                                    let cur = industries.get_untracked();
                                    industries.set(toggle_code(cur, code, &table));
                                    save.run(());
                                }
                            >
                                {*name}
                            </button>
                        }
                    }).collect_view()}
                </div>
                <div class="filter-kw-input">
                    <input
                        class="input"
                        data-testid="scrape-filter-industry-custom"
                        placeholder="自定义编码回车添加"
                        prop:value=move || industry_custom.get()
                        on:input=move |ev| industry_custom.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() != "Enter" {
                                return;
                            }
                            ev.prevent_default();
                            let code = industry_custom.get_untracked().trim().to_string();
                            if code.is_empty() {
                                return;
                            }
                            industries.update(|v| {
                                if !v.iter().any(|x| *x == code) {
                                    v.push(code.clone());
                                }
                            });
                            industry_custom.set(String::new());
                            save.run(());
                        }
                    />
                </div>
            </div>

            // ---------- 排除关键词 ----------
            <label class="field filter-exclude">
                <div class="label">
                    "排除关键词"
                    <span class="field-hint">"空格/逗号/顿号分隔，任意一项留空即表示不按该项排除，改动后自动保存"</span>
                </div>
                <textarea
                    class="input textarea"
                    rows="4"
                    data-testid="scrape-filter-exclude-words"
                    placeholder=EXCLUDE_PLACEHOLDER
                    prop:value=move || exclude.get()
                    on:input=move |ev| {
                        exclude.set(event_target_value(&ev));
                        save.run(());
                    }
                ></textarea>
            </label>

            // ---------- 底部四组 ----------
            <div class="filter-grid4">
                <div class="filter-row">
                    <div class="filter-row-head">
                        <span class="filter-row-title">"HR 活跃度 → 排除"</span>
                    </div>
                    <div class="filter-chips">
                        {HR_ACTIVE_OPTIONS.iter().map(|o| {
                            let v = o.value;
                            view! {
                                <button
                                    type="button"
                                    class=move || if hr_active.get() == v { "chip as-btn is-on" } else { "chip as-btn" }
                                    data-testid=format!("scrape-filter-hr-{v}")
                                    title=o.hint
                                    on:click=move |_| {
                                        hr_active.set(v.to_string());
                                        save.run(());
                                    }
                                >
                                    {o.name}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                <div class="filter-row">
                    <div class="filter-row-head">
                        <span class="filter-row-title">"工作制"</span>
                    </div>
                    <div class="filter-chips">
                        {WEEKEND_OPTIONS.iter().map(|o| {
                            let v = o.value;
                            view! {
                                <button
                                    type="button"
                                    class=move || if weekend.get() == v { "chip as-btn is-on" } else { "chip as-btn" }
                                    data-testid=format!("scrape-filter-weekend-{v}")
                                    title=o.hint
                                    on:click=move |_| {
                                        weekend.set(v.to_string());
                                        save.run(());
                                    }
                                >
                                    {o.name}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                <div class="filter-row">
                    <div class="filter-row-head">
                        <span class="filter-row-title">"JD 岗位要求 → 排除"</span>
                    </div>
                    <div class="filter-chips">
                        <button
                            type="button"
                            class=move || if veto_degrees.get().is_empty() { "chip as-btn is-on" } else { "chip as-btn" }
                            data-testid="scrape-filter-veto-degree-any"
                            on:click=move |_| {
                                veto_degrees.set(Vec::new());
                                save.run(());
                            }
                        >
                            "学历不限"
                        </button>
                        {DEGREE_VETO_OPTIONS.iter().map(|name| {
                            let name = *name;
                            view! {
                                <button
                                    type="button"
                                    class=move || if veto_degrees.get().iter().any(|x| x == name) {
                                        "chip as-btn is-on"
                                    } else {
                                        "chip as-btn"
                                    }
                                    data-testid=format!("scrape-filter-veto-degree-{name}")
                                    title=format!("JD 里要求 {name} 的岗位直接排除")
                                    on:click=move |_| {
                                        let was = veto_degrees.get_untracked();
                                        let mut n: Vec<String> = was.clone();
                                        if let Some(p) = n.iter().position(|x| x == name) {
                                            n.remove(p);
                                        } else {
                                            n.push(name.to_string());
                                        }
                                        // 按常量表顺序
                                        n.sort_by_key(|c| {
                                            DEGREE_VETO_OPTIONS
                                                .iter()
                                                .position(|k| k == c)
                                                .unwrap_or(usize::MAX)
                                        });
                                        veto_degrees.set(n);
                                        save.run(());
                                    }
                                >
                                    {name}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                <label class="field">
                    <div class="label">
                        "成立年限少于（年）"
                        <span class="field-hint">"留空 = 不排除"</span>
                    </div>
                    <input
                        class="input input-num"
                        type="number"
                        min="0"
                        placeholder="留空 = 不排除"
                        data-testid="scrape-filter-founded"
                        prop:value=move || founded.get()
                        on:input=move |ev| {
                            founded.set(event_target_value(&ev));
                            save.run(());
                        }
                    />
                </label>
            </div>

            // 14 项筛选条件已全部经 `set_scrape_defaults` 落库，脚注里
            // 原先那句「暂未下发后端，仅本地留档」已不成立，整条删除。
            // 这里只保留一句事实说明：改完即存，下次抓取生效——
            // 用户最容易误解的正是「改了但这次已经开跑的抓取没变」。
            <p class="filter-footnote" data-testid="scrape-filter-footnote">
                "以上条件随改随存（约 0.4 秒后写入本地配置），下次开始抓取时生效；已开始的抓取不受影响"
            </p>
        </div>
    }
}
