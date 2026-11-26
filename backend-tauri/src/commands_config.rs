//! 配置页命令层：读全量配置 / 只提交 diff / 改上下限。
//!
//! **职责边界**（AGENTS.md 2.2）：本模块只做「收参数 → 调 core → 错误转字符串」。
//! 哪些键可以写、默认值是多少、密钥怎么掩码、上下限怎么校验，全部在
//! [`crate::core::domain::app_config`]；这里一行业务规则都不写。
//!
//! **安全红线**（AGENTS.md 2.5）：`api_key` **严禁**明文回传。前端只拿到
//! `api_key_hint`（`****abcd`）与 `has_key`，足以渲染「已配置」状态，
//! 不足以泄漏密钥——即便 WebView 被调试工具接管也一样。

use crate::commands_db::{
    build_llm_client_with_temperature, store_err, store_handle, with_store, CmdResult, LlmState,
    StoreState,
};
use crate::core::domain::app_config::{
    collect_scrape_defaults, mask_api_key, resolve_limit, scrape_default_key, scrape_value_to_text,
    LimitArgs, LimitValues, ScrapeDefaultsArgs, DEFAULT_BASE_URL, DEFAULT_LLM_PREFILTER,
    DEFAULT_MAX_JD, DEFAULT_MODEL, DEFAULT_SCORE_MODE, DEFAULT_THEME, KEY_API_KEY, KEY_BASE_URL,
    KEY_DATA_DIR, KEY_EXCLUDE_KEYWORDS, KEY_EXCLUDE_RULES, KEY_GREET_PROMPT, KEY_LAST_FILTER,
    KEY_LLM_PREFILTER, KEY_MAX_JD, KEY_MIN_SCORE, KEY_MODEL, KEY_RESUME_PATH, KEY_SCORE_MODE,
    KEY_SKILLS, KEY_TEMPERATURE, KEY_GREET_TEMPERATURE, KEY_THEME, KEY_TOKENS_GREET,
    KEY_TOKENS_SCORE, KEY_TOKENS_TOTAL, WRITABLE_KEYS,
};
use crate::core::domain::greeting::DEFAULT_PROMPT;
use crate::core::scoring::SCORE_MAX;
use crate::core::domain::store_ports::ConfigStore;
use crate::core::infra::StoreError;
use serde::Serialize;

type Cfg = dyn ConfigStore<Error = StoreError>;

/// `get_app_config` 的回执。
///
/// 字段与旧版 `web/src/api.ts` 的 `Config` 同名项一一对应，前端可直接映射；
/// `api_key_hint` / `has_key` / `max_jd` / `min_score` / `score_max` 是配置页要用的
/// 派生值——全部由 Rust 算好，前端不必再猜（尤其不得自己猜满分）。
#[derive(Debug, Serialize)]
pub struct AppConfigReply {
    pub base_url: String,
    pub model: String,
    /// 打分温度（0~2）。前端直接显示这个值，不让它自己猜默认值。
    pub temperature: f32,
    /// 话术温度（0~2）
    pub greet_temperature: f32,
    /// `****abcd` 形式的掩码；未配置时为空串
    pub api_key_hint: String,
    pub has_key: bool,
    pub theme: String,
    pub score_mode: String,
    pub llm_prefilter: u8,
    pub max_jd: u32,
    pub min_score: u8,
    pub score_max: u8,
    pub greet_prompt: String,
    /// 问候语提示词的出厂值，供界面「恢复默认」按钮使用
    pub greet_prompt_default: String,
    pub skills: String,
    pub exclude_keywords: String,
    pub exclude_rules: String,
    pub resume_path: String,
    pub data_dir: String,
    pub last_filter: String,
    pub tokens_total: i64,
    pub tokens_greet: i64,
    pub tokens_score: i64,
    /// 抓取筛选面板上次用过的条件（`scrape_defaults.*` 聚合）。
    ///
    /// 一个都没配过时是 `{}`（不是 `null`）：前端可直接当对象展开，
    /// 不必到处判空。**这是新增字段，旧的 23 个字段一个都没动。**
    pub scrape_defaults: serde_json::Value,
}

/// 一次 `config_pairs()` 读全部键，再在内存里取值。
///
/// 不用 18 次 `get_config`：配置页每次打开都会调它，18 条查询会各自抢一次
/// 连接锁，正好和后台抓取入库撞上。
pub fn read_config(store: &Cfg, data_dir: &str) -> Result<AppConfigReply, String> {
    let pairs = store.config_pairs().map_err(store_err)?;
    let get = |key: &str, dflt: &str| -> String {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| dflt.to_string())
    };
    let num = |key: &str| -> i64 {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.trim().parse::<i64>().ok())
            .unwrap_or(0)
    };
    let raw_key = get(KEY_API_KEY, "");
    let stored_dir = get(KEY_DATA_DIR, "");
    Ok(AppConfigReply {
        base_url: get(KEY_BASE_URL, DEFAULT_BASE_URL),
        model: get(KEY_MODEL, DEFAULT_MODEL),
        temperature: crate::core::infra::llm::resolve_temperature(
            pairs
                .iter()
                .find(|(k, _)| k == KEY_TEMPERATURE)
                .map(|(_, v)| v.as_str()),
            crate::core::infra::SCORE_TEMPERATURE,
        ),
        greet_temperature: crate::core::infra::llm::resolve_temperature(
            pairs
                .iter()
                .find(|(k, _)| k == KEY_GREET_TEMPERATURE)
                .map(|(_, v)| v.as_str()),
            crate::core::infra::GREET_TEMPERATURE,
        ),
        api_key_hint: mask_api_key(&raw_key),
        has_key: !raw_key.trim().is_empty(),
        theme: get(KEY_THEME, DEFAULT_THEME),
        score_mode: get(KEY_SCORE_MODE, DEFAULT_SCORE_MODE),
        llm_prefilter: pairs
            .iter()
            .find(|(k, _)| k == KEY_LLM_PREFILTER)
            .and_then(|(_, v)| v.trim().parse::<u8>().ok())
            .unwrap_or(DEFAULT_LLM_PREFILTER),
        max_jd: pairs
            .iter()
            .find(|(k, _)| k == KEY_MAX_JD)
            .and_then(|(_, v)| v.trim().parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_JD),
        min_score: pairs
            .iter()
            .find(|(k, _)| k == KEY_MIN_SCORE)
            .and_then(|(_, v)| v.trim().parse::<u8>().ok())
            .unwrap_or(crate::core::scoring::SCORE_MIN_DEFAULT),
        score_max: SCORE_MAX,
        greet_prompt: get(KEY_GREET_PROMPT, DEFAULT_PROMPT),
        greet_prompt_default: DEFAULT_PROMPT.to_string(),
        skills: get(KEY_SKILLS, ""),
        exclude_keywords: get(KEY_EXCLUDE_KEYWORDS, ""),
        exclude_rules: get(KEY_EXCLUDE_RULES, ""),
        resume_path: get(KEY_RESUME_PATH, ""),
        // 用户显式配过 data_dir 就尊重它，否则用应用真实的数据目录
        data_dir: if stored_dir.trim().is_empty() {
            data_dir.to_string()
        } else {
            stored_dir
        },
        last_filter: get(KEY_LAST_FILTER, ""),
        tokens_total: num(KEY_TOKENS_TOTAL),
        tokens_greet: num(KEY_TOKENS_GREET),
        tokens_score: num(KEY_TOKENS_SCORE),
        // 读侧由 config_pairs() 一次性带出，这里只做聚合，不再单独查库
        scrape_defaults: collect_scrape_defaults(&pairs),
    })
}

/// 应用路径状态：`setup` 阶段把 `app_local_data_dir()` 塞进来，命令层只读。
///
/// 不让 UI 自己拼 `%LOCALAPPDATA%\ai-job-assistant`：打包态与开发态的
/// 标识符不同，拼错了用户看到的是一份「空库的真实路径」。
pub struct AppPaths(pub std::sync::Mutex<Option<std::path::PathBuf>>);

impl AppPaths {
    pub fn empty() -> Self {
        Self(std::sync::Mutex::new(None))
    }
    pub fn set(&self, dir: std::path::PathBuf) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(dir);
        }
    }
    pub fn get(&self) -> Option<std::path::PathBuf> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    /// 「数据目录」的展示文本；未初始化时为空串（界面显示「未知」）。
    pub fn data_dir_text(&self) -> String {
        self.get()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

/// 读配置。
#[tauri::command]
pub fn get_app_config(
    state: tauri::State<'_, StoreState>,
    paths: tauri::State<'_, AppPaths>,
) -> CmdResult<AppConfigReply> {
    let dir = paths.data_dir_text();
    with_store(&state, |s| read_config(s, &dir))
}

/// 保存结果的回执：`{ok, llm_enabled, updated}`（前两个沿用旧版 `setConfig`）。
#[derive(Debug, Serialize)]
pub struct SaveConfigReply {
    pub ok: bool,
    /// 保存后大模型客户端是否可用（界面据此决定「启用大模型」开关是否置灰）
    pub llm_enabled: bool,
    /// 实际写入的键数，便于提示「已保存（3 项）」
    pub updated: usize,
}

/// 前端只提交 diff：**没出现的键一律不动**。
///
/// 这不是优化而是硬要求：配置页若把整份配置（含被掩码成 `****abcd` 的
/// `api_key`）原样回传，掩码会被当成真密钥写进库，用户下次打分时大模型
/// 直接 401——而且发生在「保存了一次设置」之后，极难定位。因此：
/// - 只写 [`WRITABLE_KEYS`] 里的键，`tokens_*` / `min_score` / `data_dir` 一律忽略；
/// - 字符串以 `****` 开头视为「未修改的掩码」，跳过；
/// - `null` 表示删除该键。
pub fn apply_patch(store: &Cfg, patch: &serde_json::Value) -> Result<usize, String> {
    let obj = patch
        .as_object()
        .ok_or_else(|| "配置必须是一个对象".to_string())?;
    let mut written = 0usize;
    for (key, value) in obj {
        if !WRITABLE_KEYS.contains(&key.as_str()) {
            continue;
        }
        let text = match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => {
                if s.starts_with("****") {
                    continue;
                }
                Some(s.clone())
            }
            other => Some(other.to_string()),
        };
        store
            .set_config(key, text.as_deref())
            .map_err(store_err)?;
        written += 1;
    }
    Ok(written)
}

/// 保存配置 diff。
///
/// 保存后**立即重建大模型客户端**：密钥或模型可能变了，不重建的话用户
/// 改完密钥必须重启应用才生效，会以为是「保存按钮没反应」。
#[tauri::command]
pub fn set_app_config(
    state: tauri::State<'_, StoreState>,
    llm: tauri::State<'_, LlmState>,
    patch: serde_json::Value,
) -> CmdResult<SaveConfigReply> {
    let updated = with_store(&state, |s| apply_patch(s, &patch))?;
    let store = store_handle(&state)?;
    let pairs = store.config_pairs().map_err(store_err)?;
    // 温度必须跟着一起重建，否则「保存了温度但要重启才生效」。
    let client = build_llm_client_with_temperature(&pairs);
    let enabled = client.is_some();
    if let Ok(mut guard) = llm.0.lock() {
        *guard = client;
    }
    Ok(SaveConfigReply {
        ok: true,
        llm_enabled: enabled,
        updated,
    })
}

/// `set_scrape_defaults` 的回执。
#[derive(Debug, Serialize)]
pub struct ScrapeDefaultsReply {
    pub ok: bool,
    /// 实际写入的键数（`None` 字段不计）
    pub saved: usize,
}

/// 抓取默认条件的「只提交 diff」写入。
///
/// 语义与 [`apply_patch`] 对齐：**`None` 的字段一律跳过**，不写库也不覆盖已有值。
/// 键名先过 [`scrape_default_key`] 的白名单再做拼接，白名单外的相对名直接报错
/// ——静默跳过会让「保存成功但下次打开就没了」变成只有用户能发现的问题。
pub fn write_scrape_defaults(store: &Cfg, args: &ScrapeDefaultsArgs) -> Result<usize, String> {
    let mut saved = 0usize;
    for (rel, value) in args.diffs() {
        let key = scrape_default_key(rel)?;
        store
            .set_config(&key, Some(&scrape_value_to_text(value)))
            .map_err(store_err)?;
        saved += 1;
    }
    Ok(saved)
}

/// 保存抓取筛选面板的默认条件（城市 / 薪资 / … / 成立年限，共 14 项）。
///
/// 与 `set_app_config` 分开的理由：这一组值是「下次抓取用哪套条件」，
/// 键名前缀固定为 `scrape_defaults.`，**不做密钥掩码、不重建大模型客户端**，
/// 走独立命令既不污染配置页语义，也不让抓取面板的保存触发一次 LLM 客户端重建。
#[tauri::command]
pub fn set_scrape_defaults(
    state: tauri::State<'_, StoreState>,
    args: ScrapeDefaultsArgs,
) -> CmdResult<ScrapeDefaultsReply> {
    let saved = with_store(&state, |s| write_scrape_defaults(s, &args))?;
    Ok(ScrapeDefaultsReply { ok: true, saved })
}

/// `set_limit` 的回执：回显**生效后**的上下限，界面据此纠正输入框。
#[derive(Debug, Serialize)]
pub struct LimitReply {
    pub ok: bool,
    pub max_jd: u32,
    pub min_score: u8,
}

fn read_limits(store: &Cfg) -> LimitValues {
    let get = |key: &str| store.get_config(key, "").unwrap_or_default();
    LimitValues {
        max_jd: get(KEY_MAX_JD).trim().parse::<u32>().unwrap_or(DEFAULT_MAX_JD),
        min_score: get(KEY_MIN_SCORE)
            .trim()
            .parse::<u8>()
            .unwrap_or(crate::core::scoring::SCORE_MIN_DEFAULT),
    }
}

/// 改「有效 JD 上限」与「分值门槛」。
///
/// 只传一个也行：旧版状态栏两个输入框各自 onBlur 触发一次。
/// 校验（0..=500、0..=满分）在 [`resolve_limit`] 里，这里只转发。
#[tauri::command]
pub fn set_limit(state: tauri::State<'_, StoreState>, args: LimitArgs) -> CmdResult<LimitReply> {
    with_store(&state, |s| {
        let next = resolve_limit(&args, read_limits(s), SCORE_MAX)?;
        s.set_config(KEY_MAX_JD, Some(&next.max_jd.to_string()))
            .map_err(store_err)?;
        s.set_config(KEY_MIN_SCORE, Some(&next.min_score.to_string()))
            .map_err(store_err)?;
        Ok(LimitReply {
            ok: true,
            max_jd: next.max_jd,
            min_score: next.min_score,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::infra::SqliteStore;

    fn store() -> SqliteStore {
        SqliteStore::open_in_memory().expect("内存库应可打开")
    }

    fn args_from(v: serde_json::Value) -> ScrapeDefaultsArgs {
        serde_json::from_value(v).expect("入参应可反序列化")
    }

    #[test]
    fn read_config_never_leaks_the_raw_api_key() {
        let s = store();
        s.set_config(KEY_API_KEY, Some("sk-super-secret-9876")).unwrap();
        let reply = read_config(&s, "/tmp/data").expect("读配置应成功");
        assert!(reply.has_key);
        assert_eq!(reply.api_key_hint, "****9876");
        // 关键：把整个回执序列化后，原文不得出现在任何字段里
        let json = serde_json::to_string(&reply).unwrap();
        assert!(!json.contains("super-secret"), "掩码之外的原文绝不能出现在回执里");
        assert!(json.contains("****9876"));
    }

    #[test]
    fn read_config_defaults_when_empty() {
        let reply = read_config(&store(), "/tmp/data").expect("读配置应成功");
        assert!(!reply.has_key);
        assert_eq!(reply.api_key_hint, "");
        assert_eq!(reply.theme, DEFAULT_THEME);
        assert_eq!(reply.score_mode, DEFAULT_SCORE_MODE);
        assert_eq!(reply.max_jd, DEFAULT_MAX_JD);
        assert_eq!(reply.min_score, crate::core::scoring::SCORE_MIN_DEFAULT);
        assert_eq!(reply.score_max, SCORE_MAX);
        assert_eq!(reply.base_url, DEFAULT_BASE_URL);
        // 没配 data_dir 时用应用真实目录
        assert_eq!(reply.data_dir, "/tmp/data");
    }

    #[test]
    fn apply_patch_only_writes_diff_and_skips_masked_key() {
        let s = store();
        s.set_config(KEY_API_KEY, Some("sk-real-1234")).unwrap();
        // 配置页把整份配置原样回传的经典事故
        let patch = serde_json::json!({
            "api_key": "****1234",
            "theme": "dark",
            "tokens_total": 999,
            "min_score": 10,
            "unknown_key": "x"
        });
        let written = apply_patch(&s, &patch).expect("保存应成功");
        assert_eq!(written, 1, "只有 theme 该被写入");
        assert_eq!(s.get_config(KEY_API_KEY, "").unwrap(), "sk-real-1234", "掩码不得覆盖真密钥");
        assert_eq!(s.get_config(KEY_THEME, "").unwrap(), "dark");
        assert_eq!(s.get_config(KEY_TOKENS_TOTAL, "").unwrap(), "", "只读键不得被写");
        assert_eq!(s.get_config(KEY_MIN_SCORE, "").unwrap(), "", "门槛只能走 set_limit");
    }

    #[test]
    fn apply_patch_null_deletes_and_non_object_is_error() {
        let s = store();
        s.set_config(KEY_MODEL, Some("m1")).unwrap();
        apply_patch(&s, &serde_json::json!({"model": null})).expect("删除应成功");
        assert_eq!(s.get_config(KEY_MODEL, "gone").unwrap(), "gone");
        let err = apply_patch(&s, &serde_json::json!("不是对象")).unwrap_err();
        assert!(err.contains("对象"), "错误要中文可读：{err}");
    }

    #[test]
    fn apply_patch_accepts_real_key_and_numbers() {
        let s = store();
        let written = apply_patch(
            &s,
            &serde_json::json!({"api_key": "sk-new-0001", "llm_prefilter": 60}),
        )
        .expect("保存应成功");
        assert_eq!(written, 2);
        assert_eq!(s.get_config(KEY_API_KEY, "").unwrap(), "sk-new-0001");
        // 数字要按字符串落库（Python 侧历史行为一致），否则读回时类型漂移
        assert_eq!(s.get_config(KEY_LLM_PREFILTER, "").unwrap(), "60");
    }

    #[test]
    fn set_limit_writes_min_score_which_patch_cannot() {
        let s = store();
        let next = resolve_limit(
            &LimitArgs {
                max_jd: Some(30),
                min_score: Some(80),
            },
            read_limits(&s),
            SCORE_MAX,
        )
        .expect("校验应通过");
        s.set_config(KEY_MAX_JD, Some(&next.max_jd.to_string())).unwrap();
        s.set_config(KEY_MIN_SCORE, Some(&next.min_score.to_string()))
            .unwrap();
        let reply = read_config(&s, "").expect("读配置应成功");
        assert_eq!(reply.max_jd, 30);
        assert_eq!(reply.min_score, 80);
    }

    #[test]
    fn defaults_args_accept_camel_and_snake() {
        // 前端发驼峰
        let camel = args_from(serde_json::json!({
            "city": ["北京"],
            "maxPages": 3,
            "jobType": "全职",
            "companyScale": ["1000-9999人"],
            "financeStage": ["C轮"],
            "hrActive": "今日活跃",
            "workWeekend": "双休",
            "minYears": 3,
            "excludeKeywords": "外包,培训"
        }));
        assert_eq!(camel.max_pages, Some(serde_json::json!(3)));
        assert_eq!(camel.job_type, Some(serde_json::json!("全职")));
        assert_eq!(camel.company_scale, Some(serde_json::json!(["1000-9999人"])));
        assert_eq!(camel.finance_stage, Some(serde_json::json!(["C轮"])));
        assert_eq!(camel.hr_active, Some(serde_json::json!("今日活跃")));
        assert_eq!(camel.work_weekend, Some(serde_json::json!("双休")));
        assert_eq!(camel.min_years, Some(serde_json::json!(3)));
        assert_eq!(camel.exclude_keywords, Some(serde_json::json!("外包,培训")));

        // 旧调用方 / 库内序列化发蛇形
        let snake = args_from(serde_json::json!({
            "max_pages": 5,
            "job_type": "实习",
            "company_scale": ["20-99人"],
            "finance_stage": ["A轮"],
            "hr_active": "3日内活跃",
            "work_weekend": "大小周",
            "min_years": 1
        }));
        assert_eq!(snake.max_pages, Some(serde_json::json!(5)));
        assert_eq!(snake.job_type, Some(serde_json::json!("实习")));
        assert_eq!(snake.hr_active, Some(serde_json::json!("3日内活跃")));
        assert_eq!(snake.min_years, Some(serde_json::json!(1)));

        // 全部可省略
        let empty = args_from(serde_json::json!({}));
        assert!(empty.diffs().is_empty());
    }

    #[test]
    fn defaults_skip_none_and_write_only_diff() {
        let s = store();
        // 先落两份已有值
        let first = write_scrape_defaults(
            &s,
            &args_from(serde_json::json!({"city": ["北京"], "maxPages": 3})),
        )
        .expect("首次保存应成功");
        assert_eq!(first, 2);

        // 只提交一个字段：其余字段是 None，必须原封不动
        let second = write_scrape_defaults(
            &s,
            &args_from(serde_json::json!({"maxPages": 5})),
        )
        .expect("增量保存应成功");
        assert_eq!(second, 1, "只有 max_pages 该被写入");
        assert_eq!(
            s.get_config(crate::core::domain::app_config::KEY_SCRAPE_CITY, "").unwrap(),
            r#"["北京"]"#,
            "None 字段不得覆盖已有值"
        );
        assert_eq!(s.get_config(crate::core::domain::app_config::KEY_SCRAPE_MAX_PAGES, "").unwrap(), "5");

        // 显式 null 同样视为「不动」
        let third = write_scrape_defaults(
            &s,
            &args_from(serde_json::json!({"city": null, "minYears": null})),
        )
        .expect("空 diff 应成功");
        assert_eq!(third, 0);
        assert_eq!(s.get_config(crate::core::domain::app_config::KEY_SCRAPE_CITY, "").unwrap(), r#"["北京"]"#);

        // 没碰过的键不得凭空出现
        assert_eq!(s.get_config(crate::core::domain::app_config::KEY_SCRAPE_INDUSTRY, "gone").unwrap(), "gone");
    }

    #[test]
    fn defaults_reject_unknown_key() {
        // 键名白名单：即使有人绕过结构体直接拼键名，也必须被拒
        assert!(scrape_default_key("platform_login").is_err());
        assert!(scrape_default_key("api_key").is_err());
        assert!(scrape_default_key("unknown").is_err());
        // 已知键正常
        assert_eq!(scrape_default_key("city").unwrap(), crate::core::domain::app_config::KEY_SCRAPE_CITY);

        // set_app_config 的路径同样拦得住：白名单外的 scrape_defaults 键不写
        let s = store();
        let written = apply_patch(
            &s,
            &serde_json::json!({
                "scrape_defaults.unknown": "x",
                "scrape_defaults.city": ["上海"]
            }),
        )
        .expect("保存应成功");
        assert_eq!(written, 1, "未知键必须被白名单拦掉");
        assert_eq!(
            s.get_config("scrape_defaults.unknown", "gone").unwrap(),
            "gone"
        );
        assert_eq!(s.get_config(crate::core::domain::app_config::KEY_SCRAPE_CITY, "").unwrap(), r#"["上海"]"#);
    }

    #[test]
    fn defaults_roundtrip_through_config_store() {
        let s = store();
        // 空库时读侧必须是 {}，不是 null
        let empty = read_config(&s, "").expect("读配置应成功");
        assert_eq!(empty.scrape_defaults, serde_json::json!({}));

        write_scrape_defaults(
            &s,
            &args_from(serde_json::json!({
                "city": ["深圳"],
                "salary": ["20-30K"],
                "experience": ["3-5年"],
                "degree": ["本科"],
                "companyScale": ["1000-9999人"],
                "financeStage": ["已上市"],
                "industry": ["互联网"],
                "jobType": "全职",
                "maxPages": 4,
                "excludeKeywords": "外包",
                "hrActive": "今日活跃",
                "workWeekend": "双休",
                "minYears": 2
            })),
        )
        .expect("保存应成功");

        let reply = read_config(&s, "").expect("读配置应成功");
        assert_eq!(
            reply.scrape_defaults,
            serde_json::json!({
                "city": ["深圳"],
                "salary": ["20-30K"],
                "experience": ["3-5年"],
                "degree": ["本科"],
                "company_scale": ["1000-9999人"],
                "finance_stage": ["已上市"],
                "industry": ["互联网"],
                "job_type": "全职",
                "max_pages": 4,
                "exclude_keywords": "外包",
                "hr_active": "今日活跃",
                "work_weekend": "双休",
                "min_years": 2
            })
        );
        // 旧字段必须原样还在（返回结构是扩展不是替换）
        let json = serde_json::to_value(&reply).unwrap();
        for key in ["base_url", "model", "api_key_hint", "has_key", "theme", "score_mode",
                    "llm_prefilter", "max_jd", "min_score", "score_max", "greet_prompt",
                    "greet_prompt_default", "skills", "exclude_keywords", "exclude_rules",
                    "resume_path", "data_dir", "last_filter", "tokens_total", "tokens_greet",
                    "tokens_score", "scrape_defaults"] {
            assert!(json.get(key).is_some(), "回执缺字段 {key}");
        }
    }
}
