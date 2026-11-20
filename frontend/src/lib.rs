//! AI 求职助手 · Leptos 前端入口。
//!
//! 编译目标为 `wasm32-unknown-unknown`，由 trunk 打包成 `index.html` + `*.wasm`，
//! 再由 `app/src-tauri` 的 `tauri build` 内嵌进 exe。
//!
//! **为什么是 WASM 而不是纯 Rust host 侧界面**：用户要求界面用 Rust 写，
//! 同时要保留毛玻璃（`window-vibrancy` 作用于 WebView 窗口的合成层）。
//! WASM + WebView 是唯一能同时满足两者的组合——纯 eframe/egui 需要接管
//! 主线程消息循环，与 Tauri 冲突（见 `docs/ADR`）。

mod api;
mod components;
mod features;
mod lib_ui;
mod pages;

use leptos::prelude::*;

use crate::lib_ui::nav::Nav;

/// 挂载应用。由 `src/main.rs`（bin target）调用。
///
/// 不叫 `main()` 是刻意的：`lib.rs` 里的 `pub fn main` 和 bin 的 `fn main`
/// 同名时，trunk/cargo 的产物定位容易混淆，而这里本来就不是进程入口。
pub fn start() {
    // 控制台里报错要看得懂：默认 wasm panic 只有一句 "unreachable"
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

/// 应用外壳：顶栏 + 错误条 + 提示条 + 页面分发。
///
/// **不包含任何业务动作**。三条结构性约束（继承自 TS 版 `App.tsx`）：
/// 1. 满分与门槛只能来自 `get_scoring_spec`，这里不出现硬编码分数；
/// 2. 整页背景保持透明，任何不透明底色都会吃掉毛玻璃；
/// 3. 概览是默认页，E2E 依赖的控件都在那一页。
#[component]
fn App() -> impl IntoView {
    // 页面分发放在组装层，外壳只提供插槽：
    // `components/` 不得依赖 `pages/`（`tools/check_layers.py` R2）。
    //
    // `nav` / `page_tick` 在这里建、再由外壳注入 context 收取：顶栏要改它们，
    // 页面要读它们，而 `ViewFn` 是 `'static`，捕获不到外壳内部创建的信号。
    let nav = RwSignal::new(crate::lib_ui::nav::Nav::Dashboard);
    let page_tick = RwSignal::new(0u32);
    let dispatch = ViewFn::from(move || {
        let nav = use_context::<RwSignal<Nav>>().expect("AppShell 必须注入 nav");
        let page_tick = use_context::<RwSignal<u32>>().expect("AppShell 必须注入 page_tick");
        let on_notice = use_context::<Callback<String>>().expect("AppShell 必须注入 on_notice");
        view! { <pages::Page nav=nav page_tick=page_tick on_notice=on_notice /> }
    });
    view! { <components::shell::AppShell nav=nav page_tick=page_tick dispatch=dispatch /> }
}
