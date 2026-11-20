//! 通用展示组件（设计系统层）。
//!
//! 铁律：**这里只放纯展示**，不得出现任何业务判定、`invoke` 调用或状态管理。
//! 一旦某个组件开始知道「什么是门槛」「什么是抓取」，它就该移到 `features/` 下。
//!
//! 与 TS 版的差别只在 props 形状：React 的 `children`/`ReactNode` 在 Leptos 里
//! 是 `Children`/`AnyView`；可选插槽用 `Option<Children>`（`ChildrenFn` 才能被
//! 多次调用，这里都只渲染一次，故用 `Children` 即可）。

use leptos::prelude::*;

/// 毛玻璃卡片，带可选标题栏。
#[component]
pub fn Card(
    #[prop(optional, into)] title: Option<String>,
    #[prop(optional, into)] title_test_id: Option<String>,
    /// 副标题。`Signal<String>` 可吃裸字符串，也可吃派生信号（抓取状态要实时变）。
    #[prop(optional, into)] subtitle: Option<Signal<String>>,
    #[prop(optional)] actions: Option<Children>,
    #[prop(optional, into)] test_id: Option<String>,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let has_head = title.is_some() || actions.is_some();
    let cls = format!("card {}", class.unwrap_or_default());
    view! {
        <section class=cls data-testid=test_id>
            {has_head.then(|| view! {
                <header class="card-head">
                    {title.map(|t| view! { <h2 class="card-title" data-testid=title_test_id.clone()>{t}</h2> })}
                    {subtitle.map(|s| view! { <span class="card-sub">{move || s.get()}</span> })}
                    <span class="spacer"></span>
                    {actions.map(|a| a())}
                </header>
            })}
            <div class="card-body">{children()}</div>
        </section>
    }
}

/// 统计胶囊：抓取进度的「已抓 / 入库 / 重复 / 低于门槛 / 否决」。
#[component]
pub fn Stat(
    /// 标签：纯文本。刻意不做响应式——统计胶囊的标签是固定的中文词，
    /// 需要变化的是数值。
    #[prop(into)]
    label: String,
    /// 数值：吃一个 `Signal<String>`，由调用方把计数映射成字符串。
    ///
    /// 为什么不用 `MaybeProp<AnyView>`：`AnyView` 内含 `NonNull<()>`，不满足
    /// `Sync`，而 `#[component]` 的 props 要求 `Sync`（Leptos 0.8 的 boxed
    /// 视图可以在多线程调度里传递）。用信号装字符串既保持响应式，又满足约束。
    value: Signal<String>,
    #[prop(optional, into)] tone: Option<String>,
    #[prop(optional, into)] test_id: Option<String>,
) -> impl IntoView {
    let cls = match tone.as_deref() {
        Some(t) => format!("stat-value is-{t}"),
        None => "stat-value".to_string(),
    };
    view! {
        <div class="stat" data-testid=test_id>
            <div class="stat-label">{label}</div>
            <div class=cls>{move || value.get()}</div>
        </div>
    }
}

/// 建一个 `(RwSignal<String>, ReadSignal<String>)` 对，专供 `Stat::value`。
///
/// 存在理由：`Stat` 收 `ReadSignal<String>`，而多数计数来自 `RwSignal<usize>`
/// 或一个闭包。Leptos 的 `.map()` 返回的是派生信号，写起来要两行，
/// 这里收成一处，避免每个调用点重复。
pub fn stat_signal(source: impl Fn() -> usize + Send + Sync + 'static)
    -> (ReadSignal<String>, WriteSignal<String>)
{
    let (sig, set) = signal(String::new());
    Effect::new(move |_| set.set(source().to_string()));
    (sig, set)
}

/// 标签胶囊。`tone` 取 `primary` / `accent` / `warn` / `bad`，其余按默认样式。
#[component]
pub fn Chip(
    #[prop(optional, into)] tone: Option<String>,
    #[prop(optional, into)] title: Option<String>,
    children: Children,
) -> impl IntoView {
    let cls = match tone.as_deref() {
        Some(t) if !t.is_empty() && t != "default" => format!("chip chip-{t}"),
        _ => "chip".to_string(),
    };
    view! { <span class=cls title=title>{children()}</span> }
}

/// 运行状态点 + 文案。`state` 取 `running` / `idle` / `warn` / `bad`。
#[component]
pub fn StatusDot(state: String, label: String) -> impl IntoView {
    view! {
        <span class="row" style="gap:6px;font-size:12.5px;color:var(--text-2)">
            <span class=format!("dot is-{state}")></span>
            {label}
        </span>
    }
}

/// 列表加载中的骨架行，避免「先闪空态再出数据」。
///
/// 宽度序列 `[36, 62, 44, 30, 24]%` 按列索引循环，与 TS 版逐字一致——
/// 换了数列宽就不同，看起来像另一个界面。
#[component]
pub fn SkeletonRows(
    cols: usize,
    #[prop(default = 6)] rows: usize,
) -> impl IntoView {
    const WIDTHS: [u32; 5] = [36, 62, 44, 30, 24];
    (0..rows)
        .map(|_| {
            view! {
                <tr class="skeleton-row">
                    {(0..cols)
                        .map(|c| {
                            let w = WIDTHS[c % 5];
                            view! {
                                <td>
                                    <div class="skeleton-bar" style=format!("width:{w}%")></div>
                                </td>
                            }
                        })
                        .collect_view()}
                </tr>
            }
        })
        .collect_view()
}

/// 统计胶囊的骨架形态：形状与 [`Stat`] 同构（同 padding、同行高），
/// 只是把数字换成一条扫光的短条。
///
/// 为什么单独做一个而不是复用 `Stat` 再塞空串：空串会让胶囊高度塌陷，
/// 数据到达时整排胶囊「跳一下」——这正是用户点名要消除的首屏抖动。
#[component]
pub fn StatSkeleton(#[prop(optional, into)] test_id: Option<String>) -> impl IntoView {
    view! {
        <div class="stat stat-skeleton" data-testid=test_id aria-busy="true">
            <div class="stat-label">
                <div class="skeleton-bar" style="width:48%"></div>
            </div>
            <div class="stat-value">
                <div class="skeleton-bar" style="width:64%;height:18px"></div>
            </div>
        </div>
    }
}

/// 错误条：一行说明 + 「忽略」。`on_dismiss` 为空则不给按钮。
#[component]
pub fn ErrorBar(
    message: String,
    #[prop(optional, into)] on_dismiss: Option<Callback<()>>,
    #[prop(optional, into)] test_id: Option<String>,
) -> impl IntoView {
    view! {
        <div class="error-bar" data-testid=test_id>
            <b style="flex-shrink:0">"错误"</b>
            <span style="flex:1;word-break:break-word">{message}</span>
            {on_dismiss.map(|cb| view! {
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="app-error-dismiss"
                    on:click=move |_| cb.run(())
                >
                    "忽略"
                </button>
            })}
        </div>
    }
}

/// 提示横幅：比错误条温和，4 秒后自行消失（由调用方控制）。
#[component]
pub fn Banner(
    #[prop(optional)] action: Option<Children>,
    #[prop(optional, into)] test_id: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="banner" data-testid=test_id>
            <span style="flex:1">{children()}</span>
            {action.map(|a| a())}
        </div>
    }
}

/// 表单字段：标签 + 输入。`hint` 是标签右侧的灰字说明。
#[component]
pub fn Field(
    #[prop(into)] label: String,
    #[prop(optional, into)] hint: Option<String>,
    #[prop(optional, into)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let cls = format!("field {}", class.unwrap_or_default());
    view! {
        <div class=cls>
            <div class="label">
                {label}
                {hint.map(|h| view! { <span class="field-hint">{h}</span> })}
            </div>
            {children()}
        </div>
    }
}

/// 折叠分组：标题整行可点（`.section-label` 的样式依赖这个结构）。
#[component]
pub fn Collapsible(
    #[prop(into)] title: String,
    /// 副标题。`Signal<String>` 可吃裸字符串，也可吃派生信号（抓取状态要实时变）。
    #[prop(optional, into)] subtitle: Option<Signal<String>>,
    open: ReadSignal<bool>,
    on_toggle: Callback<()>,
    #[prop(optional, into)] test_id: Option<String>,
    // `ChildrenFn` 而不是 `Children`：折叠体要进 `<Show>` 的条件渲染闭包，
    // 那个闭包必须是 `Sync`，而 `Children`（`FnOnce`）不满足，
    // 会报 `dyn FnOnce() -> AnyView + Send cannot be shared between threads safely`。
    children: ChildrenFn,
) -> impl IntoView {
    let children = StoredValue::new(children);
    let body = move || children.with_value(|f| f());
    let caret = move || if open.get() { "caret is-open" } else { "caret" };
    view! {
        <div class="config-group" data-testid=test_id>
            <button
                class="section-label"
                type="button"
                aria-expanded=move || if open.get() { "true" } else { "false" }
                on:click=move |_| on_toggle.run(())
            >
                <span class=caret></span>
                {title}
                {subtitle.map(|s| view! { <span class="card-sub">{s}</span> })}
            </button>
            <Show when=move || open.get()>
                <div class="config-body">{body()}</div>
            </Show>
        </div>
    }
}

/// 空态：一行说明，可选一个动作按钮。
#[component]
pub fn EmptyState(
    #[prop(optional)] action: Option<Children>,
    #[prop(optional, into)] test_id: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="empty-state" data-testid=test_id>
            <span>{children()}</span>
            {action.map(|a| a())}
        </div>
    }
}
