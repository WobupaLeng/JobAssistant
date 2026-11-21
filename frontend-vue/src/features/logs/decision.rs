//! 决策日志视图：kind 筛选 + 关键字 + `DecisionRecord` 的真实字段渲染。
//!
//! 字段以 `core::domain::store_ports::DecisionRecord` 为准：
//! `company_job`（写成 `公司/岗位`）、`decision`、`reason`、`created_at`（秒）、
//! `old_score` / `new_score`（可空）。
//!
//! **分数空值处理**（用户特别点名）：`rescore` 目前只把「旧 → 新」写进
//! `reason` 文本，两列不落值；历史行同样是 NULL。因此这里用
//! `lib_ui::logfmt::decision_scores` 先读列、列全空再从 `reason` 里抠，
//! 抠不出来就渲染 `—`。任何路径下都不会出现 `null` / `undefined` / 假的 `0 → 0`。

use leptos::prelude::*;

use super::state::{DecisionFilter, DecisionRow, LogState};
use crate::components::widgets::{Chip, EmptyState, SkeletonRows};
use crate::lib_ui::logfmt::{fmt_decision_time, ScoreDelta};

#[component]
pub fn DecisionView(state: LogState) -> impl IntoView {
    let total = Signal::derive(move || state.decisions.get().len());
    let shown = Signal::derive(move || state.visible_decisions().len());
    let filter = state.dec_filter;

    view! {
        <div class="log-view" data-testid="logs-view-decision">
            <div class="row wrap log-tools" style="gap:10px">
                <div class="tabs" role="group" aria-label="按行为筛选">
                    {DecisionFilter::all()
                        .iter()
                        .map(|f| {
                            let f = *f;
                            view! {
                                <button
                                    type="button"
                                    class=move || {
                                        if filter.get() == f { "tab is-active" } else { "tab" }
                                    }
                                    aria-pressed=move || {
                                        if filter.get() == f { "true" } else { "false" }
                                    }
                                    data-testid=format!("logs-kfilter-{}", f.slug())
                                    on:click=move |_| filter.set(f)
                                >
                                    {f.label()}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <input
                    class="input log-search"
                    type="search"
                    data-testid="logs-search"
                    placeholder="搜索公司 / 岗位 / 原因"
                    prop:value=move || state.dec_keyword.get()
                    on:input=move |ev| state.dec_keyword.set(event_target_value(&ev))
                />
                <span class="spacer"></span>
                <span class="log-count" data-testid="logs-decision-count">
                    {move || {
                        let (s, t) = (shown.get(), total.get());
                        if s == t { format!("{t} 条") } else { format!("{s} / {t} 条") }
                    }}
                </span>
            </div>

            <Show when=move || !state.dec_error.get().is_empty()>
                <div class="log-inline-error" data-testid="logs-decision-error">
                    {move || state.dec_error.get()}
                </div>
            </Show>

            <Show
                when=move || !state.dec_loading.get()
                fallback=move || {
                    view! {
                        <table class="table log-table">
                            <tbody>
                                <SkeletonRows cols=5 rows=8 />
                            </tbody>
                        </table>
                    }
                }
            >
                <Show
                    when=move || !state.decisions.get().is_empty()
                    fallback=move || {
                        view! {
                            <EmptyState test_id="logs-decision-empty">
                                <span>"还没有决策记录。"</span>
                                <span>
                                    "每一条被排除或入库的岗位、以及每次「全量重算」改动的分数，都会记在这里。"
                                </span>
                            </EmptyState>
                        }
                    }
                >
                    <Show
                        when=move || !state.visible_decisions().is_empty()
                        fallback=move || {
                            view! {
                                <EmptyState test_id="logs-decision-filtered-empty">
                                    "没有匹配的决策记录：换个行为筛选或关键字"
                                </EmptyState>
                            }
                        }
                    >
                        <table class="table log-table" data-testid="logs-decision-table">
                            <thead>
                                <tr>
                                    <th style="width:92px">"时间"</th>
                                    <th style="width:64px">"行为"</th>
                                    <th>"公司 / 岗位"</th>
                                    <th style="width:112px">"分数变化"</th>
                                    <th>"原因"</th>
                                </tr>
                            </thead>
                            <tbody class="list-fade">
                                <For
                                    each=move || state.visible_decisions()
                                    key=|r: &DecisionRow| r.id
                                    let:row
                                >
                                    <DecisionLine row=row />
                                </For>
                            </tbody>
                        </table>
                    </Show>
                </Show>
            </Show>
        </div>
    }
}

#[component]
fn DecisionLine(row: DecisionRow) -> impl IntoView {
    // 先算，再 move 进 view：`row` 会被多处借用
    let time = fmt_decision_time(row.created_at);
    let kind_label = row.label();
    let kind_tone = row.tone().to_string();
    let raw_kind = row.decision.clone();
    let score = row.score();
    let score_id = format!("logs-decision-score-{}", row.id);
    let reason = row.reason_text();
    let reason_cls = if reason.starts_with("（未记录") {
        "log-reason is-empty"
    } else {
        "log-reason"
    };
    let company = if row.company.is_empty() {
        "（未知公司）".to_string()
    } else {
        row.company.clone()
    };
    let title = if row.title.is_empty() {
        "（未记录岗位）".to_string()
    } else {
        row.title.clone()
    };
    let row_cls = format!("log-row is-{}", row.tone());

    view! {
        <tr class=row_cls data-testid=format!("logs-decision-row-{}", row.id)>
            <td class="log-time">{time}</td>
            <td class="log-kind">
                <Chip tone=kind_tone title=raw_kind>{kind_label}</Chip>
            </td>
            <td class="log-job">
                <div class="log-company">{company}</div>
                <div class="log-title">{title}</div>
            </td>
            <td class="log-score">
                <ScoreCell score=score data_testid=score_id />
            </td>
            <td class=reason_cls>{reason}</td>
        </tr>
    }
}

/// 分数单元格。两端都有且不相等才画箭头并着色；`—` 不上色、不加箭头。
#[component]
fn ScoreCell(score: ScoreDelta, data_testid: String) -> impl IntoView {
    let text = score.text.clone();
    let cls = format!("log-score-val is-{}", score.dir);
    let arrow = score.changed();
    let tip = score_title(score.dir);
    view! {
        <span class=cls data-testid=data_testid title=tip>
            {if arrow { "▲ " } else { "" }}
            {text}
        </span>
    }
}

/// hover 提示：把方向讲成人话。
fn score_title(dir: &'static str) -> &'static str {
    match dir {
        "up" => "分数上调",
        "down" => "分数下调",
        "same" => "分数未变",
        _ => "没有分数记录",
    }
}
