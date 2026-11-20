//! 配置页的状态：大模型状态、库概览、口径。
//!
//! **为什么没有可编辑表单**：现 Tauri 命令层只暴露了读用的
//! `llm_status` / `db_overview` / `get_scoring_spec` / `app_info`，
//! 没有 `get_app_config` / `set_app_config`（旧版 TS 有，Rust 侧未实现）。
//! 造一个「点保存但什么也没写库」的假表单比只读更糟——用户会以为配置生效了。
//! 因此本页是**只读诊断 + 数据目录**，写入等命令层补齐后再做
//! （见报告 CONTRACT-IMPACT）。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::*};

#[derive(Clone, Copy)]
pub struct ConfigState {
    pub llm: RwSignal<Option<LlmStatus>>,
    pub overview: RwSignal<Option<DbOverview>>,
    pub spec: RwSignal<Option<ScoringSpec>>,
    pub info: RwSignal<Option<AppInfo>>,
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
            loading: RwSignal::new(true),
            note: RwSignal::new(String::new()),
        }
    }

    /// 四份数据并发拉取。任一失败进错误条，其余照常显示——
    /// 这几项之间没有依赖，不该互相拖累。
    pub fn reload(self, fail: Callback<String>) {
        self.loading.set(true);
        spawn_local(async move {
            let (llm, overview, spec, info) = (
                api::llm_status().await,
                api::db_overview().await,
                api::scoring_spec().await,
                api::app_info().await,
            );
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
}

/// 大模型是否可用；未配置时 `key_hint` 为空。
pub fn llm_configured(llm: &Option<LlmStatus>) -> bool {
    llm.as_ref().map(|l| l.configured).unwrap_or(false)
}

/// 密钥只显示尾 4 位。为空时给一句说明而不是空白。
pub fn key_hint_text(llm: &Option<LlmStatus>) -> String {
    let hint = llm
        .as_ref()
        .map(|l| l.key_hint.trim().to_string())
        .unwrap_or_default();
    if hint.is_empty() {
        "未配置".to_string()
    } else {
        format!("•••• {hint}")
    }
}
