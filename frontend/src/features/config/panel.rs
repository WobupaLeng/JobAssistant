//! 配置面板：大模型 / 抓取默认参数 / 打分口径 / 数据目录与库概览。
//!
//! 布局沿用旧版 `.config-wrap` → `.card.config-tools` → `.config-groups` →
//! `.config-card` 三层；字段行沿用 `.cfg-grid` / `.cfg-grid-3` / `.cfg-field`。
//!
//! **可读可写**：写入表单只在用户点「保存」时落库（[`crate::features::config::draft`]
//! 负责算「哪几个键走哪条命令」）。保存失败在页面内有一条 `ErrorBar`，
//! 同时顶栏也会收到一次提示——两处都看得见，不会静默吞掉。
//!
//! 只读语义区分：
//! - **可编辑**：API Key / Base URL / 模型 / 大模型初筛线 / 关键词 / 单次抓取上限 / 入库门槛
//! - **派生只读**（`aria-readonly="true"`，样式见 `styles/controls.css`）：
//!   数据目录、岗位总数——它们由库与应用路径算出来，不是用户能填的配置项

use leptos::prelude::*;

use super::form::ConfigSection;
use super::state::{llm_configured, masked_key_hint, ConfigState};
use crate::components::widgets::{Chip, ErrorBar};

/// 输入控件的值（`input` / `textarea` / `select` 都走这里）。
fn input_value(ev: &leptos::ev::Event) -> String {
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
        .unwrap_or_default()
}

/// 把一次输入写进草稿的对应字段。
///
/// 只 `update` 一个字段而不是整体 `set`：草稿是 `Copy` 的 `RwSignal`，
/// 整体覆盖会让「同时在改第二个框」的那一刻把上一个框的值写回旧值。
macro_rules! bind_input {
    ($state:expr, $field:ident) => {
        move |ev: leptos::ev::Event| {
            let v = input_value(&ev);
            $state.draft.update(|d| d.$field = v);
        }
    };
}

/// 一个可编辑的文本输入框：`prop:value` 回显草稿，`on:input` 写回草稿。
///
/// 用 `prop:value` 而不是 `value=`：后者只设 HTML 属性，输入过之后
/// 用户敲的内容与属性脱节，回读（`get_app_config` 之后）就刷不回去了。
///
/// 数字框用 `type="number"`，其它属性（`placeholder` / `min` / `max`）由调用方补。
macro_rules! editable {
    ($state:expr, $field:ident, $class:expr, $ty:expr $(, $attr:ident = $attr_val:expr)*) => {
        view! {
            <input
                class=$class
                type=$ty
                data-testid=concat!("config-", stringify!($field))
                prop:value=move || $state.draft.get().$field
                on:input=bind_input!($state, $field)
                $( $attr = $attr_val )*
            />
        }
    };
}

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

    // 密钥提示：恒为「•••• 尾4位」或「未配置」，绝不出现明文
    let key_hint = move || masked_key_hint(&state.config.get(), &state.llm.get());

    let scale_total = move || {
        state
            .overview
            .get()
            .map(|o| o.total.to_string())
            .unwrap_or_else(|| "—".to_string())
    };

    let data_dir = move || {
        state
            .config
            .get()
            .map(|c| c.data_dir.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
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
                        "改动只在点「保存」时落库：文本框走 set_app_config，门槛与上限走 set_limit。"
                    </span>
                    <span class="spacer"></span>
                    {llm_chip}
                    <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        data-testid="config-reset-btn"
                        disabled=move || state.loading.get() || state.saving.get()
                        on:click=move |_| {
                            state.reload(fail);
                            on_notice.run("配置状态已重新读取".to_string());
                        }
                    >
                        {move || if state.loading.get() { "读取中…" } else { "重新加载" }}
                    </button>
                    <button
                        type="button"
                        class="btn btn-sm btn-primary"
                        data-testid="config-save-btn"
                        disabled=move || state.saving.get() || state.loading.get()
                        on:click=move |_| state.save(fail, on_notice)
                    >
                        {move || if state.saving.get() { "保存中…" } else { "保存" }}
                    </button>
                </div>
                // 保存结果：成功一行、失败一行，都在页面里看得见
                {move || {
                    let err = state.save_error.get();
                    let ok = state.saved.get();
                    match (err, ok) {
                        (Some(e), _) => {
                            Some(view! {
                                <div class="config-feedback">
                                    <ErrorBar
                                        message=e
                                        on_dismiss=Callback::new(move |_| state.save_error.set(None))
                                        test_id="config-save-error".to_string()
                                    />
                                </div>
                            }
                            .into_any())
                        }
                        (None, Some((n, true))) => Some(
                            view! {
                                <div class="config-feedback" data-testid="config-save-ok">
                                    <Chip tone="primary">
                                        {if n == 0 { "没有改动，未写入".to_string() } else { format!("已保存 {n} 项") }}
                                    </Chip>
                                </div>
                            }
                            .into_any(),
                        ),
                        _ => None,
                    }
                }}
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
                            {editable!(
                                state, api_key, "input", "password",
                                autocomplete = "new-password",
                                placeholder = move || key_hint()
                            )}
                            <span class="field-hint">
                                "留空表示不改；当前："
                                {key_hint}
                            </span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"Base URL"</span>
                            {editable!(
                                state, base_url, "input", "text",
                                placeholder = "https://api.deepseek.com"
                            )}
                            <span class="field-hint">"OpenAI 兼容接口的根地址"</span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"模型"</span>
                            {editable!(
                                state, model, "input", "text",
                                placeholder = "deepseek-chat"
                            )}
                        </label>
                        <label class="field cfg-field">
                            <span class="label">
                                "大模型初筛线"
                                <span class="field-hint">"（低于此分不调用大模型）"</span>
                            </span>
                            {editable!(
                                state, llm_prefilter, "input input-num", "number",
                                min = "0",
                                max = "255"
                            )}
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
                            {editable!(
                                state, keywords, "input", "text",
                                placeholder = "Java, 后端（逗号分隔）"
                            )}
                            <span class="field-hint">"多个关键词用逗号或顿号分隔"</span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">"单次抓取上限"</span>
                            {editable!(state, max_jd, "input input-num", "number", min = "0")}
                            <span class="field-hint">"最多补多少个岗位的详情；0 = 不限制"</span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">
                                "入库门槛"
                                <span class="field-hint">"（满分由 get_scoring_spec 下发）"</span>
                            </span>
                            {editable!(
                                state, min_score, "input input-num", "number",
                                min = "0",
                                max = move || {
                                    state
                                        .score_max()
                                        .map(|m| m.to_string())
                                        .unwrap_or_default()
                                }
                            )}
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
                            <span class="label">
                                "数据目录"
                                <span class="field-hint">"（只读）"</span>
                            </span>
                            <input
                                class="input"
                                data-testid="config-data-dir"
                                readonly
                                aria-readonly="true"
                                tabindex="-1"
                                placeholder="读取中…"
                                prop:value=data_dir
                            />
                            <span class="field-hint">
                                "数据库与浏览器 profile 都在此目录；由应用路径决定，不可在此修改"
                            </span>
                        </label>
                        <label class="field cfg-field">
                            <span class="label">
                                "岗位总数"
                                <span class="field-hint">"（只读）"</span>
                            </span>
                            <input
                                class="input input-num"
                                type="number"
                                data-testid="config-db-total"
                                readonly
                                aria-readonly="true"
                                tabindex="-1"
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
