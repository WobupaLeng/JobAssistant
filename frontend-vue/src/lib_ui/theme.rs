//! 主题：深色为默认，`html[data-theme='light']` 覆盖设计令牌。
//!
//! 初始主题的取值顺序（已保存 > 系统偏好 > 深色）曾经是 `App.tsx` 里的一段
//! 内联逻辑，并在预览环境里出过「永远锁死深色」的缺陷——所以这里单独成文件，
//! 且 `prefers-color-scheme` 分支可以独立检视。

use leptos::prelude::*;

pub const THEME_KEY: &str = "aja.theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    pub fn is_dark(self) -> bool {
        self == Theme::Dark
    }

    pub fn flip(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// 初始主题：已保存的选择 > 系统偏好 > 深色（毛玻璃在深色下观感最好）。
pub fn read_initial_theme() -> Theme {
    if let Some(s) = storage() {
        if let Ok(Some(v)) = s.get_item(THEME_KEY) {
            return match v.as_str() {
                "light" => Theme::Light,
                _ => Theme::Dark,
            };
        }
    }
    // WebView 禁用 storage 时退回系统偏好
    let prefers_light = web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: light)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false);
    if prefers_light {
        Theme::Light
    } else {
        Theme::Dark
    }
}

/// 把主题写到根元素并持久化。写失败不影响渲染（WebView 可能禁用 storage）。
pub fn apply_theme(theme: Theme) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = doc.document_element() {
            let _ = root.set_attribute("data-theme", theme.as_str());
        }
    }
    if let Some(s) = storage() {
        let _ = s.set_item(THEME_KEY, theme.as_str());
    }
}

/// 主题 signal：初值走 `read_initial_theme`，变化时自动写回 DOM 与 storage。
pub fn use_theme() -> (ReadSignal<Theme>, WriteSignal<Theme>) {
    let initial = read_initial_theme();
    apply_theme(initial);
    let sig = RwSignal::new(initial);
    Effect::new(move |_| apply_theme(sig.get()));
    (sig.read_only(), sig.write_only())
}
