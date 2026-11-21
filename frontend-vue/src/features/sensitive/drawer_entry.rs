//! 敏感库抽屉的 **dispatchable 变体**：没有必需 props，入口按钮与浮层一应俱在。
//!
//! `SensitivePanel` 收 `open: RwSignal<bool>`，而 Leptos 的 `view!` 里
//! `<SensitivePanel/>` 不能凭空造出一个 `RwSignal`——自造的信号是游离的，
//! 页面层的按钮点不中它。因此集成时二选一：
//!
//! - 调用方自己持有 `open` 信号：`<SensitivePanel open=open on_open=… on_close=…/>`；
//! - 或者直接用本组件：`<SensitiveDrawer/>`（信号在组件内部创建）。
//!
//! 两种都只挂一次，不要同时挂。
//!
//! ## 为什么拆成「入口按钮」与「浮层」两件
//! 入口按钮要待在顶栏里（用户裁决：紧挨「决策记录」），浮层是 `position: fixed`
//! 的全屏层、逻辑上属于页面树。这两处分处不同层级，所以本模块把 `open` 信号
//! 提到上层（`lib.rs::App`），再分别注入：
//! - `<SensitiveEntry open/>` 放进顶栏（由 `AppShell` 的 `nav_extra` 插槽承载）；
//! - `<SensitiveOverlay open/>` 放进页面树。
//!
//! `SensitiveDrawer` 保留给「按钮和浮层在同一个视图里」的简单场景。

use leptos::prelude::*;

use super::panel::SensitivePanel;

#[component]
pub fn SensitiveDrawer() -> impl IntoView {
    let open = RwSignal::new(false);
    view! { <SensitivePanel open=open /> }
}

/// 只有入口按钮，不含浮层。放进顶栏的 `nav_extra` 插槽。
#[component]
pub fn SensitiveEntry(open: RwSignal<bool>) -> impl IntoView {
    view! { <SensitivePanel open=open show_entry=true /> }
}

/// 只有浮层，不含入口按钮。放进页面树（通常与入口同属一个 `open` 信号）。
///
/// `open` 从上方读：调用方传进来的是 `ReadSignal`，避免浮层反过来改它。
#[component]
pub fn SensitiveOverlay(open: RwSignal<bool>) -> impl IntoView {
    view! { <SensitivePanel open=open show_entry=false /> }
}
