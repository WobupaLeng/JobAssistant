//! 求职期望：读 Boss 账号里的期望职位，一键填进抓取关键词。
//!
//! **这里不需要 Chrome cookie 解密**（与旧版的关键差别）：sidecar 本来就开着
//! 已登录的浏览器，`ctx.request.get()` 直接复用会话 cookie。
//!
//! 若 sidecar 没起来，先 `session_open` 自己拉起——不该强迫用户先去抓一次岗位。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::Expectation};
use crate::components::widgets::{Card, Chip, EmptyState};
use crate::lib_ui::session::needs_session;

#[component]
pub fn Expectations(
    /// 追加进抓取关键词。参数是这些期望职位的名称。
    on_pick: Callback<Vec<String>>,
    fail: Callback<String>,
) -> impl IntoView {
    let items = RwSignal::new(Vec::<Expectation>::new());
    let loading = RwSignal::new(false);
    let loaded = RwSignal::new(false);
    let note = RwSignal::new(String::new());

    let load = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        note.set("正在读取…".to_string());
        spawn_local(async move {
            // 第一次可能因为 sidecar 没起而失败；那次失败不该变成红色错误条，
            // 因为「还没启动」是可预期的初始状态，自愈即可。
            let mut result = api::list_expectations().await;
            if let Err(e) = &result {
                if needs_session(e) {
                    // 注意：`needs_session` 收的是 `&str`，命令层把提示拼在返回字符串里，
                    // 所以这里判的是文案而不是错误码（见 lib_ui/session.rs 的说明）。
                    note.set("正在启动浏览器…".to_string());
                    let _ = api::session_open("boss", None).await;
                    result = api::list_expectations().await;
                }
            }
            match result {
                Ok(list) => {
                    items.set(list.clone());
                    loaded.set(true);
                    note.set(if list.is_empty() {
                        "账号里还没有期望职位".to_string()
                    } else {
                        format!("读到 {} 条期望职位", list.len())
                    });
                }
                Err(e) => {
                    note.set(e.clone());
                    fail.run(format!("读取期望职位失败：{e}"));
                }
            }
            loading.set(false);
        });
    };

    let pick = move |_| {
        let names = items
            .get_untracked()
            .into_iter()
            .map(|x| x.name)
            .filter(|n| !n.trim().is_empty())
            .collect::<Vec<_>>();
        if names.is_empty() {
            return;
        }
        on_pick.run(names);
    };

    view! {
        <Card
            test_id="expect-panel"
            title="求职期望"
            subtitle="读账号配置，可一键填进抓取关键词"
            actions=Box::new(move || view! {
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="expect-load-btn"
                    disabled=move || loading.get()
                    on:click=move |_| load()
                >
                    {move || {
                        if loading.get() {
                            "读取中…"
                        } else if loaded.get() {
                            "刷新"
                        } else {
                            "读取"
                        }
                    }}
                </button>
            }.into_any()) as Children
        >
            <Show
                when=move || !items.get().is_empty()
                fallback=move || {
                    view! {
                        <EmptyState test_id="expect-empty">
                            {move || {
                                if note.get().is_empty() {
                                    "点「读取」从 Boss 账号取期望职位。需要浏览器已启动且已登录。"
                                        .to_string()
                                } else {
                                    note.get()
                                }
                            }}
                        </EmptyState>
                    }
                }
            >
                <div class="expect-chips">
                    <For each=move || items.get() key=|x| x.name.clone() let:x>
                        <Chip tone="accent">
                            <span data-testid="expect-item">{x.name.clone()}</span>
                            // `when` 闭包会搬走 `x`，而下面的 `format!` 还要用它——
                            // 先把 city 克隆一份进闭包，`x` 留在原地给渲染用。
                            <Show when={ let city = x.city.clone(); move || !city.is_empty() }>
                                <span class="dim">{format!(" · {}", x.city)}</span>
                            </Show>
                        </Chip>
                    </For>
                </div>
            </Show>

            <div class="row" style="margin-top:10px;gap:10px">
                <button
                    class="btn btn-sm btn-primary"
                    data-testid="expect-apply-btn"
                    disabled=move || items.get().is_empty()
                    on:click=pick
                >
                    "填入抓取关键词"
                </button>
                <span class="card-sub">
                    {move || {
                        if note.get().is_empty() {
                            "会追加到现有关键词里，不覆盖你手填的".to_string()
                        } else {
                            note.get()
                        }
                    }}
                </span>
            </div>
        </Card>
    }
}
