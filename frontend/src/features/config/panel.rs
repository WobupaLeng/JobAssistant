//! 配置面板：大模型 / 抓取默认参数 / 打分口径 / 数据目录与库概览。
//!
//! 布局沿用旧版 `.config-wrap` → `.card.config-tools` → `.config-groups` →
//! `.config-card` 三层；字段行沿用 `.cfg-grid` / `.cfg-grid-3` / `.cfg-field`。
//!
//! **只读**：命令层还没有 `get_app_config` / `set_app_config`，
//! 详见 `features/config/state.rs` 顶部说明。

use leptos::prelude::*;

use super::form::ConfigSection;
use super::state::{key_hint_text, llm_configured, ConfigState};
use crate::components::widgets::Chip;

#[component]
pub fn ConfigPanel(
    state: ConfigState,
    on_notice: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let llm_open = RwSignal::new(true);
    let scrape_open = RwSignal::new(true);
    let score_open = RwSignal::new(true);
    let data_open = RwSignal::new(true);

    // 分组标题右侧的状态胶囊：就绪 / 未配置
    let llm_chip = move || {
        let ready = llm_configured(&state.llm.get());
        let tone = if ready { "primary" } else { "warn" };
        let text = if ready { "已配置" } else { "未配置密钥" };
        view! { <Chip tone=tone.to_string()>{text}</Chip> }
    };

    let base_url = move || {
        state
            .llm
            .get()
            .map(|l| l.model.trim().to_string())
            // 命令层没回 base_url：这里显式说明「未读取」，而不是编一个默认值
            .map(|_| String::new())
            .unwrap_or_default()
    };

    let model_text = move || {
        let m = state
            .llm
            .get()
            .map(|l| l.model.trim().to_string())
            .unwrap_or_default();
        if m.is_empty() {
            "—".to_string()
        } else {
            m
        }
    };

    let prefilter = move || {
        state
            .llm
            .get()
            .map(|l| l.prefilter_default.to_string())
            .unwrap_or_else(|| "—".to_string())
    };

    let scale_total = move || {
        state
            .overview
            .get()
            .map(|o| o.total.to_string())
            .unwrap_or_else(|| "—".to_string())
    };

    let legacy_chip = move || {
        let legacy = state
            .overview
            .get()
            .map(|o| o.has_legacy)
            .unwrap_or(false);
        if legacy {
            view! { <Chip tone="warn">"存在十分制残留，请到概览页迁移"</Chip> }.into_any()
        } else {
            view! { <Chip tone="primary">"分数口径一致"</Chip> }.into_any()
        }
    };

    // `by_scale` 是 `(打分制标记, 条数)` 的列表：真实数据，不写死
    let by_scale = move || {
        state
            .overview
            .get()
            .map(|o| o.by_scale)
            .unwrap_or_default()
    };

    view! {
        <div class="config-wrap" data-testid="config-panel">
            <div class="card config-tools" data-testid="config-tools">
                <div class="row wrap" style="gap:10px">
                    <span class="card-sub">
                        "大模型与抓取参数由 Rust 侧命令读取；本页当前为只读诊断。"
                    </span>
                    <span class="spacer"></span>
                    {llm_chip}
                    <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        data-testid="config-reset-btn"
                        disabled=move || state.loading.get()
                        on:click=move |_| {
                            state.reload(fail);
                            on_notice.run("配置状态已重新读取".to_string());
                        }
                    >
                        {move || if state.loading.get() { "读取中…" } else { "重新加载" }}
                    </button>
                </div>
            </div>

            <div class="config-groups">
                <ConfigSection
                    title="大模型".to_string()
                    subtitle="密钥只回显尾 4 位"
                    open=llm_open
                    test_id="config-llm-section"
                >
                    <div class="cfg-grid">
                        <label class="field cfg-field">
                            <span class="label">"API Key"</span>
                            <input
                                class="input"
                                type="password"
                                data-testid="config-llm-key"
                                readonly
                                placeholder=move || key_hint_text(&state.llm.get())
                                prop:value=move || String::new()
                            />
                            <span class="field-hint">
                                "留空表示不改；当前："
                                {move || key_hint_text(&state.llm.get())}
                            </span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"Base URL"</span>
                            <input
                                class="input"
                                data-testid="config-llm-base-url"
                                readonly
                                placeholder="命令层未下发"
                                prop:value=base_url
                            />
                            <span class="field-hint">"需新增 get_app_config 命令后才能回显"</span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"模型"</span>
                            <input
                                class="input"
                                data-testid="config-llm-model"
                                readonly
                                placeholder="未配置"
                                prop:value=model_text
                            />
                        </label>
                        <label class="field cfg-field">
                            <span class="label">
                                "大模型初筛线"
                                <span class="field-hint">"（低于此分不调用大模型）"</span>
                            </span>
                            <input
                                class="input input-num"
                                type="number"
                                data-testid="config-llm-prefilter"
                                readonly
                                prop:value=prefilter
                            />
                        </label>
                    </div>
                </ConfigSection>

                <ConfigSection
                    title="抓取默认参数".to_string()
                    subtitle="关键词、上限与入库门槛"
                    open=scrape_open
                    test_id="config-scrape-section"
                >
                    <div class="cfg-grid">
                        <label class="field cfg-field">
                            <span class="label">"关键词"</span>
                            <input
                                class="input"
                                data-testid="config-keywords"
                                readonly
                                placeholder="命令层未下发"
                                prop:value=move || String::new()
                            />
                            <span class="field-hint">"需新增 get_app_config 命令"</span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"单次抓取上限"</span>
                            <input
                                class="input input-num"
                                type="number"
                                data-testid="config-limit"
                                readonly
                                placeholder="—"
                                prop:value=move || String::new()
                            />
                        </label>
                        <label class="field cfg-field">
                            <span class="label">
                                "入库门槛"
                                <span class="field-hint">"（默认值由 get_scoring_spec 下发）"</span>
                            </span>
                            <input
                                class="input input-num"
                                type="number"
                                data-testid="config-min-score"
                                readonly
                                prop:value=move || {
                                    state
                                        .spec
                                        .get()
                                        .map(|s| s.score_min_default.to_string())
                                        .unwrap_or_else(|| "—".to_string())
                                }
                            />
                        </label>
                    </div>
                </ConfigSection>

                <ConfigSection
                    title="打分口径".to_string()
                    subtitle="唯一真源：core::scoring"
                    open=score_open
                    test_id="config-score-section"
                >
                    <div class="cfg-grid-3">
                        <div class="cfg-field">
                            <div class="label">"满分"</div>
                            <div class="cfg-value" data-testid="config-score-max">
                                {move || {
                                    state
                                        .spec
                                        .get()
                                        .map(|s| s.score_max.to_string())
                                        .unwrap_or_else(|| "—".to_string())
                                }}
                            </div>
                        </div>
                        <div class="cfg-field">
                            <div class="label">"默认门槛"</div>
                            <div class="cfg-value" data-testid="config-score-min">
                                {move || {
                                    state
                                        .spec
                                        .get()
                                        .map(|s| s.score_min_default.to_string())
                                        .unwrap_or_else(|| "—".to_string())
                                }}
                            </div>
                        </div>
                        <div class="cfg-field">
                            <div class="label">"五维权重"</div>
                            <div class="cfg-value cfg-value-sm">
                                "技能 40 · 薪资 20 · 经验 15 · 方向 15 · 公司质量 10"
                            </div>
                        </div>
                    </div>
                </ConfigSection>

                <ConfigSection
                    title="数据目录与库概览".to_string()
                    subtitle="本地 SQLite"
                    open=data_open
                    test_id="config-data-section"
                >
                    <div class="cfg-grid">
                        <label class="field cfg-field">
                            <span class="label">"数据目录"</span>
                            <input
                                class="input"
                                data-testid="config-data-dir"
                                readonly
                                placeholder="%LOCALAPPDATA%\\ai-job-assistant"
                                prop:value=move || {
                                    state
                                        .info
                                        .get()
                                        .map(|i| format!("{} v{}（{}）", i.name, i.version, i.shell))
                                        .unwrap_or_default()
                                }
                            />
                            <span class="field-hint">
                                "数据库与浏览器 profile 都在此目录；命令层未下发绝对路径"
                            </span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"岗位总数"</span>
                            <input
                                class="input input-num"
                                type="number"
                                data-testid="config-db-total"
                                readonly
                                prop:value=scale_total
                            />
                        </label>
                    </div>
                    <div class="row wrap" style="gap:8px;margin-top:10px">
                        {legacy_chip}
                        <For each=by_scale key=|p: &(String, i64)| p.0.clone() let:pair>
                            <Chip tone="default">
                                {format!("{} 制 · {} 条", scale_label(&pair.0), pair.1)}
                            </Chip>
                        </For>
                        <Show when=move || by_scale().is_empty()>
                            <span class="card-sub">"库里还没有按打分制统计的数据"</span>
                        </Show>
                    </div>
                </ConfigSection>
            </div>
        </div>
    }
}

/// `by_scale` 的键是打分制标记（`"100"` / `"10"` / `"null"`），转成人话。
fn scale_label(key: &str) -> String {
    match key {
        "100" => "百分".to_string(),
        "10" => "十分".to_string(),
        "null" | "" => "未标记".to_string(),
        other => other.to_string(),
    }
}
