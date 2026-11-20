//! 导航：四个页面 + 进入时的副作用表。
//!
//! 与 TS 版 `lib/nav.ts` 一致。`NAV_ENTER` 决定「切到这个页面要不要重拉数据」，
//! 放在这里而不是各页面里，是为了让「哪些页需要新鲜数据」一眼可见。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Dashboard,
    Chat,
    Config,
    Logs,
}

impl Nav {
    /// 用于 `data-testid="nav-<slug>"` 与 `page-<slug>`。**E2E 依赖这两个串**。
    pub fn slug(self) -> &'static str {
        match self {
            Nav::Dashboard => "dashboard",
            Nav::Chat => "chat",
            Nav::Config => "config",
            Nav::Logs => "logs",
        }
    }

    /// 进入该页是否触发重拉 / 副作用。
    pub fn enters_side_effect(self) -> bool {
        !matches!(self, Nav::Dashboard)
    }
}

pub struct NavItem {
    pub key: Nav,
    pub name: &'static str,
}

pub const NAV: &[NavItem] = &[
    NavItem { key: Nav::Dashboard, name: "概览" },
    NavItem { key: Nav::Chat, name: "Boss 消息" },
    NavItem { key: Nav::Config, name: "配置" },
    NavItem { key: Nav::Logs, name: "决策记录" },
];
