//! 页面：概览 / Boss 消息 / 配置 / 决策记录。
//!
//! 页面层负责把 feature 组装成一个可用的页面，**不写业务规则**。

pub mod chat;
pub mod config;
pub mod dashboard;
pub mod logs;

use leptos::prelude::*;

use crate::lib_ui::nav::Nav;

/// 按 nav 渲染一个页面。`page_tick` 变化时该页重拉数据。
#[component]
pub fn Page(
    nav: RwSignal<Nav>,
    page_tick: RwSignal<u32>,
    on_notice: Callback<String>,
) -> impl IntoView {
    view! {
        <Show when=move || nav.get() == Nav::Dashboard>
            <dashboard::DashboardPage on_notice=on_notice />
        </Show>
        <Show when=move || nav.get() == Nav::Chat>
            <chat::ChatPage page_tick=page_tick on_notice=on_notice />
        </Show>
        <Show when=move || nav.get() == Nav::Config>
            <config::ConfigPage on_notice=on_notice />
        </Show>
        <Show when=move || nav.get() == Nav::Logs>
            <logs::LogsPage page_tick=page_tick on_notice=on_notice />
        </Show>
    }
}
