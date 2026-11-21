//! 投递列表（旧版图 2）：三页签 + 「查看全部」开关 + 排序 + 批量条 + 三列卡片网格。
//!
//! **页签口径**（用户裁决）：页签只有三个——未投递 / 已投递 / 不投递；
//! 「全部」是工具行上的一个独立开关（`job-tab-all`），打开时 `list_jobs`
//! 不带 `tab`，点任一页签时它自动关闭。
//!
//! **为什么保留下面的 sr-only 表格**：`tools/gui_e2e.py` 用
//! `document.querySelectorAll('tbody tr')` 数岗位条数（数到 0 就判失败）。
//! 卡片流里没有 `<tr>`，所以卡片之后必须补一张视觉隐藏的兼容表，
//! 否则 E2E 的「列表渲染出来了」断言会静默归零。它同时保留
//! `job-row-{id}` / `score-{id}` 两个旧 testid。

use leptos::prelude::*;

use crate::api::types::Job;
use crate::lib_ui::motion;
use crate::lib_ui::motion::class::ROW_IN;
use crate::lib_ui::time::fmt_time;

use super::cards::JobCard;
use super::state::{BatchOp, JobListState, ListTab, SortField};

/// 一次最多渲染多少张卡片。后端 `refresh` 已限制 `limit: 500`，这里再兜一层，
/// 免得一张异常大的结果把 DOM 撑爆。
const PAGE: usize = 500;

/// 列表渐变的类名交替（`list-fade` / `list-fade list-fade-b`）。
/// 同一个 animation-name 连续播放不会重新触发，必须换名。
fn fade_class(tick: u32) -> &'static str {
    if tick % 2 == 0 {
        "list-fade"
    } else {
        "list-fade list-fade-b"
    }
}

fn dash(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        "—".to_string()
    } else {
        t.to_string()
    }
}

/// 读取 checkbox 的勾选态。
fn event_checked(ev: &leptos::ev::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(false)
}

#[component]
pub fn JobList(
    state: JobListState,
    #[prop(optional)] max_score: Option<u8>,
    notify: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let fade_tick = RwSignal::new(0u32);
    // 列表内容变化时打一次渐入：长度变化就够判断了
    let last_len = RwSignal::new(usize::MAX);
    Effect::new(move |_| {
        let n = state.jobs.get().len();
        if n != last_len.get_untracked() {
            last_len.set(n);
            fade_tick.update(|t| *t = t.wrapping_add(1));
        }
    });

    // 可见列表：把「页签 + 查看全部」的组合收在一个 Memo 里，卡片、兼容表、
    // 批量条、计数全部读它，保证四处的口径永远一致。
    //
    // Memo 只存 `Vec<String>`（job_id）而不是 `Vec<Job>`：`Memo` 每次都要比较
    // 新旧值，而 `lib_ui::job::Job` 没有派生 `PartialEq`（给 28 个字段逐个比较
    // 没有意义）。卡片渲染再按 id 回查，代价是 O(n^2) 但 n 只有一屏。
    let visible_ids_memo = Memo::new(move |_| {
        let tab = state.tab.get();
        let show_all = state.show_all.get();
        state
            .visible(&state.jobs.get(), tab, show_all)
            .into_iter()
            .take(PAGE)
            .map(|j| j.job_id)
            .collect::<Vec<String>>()
    });
    let visible_ids = move || -> Vec<String> { visible_ids_memo.get() };
    let visible = move || -> Vec<Job> {
        let ids = visible_ids_memo.get();
        let all = state.jobs.get();
        ids.into_iter()
            .filter_map(|id| all.iter().find(|j| j.job_id == id).cloned())
            .collect()
    };

    view! {
        <section class="card job-list" data-testid="job-list">
            <header class="card-head">
                <h2 class="card-title">"投递列表"</h2>
                <span class="spacer"></span>
                <span class="dim job-head-count" data-testid="job-count">
                    {move || format!("共 {} 条", state.head_count(state.tab.get(), state.show_all.get()))}
                </span>
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="job-refresh"
                    on:click=move |_| state.refresh(fail)
                >
                    "刷新"
                </button>
            </header>

            // `is-fill`：卡片顶满父容器剩余高度，只有网格区自己滚
            // （用户裁决「投递列表默认撑满下发的空间」）。
            <div class="card-body job-body is-fill">
                // ---------- 页签行 ----------
                <div class="job-tabs-row">
                    <div class="job-tabs" role="tablist">
                        {ListTab::all().into_iter().map(|tab| {
                            view! {
                                <button
                                    type="button"
                                    role="tab"
                                    class=move || {
                                        let on = !state.show_all.get() && state.tab.get() == tab;
                                        if on { "job-tab is-active" } else { "job-tab" }
                                    }
                                    data-testid=format!("job-tab-{}", tab.slug())
                                    aria-selected=move || {
                                        (!state.show_all.get() && state.tab.get() == tab).to_string()
                                    }
                                    on:click=move |_| state.on_tab(tab, fail)
                                >
                                    {tab.label()}
                                    <span class="job-tab-count" data-testid=format!("job-tab-count-{}", tab.slug())>
                                        {move || state.head_count(tab, false).to_string()}
                                    </span>
                                </button>
                            }
                        }).collect_view()}
                        // 「查看全部」——不是页签，是工具行上的开关
                        <label class="job-all-toggle" title="不受页签限制，列出库里全部岗位">
                            <input
                                type="checkbox"
                                data-testid="job-tab-all"
                                prop:checked=move || state.show_all.get()
                                on:change=move |ev| state.on_show_all(event_checked(&ev), fail)
                            />
                            <span>"查看全部"</span>
                        </label>
                    </div>
                    <span class="spacer"></span>
                    <div class="job-sorts">
                        <span class="dim job-sort-label">"排序"</span>
                        {SortField::all().into_iter().map(|f| {
                            view! {
                                <button
                                    type="button"
                                    class=move || if state.sort.get() == f { "job-sort is-active" } else { "job-sort" }
                                    data-testid=format!("job-sort-{}", f.slug())
                                    title="点一次切换升降序"
                                    on:click=move |_| state.on_sort(f)
                                >
                                    {f.label()}
                                    <span class="job-sort-arrow">
                                        {move || if state.sort.get() == f {
                                            if state.ascending.get() { "↑" } else { "↓" }
                                        } else { "" }}
                                    </span>
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                // ---------- 批量条 ----------
                <div class="job-batch">
                    <label class="job-batch-check">
                        <input
                            type="checkbox"
                            data-testid="job-select-all"
                            prop:checked=move || {
                                let ids = visible_ids();
                                !ids.is_empty() && ids.iter().all(|id| state.is_checked(id))
                            }
                            on:change={
                                let ids_of = visible_ids;
                                move |ev| {
                                    let on = event_checked(&ev);
                                    state.set_check_all(ids_of(), on);
                                }
                            }
                        />
                        <span>"全选本页"</span>
                    </label>
                    <span class="dim job-selected" data-testid="job-selected-count">
                        {move || format!("已选 {} 条", state.checked.get().len())}
                    </span>
                    <span class="dim job-total" data-testid="job-list-total">
                        {move || format!("列表共 {} 条", visible().len())}
                    </span>
                    <span class="spacer"></span>
                    {move || {
                        let n = state.checked.get().len();
                        if n >= 2 {
                            view! {
                                <button
                                    type="button"
                                    class="btn btn-sm btn-danger"
                                    data-testid="job-batch-mark"
                                    on:click=move |_| state.batch(BatchOp::Reject, notify, fail)
                                >
                                    {format!("不投递（{n}）")}
                                </button>
                                <button
                                    type="button"
                                    class="btn btn-sm btn-ghost"
                                    data-testid="job-batch-allow"
                                    on:click=move |_| state.batch(BatchOp::Allow, notify, fail)
                                >
                                    {format!("允许投递（{n}）")}
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <span class="dim job-batch-hint">
                                    "勾选 2 条以上可批量标记；单条直接用行内标签"
                                </span>
                            }.into_any()
                        }
                    }}
                </div>

                // ---------- 卡片网格 ----------
                {move || {
                    let loading = state.loading.get();
                    let rows = visible();
                    if loading && rows.is_empty() {
                        return view! {
                            <div class="job-grid is-skeleton" data-testid="job-grid">
                                {(0..6).map(|i| view! {
                                    <div class="job-card is-skeleton" aria-hidden="true">
                                        <div class="skeleton-bar" style=format!("width:{}%", 70 + (i % 3) * 8)></div>
                                        <div class="skeleton-bar" style="width:45%"></div>
                                        <div class="skeleton-bar" style="width:60%"></div>
                                    </div>
                                }).collect_view()}
                            </div>
                        }.into_any();
                    }
                    if rows.is_empty() {
                        return view! {
                            <div class="job-grid" data-testid="job-grid">
                                <div class="empty-state" data-testid="job-empty">
                                    "这个页签下还没有岗位。先在上面展开筛选面板，填关键词后开始抓取。"
                                </div>
                            </div>
                        }.into_any();
                    }
                    let cls = format!("job-grid {}", fade_class(fade_tick.get_untracked()));
                    view! {
                        <div class=cls data-testid="job-grid">
                            {rows.into_iter().map(|job| view! {
                                <JobCard job=job state=state max=max_score notify=notify fail=fail/>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}

                // ---------- E2E 兼容表（视觉隐藏，见文件头注释）----------
                <table class="table sr-only" aria-hidden="true">
                    <tbody>
                        {move || {
                            visible().into_iter().map(|job| view! {
                                <tr data-testid=format!("job-row-{}", job.job_id)>
                                    <td data-testid=format!("score-{}", job.job_id)>
                                        {job.match_score.to_string()}
                                    </td>
                                    <td>{dash(&job.title)}</td>
                                    <td>{dash(&job.company)}</td>
                                    <td>{dash(&job.location)}</td>
                                    <td>{fmt_time(job.created_at)}</td>
                                </tr>
                            }).collect_view()
                        }}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

/// 卡片入场动画入口，供页面在整体刷新后调用。
pub fn replay(el: web_sys::Element) {
    motion::play_next_frame(el, ROW_IN);
}
