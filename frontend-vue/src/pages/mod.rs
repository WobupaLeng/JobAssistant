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

/// 四个页面里，「概览」是刻意不接 `page_tick` 的那一个：它自己带 5 秒计数轮询
/// 与首帧 `spec` 驱动，再叠一条「切回本页就整表重拉」会让用户从别的页切回来时
/// 丢掉滚动位置与勾选。
fn page_takes_tick(nav: Nav) -> bool {
    match nav {
        Nav::Dashboard => false,
        Nav::Chat | Nav::Config | Nav::Logs => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib_ui::nav::NAV;

    /// 每个 nav 都得有对应页面被分发到，否则切过去是一片空白。
    #[test]
    fn every_nav_has_a_distinct_slug() {
        let mut slugs: Vec<&str> = NAV.iter().map(|i| i.key.slug()).collect();
        let n = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), n, "两个页面的 slug 重名，data-testid 会互相顶掉");
        assert_eq!(n, 4);
    }

    /// 概览不接 `page_tick`，其余三页接。这个划分是「切回本页要不要整表重拉」
    /// 的唯一判据，写反会让概览页每次切回来都丢勾选，或者让日志页不再自动刷新。
    #[test]
    fn only_the_overview_page_ignores_the_page_tick() {
        assert!(!page_takes_tick(Nav::Dashboard));
        for n in [Nav::Chat, Nav::Config, Nav::Logs] {
            assert!(page_takes_tick(n), "{} 页需要 page_tick", n.slug());
        }
    }

    /// 侧边栏与页面分发共用同一份 `NAV`：顺序换了会改变默认落地页的直觉。
    #[test]
    fn sidebar_order_starts_with_dashboard() {
        assert_eq!(NAV[0].key, Nav::Dashboard);
        assert_eq!(NAV[0].name, "概览");
        // 四个页面一个都不能漏，且顺序即渲染顺序
        let keys: Vec<Nav> = NAV.iter().map(|i| i.key).collect();
        assert_eq!(keys, [Nav::Dashboard, Nav::Chat, Nav::Config, Nav::Logs]);
    }

    /// 带副作用的页面（`enters_side_effect`）与接收 `page_tick` 的页面
    /// 必须是同一批。两处判据不一致时，要么有页面切回来不刷新，
    /// 要么有页面被重复刷新两次。
    #[test]
    fn side_effect_pages_are_exactly_the_tick_pages() {
        for item in NAV {
            assert_eq!(
                item.key.enters_side_effect(),
                page_takes_tick(item.key),
                "{} 页的两处判据不一致",
                item.key.slug()
            );
        }
    }
}
