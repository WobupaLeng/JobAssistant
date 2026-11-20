//! 敏感库抽屉的 **dispatchable 变体**：没有 props，入口浮层一应俱在。
//!
//! `SensitivePanel` 收 `open: RwSignal<bool>`，而 Leptos 的 `view!` 里
//! `<SensitivePanel/>` 不能凭空造出一个 `RwSignal`——自造的信号是游离的，
//! 页面层的按钮点不中它。因此集成时二选一：
//!
//! - 页面层自己持有 `open` 信号：`<SensitivePanel open=open on_open=… on_close=…/>`；
//! - 或者直接用本组件：`<sensitive::SensitiveDrawer/>`（信号在组件内部创建）。
//!
//! 两种都只挂一次，不要同时挂。

use leptos::prelude::*;

use super::panel::SensitivePanel;

#[component]
pub fn SensitiveDrawer() -> impl IntoView {
    let open = RwSignal::new(false);
    view! { <SensitivePanel open=open /> }
}
