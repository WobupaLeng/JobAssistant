//! 配置页的状态：已存配置（`get_app_config`）、编辑草稿、保存中/保存结果，
//! 以及大模型状态、库概览、打分口径这四份只读诊断数据。
//!
//! **接线关系**（2026-11-21 起）：
//! - 读：`reload()` 并发拉 `get_app_config` / `llm_status` / `db_overview` /
//!   `get_scoring_spec` / `app_info`，其中 `get_app_config` 的回执经
//!   [`draft::draft_from_reply`] 同时写入 `stored`（基线）与 `draft`（编辑框）。
//! - 写：`save()` 由 [`draft::build_plan`] 算出两条命令各要提交什么——
//!   `set_app_config` 只拿 `WRITABLE_KEYS` 里改过的键，门槛与上限只走 `set_limit`。
//!
//! **密钥红线**：`AppConfigReply` 只带 `api_key_hint`（`****尾4位`），草稿里的
//! `api_key` 初始为空串；[`masked_key_hint`] 会把任何 `****` 形式的值再掩一遍，
//! 保证界面上出现的永远是尾 4 位，且不打日志。

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::draft::{self, ConfigDraft, SavePlan};
use crate::api::{self, types::*};

#[derive(Clone, Copy)]
pub struct ConfigState {
    pub llm: RwSignal<Option<LlmStatus>>,
    pub overview: RwSignal<Option<DbOverview>>,
    pub spec: RwSignal<Option<ScoringSpec>>,
    pub info: RwSignal<Option<AppInfo>>,
    /// `get_app_config` 的原始回执：只读字段都从它取（数据目录、密钥尾 4 位…）
    pub config: RwSignal<Option<AppConfigReply>>,
    /// 上次成功读/写后的基线。`build_plan` 拿它做 diff，避免整份回传
    pub stored: RwSignal<ConfigDraft>,
    /// 编辑中的草稿（输入框双向绑定的唯一真源）
    pub draft: RwSignal<ConfigDraft>,
    /// 上一次保存的实际写入项数与是否成功；`None` = 本次没保存过
    pub saved: RwSignal<Option<(usize, bool)>>,
    pub saving: RwSignal<bool>,
    /// 页面内可见的保存错误。**不能只丢给顶栏**——顶栏的提示可能已被别处覆盖，
    /// 而「保存失败」是用户必须看到的结果（AGENTS.md 2.5：不得静默吞错）
    pub save_error: RwSignal<Option<String>>,
    pub loading: RwSignal<bool>,
    pub note: RwSignal<String>,
}

impl ConfigState {
    pub fn new() -> Self {
        Self {
            llm: RwSignal::new(None),
            overview: RwSignal::new(None),
            spec: RwSignal::new(None),
            info: RwSignal::new(None),
            config: RwSignal::new(None),
            stored: RwSignal::new(ConfigDraft::default()),
            draft: RwSignal::new(ConfigDraft::default()),
            saved: RwSignal::new(None),
            saving: RwSignal::new(false),
            save_error: RwSignal::new(None),
            loading: RwSignal::new(true),
            note: RwSignal::new(String::new()),
        }
    }

    /// 口径里的满分。**禁止硬编码 100**（AGENTS.md 2.3）：只能来自 `get_scoring_spec`。
    /// 口径还没回来时是 `None`，校验与保存时据此只做「是不是数字」的检查。
    pub fn score_max(self) -> Option<u8> {
        self.spec.get().map(|s| s.score_max)
    }

    /// 五份数据并发拉取。任一失败进错误条，其余照常显示——
    /// 这几项之间没有依赖，不该互相拖累。
    ///
    /// 只失败一个 `get_app_config` 时，**编辑框保持原样**（不清空成空串）：
    /// 把用户正在看的配置抹成空白，比显示一条错误糟得多。
    pub fn reload(self, fail: Callback<String>) {
        self.loading.set(true);
        spawn_local(async move {
            let (cfg, llm, overview, spec, info) = (
                api::get_app_config().await,
                api::llm_status().await,
                api::db_overview().await,
                api::scoring_spec().await,
                api::app_info().await,
            );
            match cfg {
                Ok(v) => {
                    let d = draft::draft_from_reply(&v);
                    self.stored.set(d.clone());
                    self.draft.set(d);
                    self.config.set(Some(v));
                }
                Err(e) => fail.run(format!("读取应用配置失败：{e}")),
            }
            match llm {
                Ok(v) => self.llm.set(Some(v)),
                Err(e) => fail.run(format!("读取大模型状态失败：{e}")),
            }
            match overview {
                Ok(v) => self.overview.set(Some(v)),
                Err(e) => fail.run(format!("读取库概览失败：{e}")),
            }
            match spec {
                Ok(v) => {
                    self.spec.set(Some(v));
                    // 顶栏的口径也一起更新：两处数字必须同源
                    self.note.set(String::new());
                }
                Err(e) => fail.run(format!("读取打分口径失败：{e}")),
            }
            match info {
                Ok(v) => self.info.set(Some(v)),
                Err(e) => fail.run(format!("读取应用信息失败：{e}")),
            }
            self.loading.set(false);
        });
    }

    /// 保存草稿。成功 → 顶栏提示「已保存（N 项）」并刷新基线；
    /// 失败 → 页面内的错误条 + 顶栏各说一次，二者都不静默。
    pub fn save(self, fail: Callback<String>, notice: Callback<String>) {
        if self.saving.get_untracked() {
            return;
        }
        let d = self.draft.get_untracked();
        // 先本地校验：算错的值不该打到后端去换一句英文 serde 错误
        if let Err(e) = draft::validate(&d, self.score_max()) {
            self.save_error.set(Some(e.clone()));
            return;
        }
        let plan = draft::build_plan(&d, &self.stored.get_untracked(), self.score_max());
        if plan.is_empty() {
            self.save_error.set(None);
            self.saved.set(Some((0, true)));
            notice.run("没有需要保存的改动".to_string());
            return;
        }
        self.saving.set(true);
        self.save_error.set(None);
        self.saved.set(None);
        spawn_local(async move {
            match write_plan(&plan).await {
                Ok(written) => {
                    // 保存成功后重建基线：再点一次保存就是「没有改动」，
                    // 不会把刚存的值当成改动再发一遍
                    self.stored.set(d.clone());
                    self.saved.set(Some((written, true)));
                    notice.run(format!("配置已保存（{written} 项）"));
                    // 回读一次：`set_limit` 会归一化/夹住数值，界面要显示**生效后**的值
                    if let Ok(v) = api::get_app_config().await {
                        let fresh = draft::draft_from_reply(&v);
                        self.stored.set(fresh.clone());
                        self.draft.set(fresh);
                        self.config.set(Some(v));
                    }
                }
                Err(e) => {
                    self.saved.set(Some((0, false)));
                    self.save_error.set(Some(e.clone()));
                    fail.run(format!("保存配置失败：{e}"));
                }
            }
            self.saving.set(false);
        });
    }
}

/// 按计划依次写两条命令，返回实际写入项数。
///
/// 顺序固定为 `set_app_config` → `set_limit`：前者会重建大模型客户端，
/// 后者只写两个整数。任一失败立即返回**带命令名**的错误——
/// 「保存失败」却不说是哪条命令，用户无法判断改动有没有部分生效。
async fn write_plan(plan: &SavePlan) -> Result<usize, String> {
    let mut written = 0usize;
    if let Some(patch) = &plan.patch {
        let v = serde_json::Value::Object(patch.clone());
        let r = api::set_app_config(v).await?;
        written += r.updated;
    }
    if plan.max_jd.is_some() || plan.min_score.is_some() {
        let r = api::set_limit(plan.max_jd, plan.min_score).await?;
        written += usize::from(plan.max_jd.is_some());
        written += usize::from(plan.min_score.is_some());
        // 后端裁决后的值才是生效值；不相等时如实说明（例如被夹到上限）
        if let Some(want) = plan.max_jd {
            if r.max_jd != want {
                return Err(format!(
                    "单次抓取上限提交 {want}，后端生效为 {}（超范围会被夹住）",
                    r.max_jd
                ));
            }
        }
    }
    Ok(written)
}

/// 密钥提示：`•••• abcd`。
///
/// 输入只用 `api_key_hint`（`****abcd`）；若回执里混进原文（不该发生），
/// 这里也只取尾 4 位——**密钥永不完整出现在界面上**。
pub fn masked_key_hint(cfg: &Option<AppConfigReply>, llm: &Option<LlmStatus>) -> String {
    let raw = cfg
        .as_ref()
        .map(|c| c.api_key_hint.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            llm.as_ref()
                .map(|l| l.key_hint.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    if raw.is_empty() {
        return "未配置".to_string();
    }
    let tail: String = raw
        .trim_start_matches('*')
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.is_empty() || tail.chars().all(|c| c == '*') {
        "已配置".to_string()
    } else {
        format!("•••• {tail}")
    }
}

/// 大模型是否可用；未配置时 `key_hint` 为空。
pub fn llm_configured(llm: &Option<LlmStatus>) -> bool {
    llm.as_ref().map(|l| l.configured).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(hint: &str, has_key: bool) -> Option<AppConfigReply> {
        Some(AppConfigReply {
            api_key_hint: hint.to_string(),
            has_key,
            ..Default::default()
        })
    }

    #[test]
    fn key_hint_only_ever_shows_the_tail() {
        assert_eq!(masked_key_hint(&cfg_with("****9876", true), &None), "•••• 9876");
        assert_eq!(masked_key_hint(&cfg_with("", false), &None), "未配置");
        // 回执里混进原文时（不该发生）也只露尾 4 位
        assert_eq!(
            masked_key_hint(&cfg_with("sk-super-secret-9876", true), &None),
            "•••• 9876"
        );
        // 只有掩码、没有尾位
        assert_eq!(masked_key_hint(&cfg_with("****", true), &None), "已配置");
    }

    #[test]
    fn falls_back_to_llm_status_when_config_is_missing() {
        let llm = Some(LlmStatus {
            configured: true,
            key_hint: "1234".to_string(),
            ..Default::default()
        });
        assert_eq!(masked_key_hint(&None, &llm), "•••• 1234");
        assert!(llm_configured(&llm));
    }
}
