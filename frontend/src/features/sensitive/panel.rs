//! 敏感库入口按钮 + 居中浮层（旧版图 1）。
//!
//! ## 为什么是「一个组件两个出口」
//! 入口按钮必须由页面层放在工具条里，而浮层是 `position: fixed` 的全屏层。
//! 做成两个组件就要页面层额外持有一份 `open` 信号；这里收成一个组件，
//! 页面层只需要 `let open = RwSignal::new(false);` 再挂 `<SensitivePanel open/>`。
//!
//! ## 与详情抽屉互斥
//! `on_open` / `on_close` 回调由页面层决定（打开时顺手把详情抽屉关掉）。
//! 组件自己不持有详情抽屉的状态——那是 `features/jobs` 的地盘。
//!
//! ## 数据
//! 只在「打开」与「搜索 / 筛选 / 移出后」拉一次 `sensitive_list`，
//! **不轮询**：敏感库的变化只可能来自本抽屉自己的写操作或抓取进程，
//! 前者我们已经显式刷新，后者由页面层的抓取进度驱动刷新（`reload` 信号）。

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::list::SensitiveList;
use super::state::{
    remove_selected_label, title_text, Filter, SensitiveState,
};
use crate::api;
use crate::components::widgets::{EmptyState, ErrorBar};

/// 一次拉多少条。旧版是 200；三列网格下 200 条卡片会渲染出一万多个节点，
/// 抽屉打开就卡。这里取 60（三列 × 20 行），配合 `total` 显示真实总数。
const PAGE_LIMIT: u32 = 60;

#[component]
pub fn SensitivePanel(
    /// 由页面层持有：浮层是否可见。
    open: RwSignal<bool>,
    /// `None` 时不渲染入口按钮（例如页面层只想保留浮层）。
    #[prop(optional)] show_entry: Option<bool>,
    /// 打开前回调（页面层据此关掉岗位详情抽屉）。
    #[prop(optional, into)] on_open: Option<Callback<()>>,
    /// 关闭后回调。
    #[prop(optional, into)] on_close: Option<Callback<()>>,
    /// 外部递增即触发一次重新拉取（抓取入库后刷新条数）。
    #[prop(optional, into)] reload: Option<RwSignal<u32>>,
) -> impl IntoView {
    let state = RwSignal::new(SensitiveState::default());
    // 抽屉没打开过就不该有 IPC：首次打开前 state.loading 恒为 false。
    let loaded = RwSignal::new(false);
    // 输入框的受控值。`value=` 在 Leptos 里是 attribute，输入后 DOM 与信号会脱钩，
    // 所以重置时显式写回（见 `reset_search`）。
    let input_el = NodeRef::<leptos::html::Input>::new();

    // 移出动作：单条与批量走同一条路径。
    let remove = {
        let state = state;
        Callback::new(move |ids: Vec<i64>| {
            if ids.is_empty() {
                return;
            }
            let ids_snapshot = ids.clone();
            spawn_local(async move {
                match api::sensitive_remove(ids_snapshot.clone()).await {
                    Ok(0) => {
                        state.update(|s| s.error = "这些条目已经不在敏感库里了".to_string());
                    }
                    Ok(_) => {}
                    Err(e) => state.update(|s| s.error = format!("移出失败：{e}")),
                }
                // 无论成功与否都刷新：失败时也要让界面回到后端真实状态
                for id in ids_snapshot {
                    state.update(|s| {
                        s.selected.remove(&id);
                    });
                }
                reload_now(state).await;
            });
        })
    };

    let request = move || {
        // 先把查询参数拷成拥有所有权的 `String`：`query()` 返回的是对状态的借用，
        // 而下面的 `state.update(...)` 需要独占借用同一个信号。
        let query = state.get_untracked();
        let platform = query.query().0.map(str::to_string);
        let keyword = query.query().1.map(str::to_string);
        let kind = query.query().2.map(str::to_string);
        drop(query);
        state.update(|s| {
            s.loading = true;
            s.error.clear();
        });
        spawn_local(async move {
            match api::sensitive_list(
                platform.as_deref(),
                keyword.as_deref(),
                kind.as_deref(),
                Some(PAGE_LIMIT),
                Some(0),
            )
            .await
            {
                Ok(page) => state.update(|s| {
                    s.items = page.items;
                    s.total = page.total;
                    s.selected.clear();
                    s.loading = false;
                }),
                Err(e) => state.update(|s| {
                    s.loading = false;
                    s.error = format!("读取敏感库失败：{e}");
                }),
            }
        });
    };

    // 打开 → 拉一次；之后靠显式动作刷新
    Effect::new(move |_| {
        if open.get() {
            if !loaded.get_untracked() {
                loaded.set(true);
            }
            request();
        }
    });

    // 外部刷新信号
    if let Some(sig) = reload {
        Effect::new(move |_| {
            sig.track();
            if open.get_untracked() {
                request();
            }
        });
    }

    // Esc 关闭。监听挂在 window 上：焦点可能在搜索框里，也可能在卡片按钮上。
    let esc_key = window_event_listener(leptos::ev::keydown, move |ev| {
        if open.get_untracked() && ev.key() == "Escape" {
            close(open, on_close);
        }
    });
    on_cleanup(move || esc_key.remove());

    let submit_search = move || {
        let kw = state.get_untracked().keyword.clone();
        state.update(|s| s.applied = kw);
        request();
    };

    let pick_filter = move |f: Filter| {
        state.update(|s| {
            if s.filter == f {
                return;
            }
            s.filter = f;
        });
        request();
    };

    let reset_search = move || {
        state.update(|s| {
            s.keyword.clear();
            s.applied.clear();
            s.filter = Filter::All;
        });
        if let Some(el) = input_el.get_untracked() {
            el.set_value("");
        }
        request();
    };

    let on_entry = move |_| {
        if let Some(cb) = on_open {
            cb.run(());
        }
        open.set(true);
    };

    let remove_selected = move |_| {
        let ids = state.get_untracked().selected_visible();
        if ids.is_empty() {
            return;
        }
        remove.run(ids);
    };

    let select_all = move |_| {
        state.update(|s| s.toggle_select_all());
    };

    let show_entry = show_entry.unwrap_or(true);

    view! {
        <Show when=move || show_entry>
            <button
                class="btn btn-sm"
                data-testid="sensitive-open"
                title="打开敏感库：已抓过的岗位（已入库 + 被排除）"
                on:click=on_entry
            >
                {move || title_text(state.get().total)}
            </button>
        </Show>

        <Show when=move || open.get()>
            {
                let backdrop = view! {
                    <div
                        class="sensitive-backdrop"
                        data-testid="sensitive-backdrop"
                        on:click=move |_| close(open, on_close)
                    ></div>
                };
                let panel = view! {
                    <div
                        class=move || {
                            // 右侧滑入：收起态由 `translateX(100%)` 承担，`is-open` 才滑到 0。
                            if open.get() { "sensitive-panel is-open" } else { "sensitive-panel" }
                        }
                        data-testid="sensitive-panel"
                        role="dialog"
                        aria-modal="true"
                    >
                        <header class="sensitive-head">
                            <div class="sensitive-head-text">
                                <h2 class="sensitive-title" data-testid="sensitive-title">
                                    {move || title_text(state.get().total)}
                                </h2>
                                <p class="sensitive-sub">
                                    "已抓过的岗位（已入库 + 被排除）都会记在这里，下一轮抓取直接跳过；可手动移出"
                                </p>
                            </div>
                            <button
                                class="btn btn-sm btn-ghost"
                                data-testid="sensitive-close"
                                on:click=move |_| close(open, on_close)
                            >
                                "关闭"
                            </button>
                        </header>

                        <div class="sensitive-tools">
                            <input
                                node_ref=input_el
                                class="input sensitive-search"
                                data-testid="sensitive-search"
                                placeholder="搜索公司 / 岗位 / 排除原因"
                                prop:value=move || state.get().keyword
                                on:input=move |ev| {
                                    let v = event_target_value(&ev);
                                    state.update(|s| s.keyword = v);
                                }
                                on:keydown=move |ev| {
                                    if ev.key() == "Enter" {
                                        submit_search();
                                    }
                                }
                            />
                            <button class="btn btn-sm" data-testid="sensitive-search-btn" on:click=move |_| submit_search()>
                                "搜索"
                            </button>
                            <button class="btn btn-sm btn-ghost" data-testid="sensitive-reset" on:click=move |_| reset_search()>
                                "重置"
                            </button>
                            <span class="spacer"></span>
                            <For
                                each=|| [Filter::All, Filter::Stored, Filter::Rejected]
                                key=|f| f.slug()
                                children=move |f: Filter| {
                                    let testid = format!("sensitive-filter-{}", f.slug());
                                    view! {
                                        <button
                                            class="chip as-btn sensitive-filter"
                                            class:is-on=move || state.get().filter == f
                                            data-testid=testid
                                            on:click=move |_| pick_filter(f)
                                        >
                                            {f.label()}
                                        </button>
                                    }
                                }
                            />
                        </div>

                        <div class="sensitive-tools sensitive-tools-actions">
                            <button class="chip as-btn" data-testid="sensitive-select-all" on:click=select_all>
                                {move || if state.get().all_selected() { "取消全选" } else { "全选" }}
                            </button>
                            <button
                                class="btn btn-sm btn-danger"
                                data-testid="sensitive-remove-selected"
                                prop:disabled=move || !state.get().selected_visible().len().gt(&0)
                                on:click=remove_selected
                            >
                                {move || remove_selected_label(state.get().selected_visible().len())}
                            </button>
                            <span class="spacer"></span>
                            <Show when=move || {
                                // 两个条件都要读到信号；`&&` 短路会让 `total` 依赖丢失。
                                let loading = state.get().loading;
                                let overflow = state.get().total > PAGE_LIMIT as i64;
                                !loading && overflow
                            }>
                                <span class="sensitive-hint dim">
                                    {move || format!("显示前 {PAGE_LIMIT} 条，用搜索缩小范围")}
                                </span>
                            </Show>
                        </div>

                        {move || {
                            let err = state.get().error;
                            (!err.is_empty()).then(|| view! {
                                <div class="sensitive-error">
                                    <ErrorBar message=err test_id="sensitive-error" />
                                </div>
                            })
                        }}

                        <div class="sensitive-body">
                            <Show
                                when=move || !state.get().loading
                                fallback=|| view! { <div class="sensitive-loading dim">"加载中…"</div> }
                            >
                                <Show
                                    when=move || !state.get().items.is_empty()
                                    fallback=|| view! {
                                        <EmptyState test_id="sensitive-empty">"（暂无记录）"</EmptyState>
                                    }
                                >
                                    <SensitiveList
                                        items=Signal::derive(move || state.get().items)
                                        selected=Signal::derive(move || state.get().selected_visible())
                                        on_toggle=Callback::new(move |id: i64| {
                                            state.update(|s| {
                                                if !s.selected.remove(&id) {
                                                    s.selected.insert(id);
                                                }
                                            });
                                        })
                                        on_remove=Callback::new(move |id: i64| remove.run(vec![id]))
                                    />
                                </Show>
                            </Show>
                        </div>
                    </div>
                };
                view! { {backdrop} {panel} }
            }
        </Show>
    }
}

fn close(open: RwSignal<bool>, on_close: Option<Callback<()>>) {
    open.set(false);
    if let Some(cb) = on_close {
        cb.run(());
    }
}

async fn reload_now(state: RwSignal<SensitiveState>) {
    // 查询参数是 `&str` 借用，必须先把状态整体取到一个具名变量里，
    // 否则临时值在本语句末尾就被释放，借用活不到 `await`。
    let snapshot = state.get_untracked();
    let (platform, keyword, kind) = snapshot.query();
    match api::sensitive_list(platform, keyword, kind, Some(PAGE_LIMIT), Some(0)).await {
        Ok(page) => state.update(|s| {
            s.items = page.items;
            s.total = page.total;
            s.retain_selection();
        }),
        Err(e) => state.update(|s| s.error = format!("刷新敏感库失败：{e}")),
    }
}
