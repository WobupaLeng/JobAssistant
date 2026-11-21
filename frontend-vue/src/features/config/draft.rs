//! 配置页的「草稿 → 保存计划」纯逻辑（无 DOM、无 IPC，可原生单测）。
//!
//! 拆出来的理由：配置页跨**两条**命令（`set_app_config` 与 `set_limit`），
//! 哪几个键走哪条、谁算「改过」、本地怎么校验，全是可证伪的规则；
//! 埋在 `view!` 里只能靠肉眼看，而这种错误的表现又是「保存成功但没生效」
//! （后端的 `apply_patch` 对白名单外的键**静默跳过**，AGENTS.md 3.2）。
//!
//! 三条铁律在这一层固化：
//! 1. **门槛（`min_score`）永远只走 `set_limit`**：它不在 `WRITABLE_KEYS` 里，
//!    混进 `set_app_config` 的 patch 会被静默丢弃，用户以为存了其实没存。
//! 2. **只提交 diff**：没改过的键不进 patch。后端对 `null` 的语义是「删除该键」，
//!    把整份配置原样回传等于每保存一次就把清空的项全删一遍。
//! 3. **密钥永不回传**：回执里只有 `****尾4位`（`api_key_hint`），前端不得把它
//!    当明文存进草稿，patch 里也绝不能出现 `api_key` 的 mask 版本。

use crate::api::types::AppConfigReply;

/// 配置页可编辑字段的一份快照。
///
/// `api_key` 恒为**用户本次输入**的原文（初始为空串，见 [`ConfigDraft::from_reply`]）；
/// 密钥的「已配置 / 尾 4 位」是单独两个只读字段，不参与任何 patch。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigDraft {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub llm_prefilter: String,
    pub keywords: String,
    pub max_jd: String,
    pub min_score: String,
}

/// 从 `get_app_config` 的回执生成初始草稿。
///
/// 数字一律转字符串：输入框里用户正在敲的值可能暂时不是合法数字
/// （如空串、`"6"` 敲到一半），用 `u32` 存会让按键时的每一位都触发一次解析失败。
/// 校验放到 [`ConfigDraft::validate`]。
pub fn draft_from_reply(reply: &AppConfigReply) -> ConfigDraft {
    ConfigDraft {
        // 刻意不取 reply.api_key / api_key_hint：前者恒空，后者是掩码，都不是明文
        api_key: String::new(),
        base_url: reply.base_url.trim().to_string(),
        model: reply.model.trim().to_string(),
        llm_prefilter: reply.llm_prefilter.to_string(),
        keywords: reply.exclude_keywords.trim().to_string(),
        max_jd: reply.max_jd.to_string(),
        min_score: reply.min_score.to_string(),
    }
}

/// 保存计划：两条命令各要提交什么。
///
/// `patch` 为 `None` 表示「这条命令不用发」——不是「发个空对象」。
/// 空的 `set_app_config` 也会重建一次大模型客户端，白跑一趟没有意义。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavePlan {
    /// 走 `set_app_config`：只含 `WRITABLE_KEYS` 里改过的键（蛇形键名）
    pub patch: Option<serde_json::Map<String, serde_json::Value>>,
    /// 走 `set_limit`：`(max_jd, min_score)`，逐项 `None` = 不改
    pub max_jd: Option<u32>,
    pub min_score: Option<u8>,
}

impl SavePlan {
    /// 两条命令都不用发：用户没改任何东西就点了保存。
    pub fn is_empty(&self) -> bool {
        self.patch.is_none() && self.max_jd.is_none() && self.min_score.is_none()
    }
}

/// 汇总保存计划要提示的写入项数（供「已保存（N 项）」用）。
#[cfg(test)]
pub fn plan_write_count(plan: &SavePlan) -> usize {
    plan.patch.as_ref().map(|m| m.len()).unwrap_or(0)
        + usize::from(plan.max_jd.is_some())
        + usize::from(plan.min_score.is_some())
}

/// 本地校验。返回第一条可读错误；全部通过返回 `Ok(())`。
///
/// `score_max` 来自 `get_scoring_spec`（**绝不硬编码 100**，AGENTS.md 2.3）。
/// `score_max` 为 `None`（口径还没拉回来）时只校验「是数字」，范围交给后端
/// ——`set_limit` 会回 400 级的 `Err(String)`，这里不替它猜。
pub fn validate(draft: &ConfigDraft, score_max: Option<u8>) -> Result<(), String> {
    if !draft.llm_prefilter.trim().is_empty() {
        let v: u32 = draft
            .llm_prefilter
            .trim()
            .parse()
            .map_err(|_| format!("大模型初筛线必须是整数（当前填的是「{}」）", draft.llm_prefilter))?;
        if v > 255 {
            return Err(format!("大模型初筛线超出取值范围 0–255（当前填的是 {v}）"));
        }
    }
    if draft.keywords.trim().is_empty() {
        return Err("关键词不能为空：抓取与打分都要用它".to_string());
    }
    let max_jd = parse_num(&draft.max_jd, "单次抓取上限")?;
    if let Some(v) = max_jd {
        if v > u32::MAX as u64 {
            return Err(format!("单次抓取上限过大（当前填的是 {v}）"));
        }
    }
    let min_score = parse_num(&draft.min_score, "入库门槛")?;
    if let (Some(v), Some(max)) = (min_score, score_max) {
        if v > max as u64 {
            return Err(format!("入库门槛不能超过满分 {max}（当前填的是 {v}）"));
        }
    }
    Ok(())
}

fn parse_num(raw: &str, label: &str) -> Result<Option<u64>, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{label}必须是非负整数（当前填的是「{t}」）"))
}

/// 由「当前草稿」与「后端回显的基线」算出保存计划。
///
/// 基线是**上次成功读/写后的值**（`stored`），不是「页面打开时的值」：
/// 保存成功后调用方会把 `stored` 换成新草稿，于是再点一次保存就是空计划。
pub fn build_plan(draft: &ConfigDraft, stored: &ConfigDraft, score_max: Option<u8>) -> SavePlan {
    let mut plan = SavePlan::default();

    let mut patch = serde_json::Map::new();
    // 密钥：只有用户真敲了（非空且不是掩码）才提交。
    // 以 `****` 开头的值后端会跳过，但这道兜底不是许可（见 api/mod.rs 注释）。
    let key = draft.api_key.trim();
    if !key.is_empty() && !key.starts_with("****") {
        patch.insert("api_key".into(), serde_json::json!(key));
    }
    let mut text = |key: &str, cur: &str, base: &str| {
        let cur = cur.trim();
        if cur != base.trim() {
            // 空串写 empty 而不是 null：`null` 在 apply_patch 里是「删除该键」，
            // 会把「清空 Base URL」变成「恢复出厂默认」——两者语义不同。
            patch.insert(key.to_string(), serde_json::json!(cur));
        }
    };
    text("base_url", &draft.base_url, &stored.base_url);
    text("model", &draft.model, &stored.model);
    text("exclude_keywords", &draft.keywords, &stored.keywords);
    // 数字按字符串落库（与 `set_app_config` 的 `other.to_string()` 一致），
    // 否则读回时类型漂移（见 commands_config.rs 的 `apply_patch_accepts_real_key_and_numbers`）
    text("llm_prefilter", &draft.llm_prefilter, &stored.llm_prefilter);
    if !patch.is_empty() {
        plan.patch = Some(patch);
    }

    // 门槛与上限：只走 set_limit，且逐项只发改过的那一个
    if let Some(v) = parse_num(&draft.max_jd, "单次抓取上限").ok().flatten() {
        if draft.max_jd.trim() != stored.max_jd.trim() {
            plan.max_jd = Some(v.min(u32::MAX as u64) as u32);
        }
    }
    if let Some(v) = parse_num(&draft.min_score, "入库门槛").ok().flatten() {
        let over = score_max.map(|m| v > m as u64).unwrap_or(false);
        if draft.min_score.trim() != stored.min_score.trim() && !over {
            plan.min_score = Some(v.min(u8::MAX as u64) as u8);
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> AppConfigReply {
        AppConfigReply {
            api_key_hint: "****9876".to_string(),
            has_key: true,
            base_url: "https://api.deepseek.com".to_string(),
            model: "deepseek-chat".to_string(),
            llm_prefilter: 50,
            max_jd: 15,
            min_score: 70,
            score_max: 100,
            exclude_keywords: "外包,培训".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn draft_never_carries_the_masked_key() {
        let d = draft_from_reply(&reply());
        assert_eq!(d.api_key, "", "掩码不得进入草稿，否则保存时会被原样回传");
        assert_eq!(d.keywords, "外包,培训");
        assert_eq!(d.max_jd, "15");
        assert_eq!(d.min_score, "70");
    }

    #[test]
    fn untouched_draft_produces_an_empty_plan() {
        let d = draft_from_reply(&reply());
        let plan = build_plan(&d, &d, Some(100));
        assert!(plan.is_empty(), "没改任何东西不该发任何命令：{plan:?}");
        assert_eq!(plan_write_count(&plan), 0);
    }

    #[test]
    fn min_score_never_enters_the_patch_it_only_goes_to_set_limit() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.min_score = "80".to_string();
        let plan = build_plan(&d, &base, Some(100));
        assert!(
            plan.patch.is_none(),
            "门槛混进 set_app_config 会被 apply_patch 静默丢弃"
        );
        assert_eq!(plan.min_score, Some(80));
        assert_eq!(plan.max_jd, None, "没改上限就不能把上限也发一遍");
    }

    #[test]
    fn writable_fields_go_into_the_patch_with_snake_keys() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.base_url = " https://example.com ".to_string();
        d.model = "qwen-max".to_string();
        d.keywords = "Java,Rust".to_string();
        d.llm_prefilter = "60".to_string();
        let plan = build_plan(&d, &base, Some(100));
        let patch = plan.patch.expect("有改动就该有 patch");
        assert_eq!(patch.get("base_url").unwrap(), "https://example.com");
        assert_eq!(patch.get("model").unwrap(), "qwen-max");
        assert_eq!(patch.get("exclude_keywords").unwrap(), "Java,Rust");
        // 数字按字符串落库，与后端 apply_patch 的 other.to_string() 一致
        assert_eq!(patch.get("llm_prefilter").unwrap(), "60");
        assert!(!patch.contains_key("api_key"), "没动密钥框就不能提交 api_key");
        assert!(!patch.contains_key("min_score"));
        assert!(!patch.contains_key("max_jd"));
        assert!(plan.max_jd.is_none() && plan.min_score.is_none());
    }

    #[test]
    fn typed_key_is_sent_but_a_mask_is_not() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.api_key = "sk-new-0001".to_string();
        let plan = build_plan(&d, &base, Some(100));
        assert_eq!(
            plan.patch.unwrap().get("api_key").unwrap(),
            "sk-new-0001"
        );

        // 即便有人把回执里的掩码塞进草稿，也不许提交
        let mut masked = base.clone();
        masked.api_key = "****9876".to_string();
        assert!(build_plan(&masked, &base, Some(100)).patch.is_none());
    }

    #[test]
    fn max_jd_also_routes_to_set_limit() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.max_jd = "30".to_string();
        let plan = build_plan(&d, &base, Some(100));
        assert_eq!(plan.max_jd, Some(30));
        assert!(plan.min_score.is_none());
        assert!(plan.patch.is_none());
    }

    #[test]
    fn validate_rejects_bad_numbers_and_over_max_threshold() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.llm_prefilter = "abc".to_string();
        assert!(validate(&d, Some(100)).unwrap_err().contains("整数"));

        let mut d = base.clone();
        d.max_jd = "-1".to_string();
        assert!(validate(&d, Some(100)).unwrap_err().contains("单次抓取上限"));

        let mut d = base.clone();
        d.min_score = "101".to_string();
        assert!(validate(&d, Some(100)).unwrap_err().contains("满分"));

        let mut d = base.clone();
        d.min_score = "150".to_string();
        // 口径没拉回来时不猜满分，交给后端裁决
        assert!(validate(&d, None).is_ok());

        let mut d = base.clone();
        d.keywords = "   ".to_string();
        assert!(validate(&d, Some(100)).unwrap_err().contains("关键词"));

        let clean = draft_from_reply(&reply());
        assert!(validate(&clean, Some(100)).is_ok());
    }

    #[test]
    fn over_max_threshold_is_not_sent_to_set_limit() {
        let base = draft_from_reply(&reply());
        let mut d = base.clone();
        d.min_score = "101".to_string();
        let plan = build_plan(&d, &base, Some(100));
        assert!(plan.min_score.is_none(), "超范围的值得先被本地校验拦住");
    }
}
