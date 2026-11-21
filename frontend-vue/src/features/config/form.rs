//! 配置分组（与旧版 `ConfigPanel.tsx` 的 `Section` 同构）：
//! 一整行可点的标题 + `.chevron` + 展开体。
//!
//! 不复用 `components::Collapsible`：那个组件的标题是 `.section-label` + `.caret`，
//! 旧版配置页用的是 `.config-head` + `.config-title` + `.chevron`——类名不同，
//! 样式与视觉层级也不同。沿用旧结构才不会「看起来像另一个界面」。

use leptos::prelude::*;

#[component]
pub fn ConfigSection(
    title: String,
    #[prop(optional, into)] subtitle: Option<String>,
    open: RwSignal<bool>,
    #[prop(optional, into)] test_id: Option<String>,
    children: ChildrenFn,
) -> impl IntoView {
    let body = StoredValue::new(children);
    let sub = subtitle.unwrap_or_default();
    let has_sub = !sub.is_empty();
    view! {
        <div class="config-card" data-testid=test_id>
            <button
                type="button"
                class="config-head"
                aria-expanded=move || if open.get() { "true" } else { "false" }
                on:click=move |_| open.update(|v| *v = !*v)
            >
                <span class=move || if open.get() { "chevron is-open" } else { "chevron" }></span>
                <span class="config-title">{title}</span>
                {has_sub.then(|| view! { <span class="card-sub">{sub.clone()}</span> })}
            </button>
            <Show when=move || open.get()>
                <div class="config-body">{move || body.with_value(|f| f())}</div>
            </Show>
        </div>
    }
}
