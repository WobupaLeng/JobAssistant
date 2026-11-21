//! 应用外壳：顶栏 + 错误条 + 提示条 + 页面插槽。
//!
//! 与 TS 版 `App.tsx` 结构一一对应。**本文件不包含任何业务动作**。
//!
//! **不包含页面分发**：`components` 不得依赖 `pages`（`tools/check_layers.py` R2）。
//! 页面树由调用方经 `dispatch` 插槽传入，见 [`AppShell`]。

use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;

use crate::api::{self, types::*};
use crate::components::widgets::{Banner, Chip, ErrorBar};
use crate::lib_ui::nav::{Nav, NAV, NAV_BLOCK_CLASS};

/// 错误条存活时长：10 秒后自动销毁。
///
/// 与提示条（4 秒）分开：错误要留够时间读完，但不能一直挂在页面顶部吃掉
/// 一条高度——用户看完即走，真有持久故障由下一次操作再次触发。
const ERROR_AUTO_DISMISS_MS: u32 = 10_000;

/// 全局引导状态：三份一次性数据 + 错误/提示。
///
/// 用 `RwSignal` 打包成一个结构而不是拆成六个独立 signal：它们在
/// `useBootstrap` 里总是一起加载、一起失败，拆开只会制造「部分加载」的假状态。
#[derive(Clone)]
pub struct Boot {
    pub info: RwSignal<Option<AppInfo>>,
    pub spec: RwSignal<Option<ScoringSpec>>,
    pub llm: RwSignal<Option<LlmStatus>>,
    pub error: RwSignal<Option<String>>,
}

impl Boot {
    pub fn new() -> Self {
        Self {
            info: RwSignal::new(None),
            spec: RwSignal::new(None),
            llm: RwSignal::new(None),
            error: RwSignal::new(None),
        }
    }

    pub fn fail(&self, e: impl ToString) {
        self.error.set(Some(e.to_string()));
    }

    /// 重新拉大模型状态。保存配置后要调它，否则顶栏还挂着「未配置」。
    pub fn refresh_llm(self) {
        spawn_local(async move {
            if let Ok(v) = api::llm_status().await {
                self.llm.set(Some(v));
            }
        });
    }
}

#[component]
pub fn AppShell(
    nav: RwSignal<Nav>,
    page_tick: RwSignal<u32>,
    dispatch: ViewFn,
    /// 顶栏导航区末尾的附加插槽（敏感库入口）。
    ///
    /// 外壳不自己造这个组件：它住在 `features/`，而 `components/` 不得依赖
    /// `features/`（`tools/check_layers.py` R2）。由 `lib.rs::App` 组装后传入。
    #[prop(optional)]
    nav_extra: Option<ViewFn>,
) -> impl IntoView {
    let boot = Boot::new();
    provide_context(boot.clone());

    // 页面要读 `nav` / `page_tick` / 提示通道，但页面树在 `dispatch` 里、
    // 由组装层（`lib.rs`）持有，所以走 context 而不是 props。
    provide_context(nav);
    provide_context(page_tick);

    let notice = RwSignal::new(String::new());
    provide_context(Callback::new(move |m: String| notice.set(m)));
    let (theme, set_theme) = crate::lib_ui::theme::use_theme();

    // 一次性加载：任一失败都进错误条，但不阻断界面渲染。
    // 这里用克隆而不是搬走 boot：下面的 build_tag/spec_text/llm_chip 三个闭包
    // 还要继续读它。Boot 内部全是 RwSignal（Copy/Clone 共享同一份状态），
    // 克隆等于多一个句柄，不会各读各的。
    let boot_load = boot.clone();
    spawn_local(async move {
        let boot = boot_load;
        match api::app_info().await {
            Ok(v) => boot.info.set(Some(v)),
            Err(e) => boot.fail(e),
        }
        match api::scoring_spec().await {
            Ok(v) => boot.spec.set(Some(v)),
            Err(e) => boot.fail(e),
        }
        match api::llm_status().await {
            Ok(v) => boot.llm.set(Some(v)),
            Err(e) => boot.fail(e),
        }
    });

    // 关窗收尾：只 stop 不关 stdin 会让 Python 主循环残留，进程退不干净
    spawn_local(async move {
        let _ = api::scrape_close().await;
    });

    // 提示 4 秒后自动消失
    Effect::new(move |_| {
        let msg = notice.get();
        if msg.is_empty() {
            return;
        }
        set_timeout(
            move || notice.set(String::new()),
            std::time::Duration::from_secs(4),
        );
    });

    // 错误条 10 秒后自动销毁（用户：图 1 那种错误信息）。
    // 每次错误出现时重新计时；`on_cleanup` 撤掉上一个计时器，
    // 否则「报错 → 点忽略 → 再报错」会让前一个计时器提前清掉新的错误。
    // 计时器句柄放在 `Arc<AtomicI32>` 里：`on_cleanup` 的回调要求 `Send + Sync`，
    // 而 `RwSignal` 不能直接住进那个闭包；句柄本身是 `Copy` 的 `i32`，
    // 用原子量让「效果回调写句柄」与「清理回调读句柄」共享同一个槽位。
    let error_timer: Arc<AtomicI32> = Arc::new(AtomicI32::new(0));
    let error_timer_effect = error_timer.clone();
    let boot_dismiss = boot.clone();
    Effect::new(move |_| {
        if boot_dismiss.error.get().is_none() {
            return;
        }
        let boot2 = boot_dismiss.clone();
        let cb = Closure::once_into_js(move || boot2.error.set(None));
        if let Some(w) = web_sys::window() {
            let prev = error_timer_effect.swap(0, Ordering::Relaxed);
            if prev != 0 {
                w.clear_timeout_with_handle(prev);
            }
            if let Ok(h) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                ERROR_AUTO_DISMISS_MS as i32,
            ) {
                error_timer_effect.store(h, Ordering::Relaxed);
            }
        }
    });

    let error_timer_cleanup = error_timer.clone();
    on_cleanup(move || {
        let h = error_timer_cleanup.swap(0, Ordering::Relaxed);
        if h != 0 {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(h);
            }
        }
    });

    let go = move |k: Nav| {
        nav.set(k);
        if k.enters_side_effect() {
            page_tick.update(|n| *n += 1);
        }
        if k == Nav::Chat {
            // 顺手把浏览器带到聊天页。sidecar 可能早在别处起来了，但停在别的页面。
            spawn_local(async move {
                let _ = api::session_open("boss", Some("chat")).await;
            });
        }
    };

    let build_tag = move || {
        boot.info
            .get()
            .map(|i| format!("{} · v{} · {}", i.shell, i.version, i.stage))
            .unwrap_or_else(|| "连接中…".to_string())
    };

    let spec_text = move || {
        boot.spec
            .get()
            .map(|s| format!("满分 {} / 默认门槛 {}", s.score_max, s.score_min_default))
            .unwrap_or_else(|| "满分 … / 默认门槛 …".to_string())
    };

    let llm_chip = move || {
        let configured = boot.llm.get().map(|l| l.configured).unwrap_or(false);
        let model = boot.llm.get().map(|l| l.model).unwrap_or_default();
        let tone = if configured { "primary" } else { "default" };
        let text = if configured {
            format!("大模型 {}", if model.is_empty() { "已配置".to_string() } else { model })
        } else {
            "大模型未配置".to_string()
        };
        view! { <Chip tone=tone.to_string()>{text}</Chip> }
    };

    // 概览页启用「投递列表撑满剩余空间」：该页只有一份列表，其余面板都可折叠/定高，
    // 所以让列表吃掉页面剩下的高度、自己内部滚动（样式见 layout.css `.is-fill-page`）。
    let page_cls = move || {
        if nav.get() == Nav::Dashboard {
            "dashboard is-fill-page".to_string()
        } else {
            "dashboard".to_string()
        }
    };

    // 页面树由调用方传进来：`nav` / `page_tick` 是外壳自己的状态，
    // 所以这里先把它们绑进 `dispatch` 再演进 `view!`——`Callback` 克隆即用。
    let dispatch = dispatch.run();

    view! {
        <div class="app-shell" data-testid="app-root">
            <header class="topbar">
                <div class="brand">
                    <span class="brand-mark">"职"</span>
                    "AI 求职助手"
                </div>
                <span class="build-tag" data-testid="app-build">{build_tag}</span>
                // 导航页签与敏感库入口包成一块：入口要紧挨「决策记录」，
                // 中间不能有 `.spacer`。`.nav` 是 `display: contents`，
                // 子项仍是 `.topbar` 的直接 flex 项，排布一点没变。
                <div class=NAV_BLOCK_CLASS>
                    <nav class="tabs nav-tabs" role="tablist" aria-label="主导航">
                        {NAV
                            .iter()
                            .map(|t| {
                                let key = t.key;
                                let name = t.name;
                                view! {
                                    <button
                                        role="tab"
                                        class=move || {
                                            if nav.get() == key { "tab is-active" } else { "tab" }
                                        }
                                        aria-selected=move || {
                                            if nav.get() == key { "true" } else { "false" }
                                        }
                                        data-testid=format!("nav-{}", key.slug())
                                        on:click=move |_| go(key)
                                    >
                                        {name}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </nav>
                    // 顶栏附加插槽（敏感库入口由 `lib.rs::App` 组装后传入）。
                    {nav_extra.map(|f| f.run())}
                </div>
                <span class="spacer"></span>
                <span class="card-sub" data-testid="score-spec">{spec_text}</span>
                {llm_chip}
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="theme-toggle"
                    title="切换明暗主题"
                    on:click=move |_| set_theme.set(theme.get().flip())
                >
                    {move || if theme.get().is_dark() { "☾ 深色" } else { "☀ 浅色" }}
                </button>
            </header>

            <div class=page_cls data-testid=move || format!("page-{}", nav.get().slug())>
                {move || {
                    boot.error
                        .get()
                        .map(|m| {
                            let boot2 = boot.clone();
                            view! {
                                <ErrorBar
                                    message=m
                                    on_dismiss=Callback::new(move |_| boot2.error.set(None))
                                    test_id="app-error".to_string()
                                />
                            }
                        })
                }}
                {move || {
                    let m = notice.get();
                    (!m.is_empty()).then(|| view! { <Banner test_id="app-notice".to_string()>{m}</Banner> })
                }}

                {dispatch}
            </div>
        </div>
    }
}

/// 页面分发：由 `pages::Page` 负责，`components` 不得反向依赖 `pages`。
///
/// 之前页面树直接写在 `AppShell` 的 `view!` 里，`tools/check_layers.py`
/// 因此报「components/ 不得依赖 pages/」。改由调用方（`lib.rs` 的 `App`）
/// 把页面树作为 `dispatch` 传进来：外壳只负责顶栏 + 插槽，谁渲染哪一页由上层决定。
///
/// 延迟执行（无 tokio，靠 js 的 setTimeout）。
///
/// 放在这儿而不是 `lib_ui`：只有外壳的「提示自动消失」需要它。
fn set_timeout<F: FnOnce() + 'static>(f: F, after: std::time::Duration) {
    use wasm_bindgen::prelude::*;
    let cb = Closure::once_into_js(f);
    web_sys::window()
        .expect("window")
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.unchecked_ref(),
            after.as_millis() as i32,
        )
        .ok();
}
