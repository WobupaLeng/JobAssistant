//! 抓取控制（旧版图 3）：可折叠的筛选面板 + 顶条 + 进度 + 事件流。
//!
//! 结构（自上而下，与旧版一致）：
//! 1. **顶条**：「收起筛选 ▲」+ 开始/停止 + 「空闲 · 已入库 N 条」
//!    + 右侧「有效 JD 上限 [ ] 条」「分值门槛 ≥ [ ] 分（低于不入库）」
//! 2. **展开体**（`filter.rs::ScrapeFilterPanel`）：关键词、城市/职位类型/翻页、
//!    薪资/经验/学历/规模/阶段/行业、排除词、底部四组、灰色脚注
//! 3. **进度胶囊 + 事件流**（契约区，`gui_e2e.py` 解析这里的文案）
//!
//! **轮询只在 running 时进行**：`scrape_progress` 每次都走一次 IPC，
//! 空闲时每 800ms 调一次等于让 WebView 白烧 CPU。运行中才起定时器，
//! 停止后立刻撤掉。
//!
//! 事件流用 `scrape_drain` 增量取，不用全量重绘：一次抓取会产出上千行日志，
//! 每帧重建整个列表会让滚动位置不断归零。

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;

use crate::api::{self, types::*};
use crate::components::widgets::{Card, Stat, StatusDot};
use crate::features::auto::AutoBar;
use crate::lib_ui::text::split_list;

use super::filter::ScrapeFilterPanel;

/// 事件流最多保留多少行。超过就丢最旧的——用户看的是最近发生了什么，
/// 无限增长会让 DOM 节点数在长任务里爆掉。
const FEED_MAX: usize = 400;

/// 「有效 JD 上限」的本地默认值。后端没有下发这个值（`ScrapeArgs.max_jd` 是
/// 每次抓取现填的），所以这里给一个保守默认，用户改过就用用户的。
const MAX_JD_DEFAULT: u32 = 100;

/// 页签条数的轮询周期（旧版 5 秒）。
const E5: u32 = 5000;

const LOGIN_LABEL: &[(&str, &str)] = &[
    ("logged_in", "已登录"),
    ("no_login", "未登录"),
    ("unknown", "未知"),
    ("checking", "检查中"),
];

fn login_text(state: &str) -> String {
    LOGIN_LABEL
        .iter()
        .find(|(k, _)| *k == state)
        .map(|(_, v)| (*v).to_string())
        .unwrap_or_else(|| {
            if state.is_empty() {
                "未知".to_string()
            } else {
                state.to_string()
            }
        })
}

/// 事件流里的一行。`tone` 决定颜色，`tag` 是左侧对齐的短标签。
#[derive(Debug, Clone, PartialEq)]
struct FeedLine {
    tag: String,
    msg: String,
    tone: &'static str,
}

fn to_feed_line(u: &ScrapeUpdate) -> FeedLine {
    match u {
        ScrapeUpdate::Job {
            summary,
            accepted,
            score,
        } => {
            if *accepted {
                FeedLine {
                    // `✔` 不是装饰：`tools/gui_e2e.py` 用 feed.count("✔") 统计入库条数。
                    tag: "✔".to_string(),
                    msg: format!("{score} 分 · {summary}"),
                    tone: "ok",
                }
            } else {
                FeedLine {
                    tag: "跳过".to_string(),
                    msg: summary.clone(),
                    tone: "skip",
                }
            }
        }
        ScrapeUpdate::Log { level, msg } => FeedLine {
            tag: level.clone(),
            msg: msg.clone(),
            tone: match level.as_str() {
                "error" => "bad",
                "warn" => "skip",
                "info" => "info",
                _ => "plain",
            },
        },
        ScrapeUpdate::Login { state, msg } => FeedLine {
            tag: "登录".to_string(),
            msg: format!("{} · {msg}", login_text(state)),
            tone: if state == "logged_in" { "ok" } else { "skip" },
        },
        ScrapeUpdate::Malformed { line } => FeedLine {
            tag: "坏帧".to_string(),
            msg: line.clone(),
            tone: "bad",
        },
        ScrapeUpdate::Exited { code } => FeedLine {
            tag: "退出".to_string(),
            msg: format!("sidecar 已退出，退出码 {code}"),
            tone: "bad",
        },
        ScrapeUpdate::Restarted { pid } => FeedLine {
            tag: "重启".to_string(),
            msg: format!("sidecar 已重启，pid={pid}"),
            tone: "info",
        },
    }
}

#[component]
pub fn ScrapePanel(
    /// 关键词原文（逗号分隔）。由页面层持有，`Expectations` 也要改它。
    keywords: RwSignal<String>,
    /// 入库门槛。同样由页面层持有。
    min_score: RwSignal<u8>,
    score_max: Option<u8>,
    min_score_default: Option<u8>,
    on_inserted: Callback<()>,
    /// 自动投递每发出一条后回调（页面层据此刷新列表与敏感库条数）。
    /// 与 `on_inserted` 分开：抓取入库和自动投递是两条不同的刷新触发源。
    #[prop(optional)] on_auto_sent: Option<Callback<()>>,
    notify: Callback<String>,
    fail: Callback<String>,
    /// 概览页把「重新打分」也挂进这条顶栏（原位是已被移除的统计胶囊行）。
    /// 只对概览页传：日志页等其它装配点没有这个面板，`None` 时整块不渲染。
    #[prop(optional)] rescore: Option<ViewFn>,
    /// 「重新打分」用的已入库岗位数（`db_overview.total`）。
    #[prop(optional)] rescore_total: Option<Signal<usize>>,
) -> impl IntoView {
    let progress = RwSignal::new(ScrapeProgress::default());
    let feed = RwSignal::new(Vec::<FeedLine>::new());
    let starting = RwSignal::new(false);
    let max_jd = RwSignal::new(MAX_JD_DEFAULT);
    // 筛选面板默认展开：用户第一次进来要能看见关键词框并直接开始。
    let expanded = RwSignal::new(true);
    let feed_el = NodeRef::<leptos::html::Div>::new();

    // 轮询：只在 running 时跑。用 Effect 跟踪 running，切到 true 才起定时器。
    let running = create_memo(move |_| progress.get().running);
    Effect::new(move |_| {
        if !running.get() {
            return;
        }
        let handle = IntervalHandle::new(move || {
            spawn_local(async move {
                match api::scrape_progress().await {
                    Ok(p) => {
                        let was_running = progress.get_untracked().running;
                        let inserted_before = progress.get_untracked().inserted;
                        progress.set(p.clone());
                        // 入库数变化 → 列表要刷新（新岗位可能已过门槛）
                        if p.inserted != inserted_before {
                            on_inserted.run(());
                        }
                        // 抓取自然结束 → 撤掉定时器
                        if was_running && !p.running {
                            IntervalHandle::stop_current();
                        }
                    }
                    Err(e) => fail.run(format!("读取抓取进度失败：{e}")),
                }
                // 事件流增量取
                match api::scrape_drain(50).await {
                    Ok(items) => {
                        if !items.is_empty() {
                            let mut added = items.iter().map(to_feed_line).collect::<Vec<_>>();
                            feed.update(|v| {
                                v.append(&mut added);
                                let overflow = v.len().saturating_sub(FEED_MAX);
                                if overflow > 0 {
                                    v.drain(0..overflow);
                                }
                            });
                            scroll_feed_bottom(feed_el);
                        }
                    }
                    Err(e) => fail.run(format!("读取抓取事件失败：{e}")),
                }
            });
        });
        // 丢掉句柄时定时器继续跑；靠上面的 stop_current 或 Effect 重入来撤。
        std::mem::forget(handle);
    });

    let on_start = move |_| {
        let kws = split_list(&keywords.get_untracked());
        if kws.is_empty() {
            fail.run("请先填关键词，多个关键词用逗号或顿号分隔".to_string());
            return;
        }
        starting.set(true);
        spawn_local(async move {
            let min = min_score.get_untracked();
            match api::scrape_start("boss", kws, max_jd.get_untracked(), min).await {
                Ok(r) => {
                    notify.run(format!(
                        "已启动抓取进程 pid={} · 关键词 {} · 门槛 {} 分",
                        r.pid,
                        r.keywords.join(" / "),
                        r.min_score
                    ));
                }
                Err(e) => fail.run(format!("启动抓取失败：{e}")),
            }
            starting.set(false);
        });
    };

    let on_stop = move |_| {
        spawn_local(async move {
            match api::scrape_stop().await {
                Ok(()) => notify.run("已请求停止抓取".to_string()),
                Err(e) => fail.run(format!("停止抓取失败：{e}")),
            }
        });
    };

    let on_login = move |_| {
        spawn_local(async move {
            match api::login_start("boss").await {
                Ok(_) => notify.run("已打开登录窗口，请在弹出的浏览器里扫码".to_string()),
                Err(e) => fail.run(format!("启动登录失败：{e}")),
            }
        });
    };

    // ---------- 顶条右侧两个输入：改完就写后端（set_limit） ----------
    //
    // 用 `change` 而不是 `input`：每敲一个数字都 invoke 一次会连打十几次 IPC，
    // 后端还要写库。`change` 在失焦/回车时才触发，正是旧版 onBlur 的时机。
    let on_max_jd = move |ev: leptos::ev::Event| {
        let v = event_target_value(&ev).trim().parse::<u32>().unwrap_or(MAX_JD_DEFAULT);
        let v = v.clamp(1, 9999);
        max_jd.set(v);
        spawn_local(async move {
            match api::set_limit(Some(v), None).await {
                Ok(r) => notify.run(format!("有效 JD 上限已设为 {} 条", r.max_jd)),
                Err(e) => fail.run(format!("保存 JD 上限失败：{e}")),
            }
        });
    };

    let on_min_score = move |ev: leptos::ev::Event| {
        let Some(max) = score_max else {
            fail.run("打分口径还没加载完，稍后再试".to_string());
            return;
        };
        let raw = event_target_value(&ev).trim().parse::<u16>().unwrap_or(0);
        // 门槛必须落在 [0, 满分]：超出范围的值后端会拒，但界面先夹住更好懂
        let v = raw.min(max as u16) as u8;
        min_score.set(v);
        spawn_local(async move {
            match api::set_limit(None, Some(v)).await {
                Ok(r) => notify.run(format!("门槛已设为 {} 分", r.min_score)),
                Err(e) => fail.run(format!("保存门槛失败：{e}")),
            }
        });
    };

    let status_state: Signal<String> = Signal::derive(move || {
        let p = progress.get();
        if p.running {
            "running".to_string()
        } else {
            "idle".to_string()
        }
    });

    let status_label: Signal<String> = Signal::derive(move || {
        let p = progress.get();
        if p.running {
            "抓取中".to_string()
        } else if !p.finished_reason.is_empty() {
            format!("未在抓取 · {}", p.finished_reason)
        } else {
            format!("未在抓取 · 登录态 {}", login_text(&p.login_state))
        }
    });

    // 顶条中间那行状态：`空闲 · 已入库 N 条`（旧版 progress_text）
    let progress_text = move || {
        let p = progress.get();
        let head = if p.running { "抓取中" } else { "空闲" };
        format!(
            "{head} · 已入库 {} 条 · 已抓 {} · 低于门槛 {} · 规则否决 {}",
            p.inserted, p.scraped, p.below_threshold, p.rejected
        )
    };

    // Stat 的 value 收 `ReadSignal<String>`：把计数映射成文本，组件保持纯展示。
    let scraped_text = Signal::derive(move || progress.get().scraped.to_string());
    let inserted_text = Signal::derive(move || progress.get().inserted.to_string());
    let duplicates_text = Signal::derive(move || progress.get().duplicates.to_string());
    let below_threshold_text = Signal::derive(move || progress.get().below_threshold.to_string());
    let rejected_text = Signal::derive(move || progress.get().rejected.to_string());

    let summary_text = move || {
        let p = progress.get();
        format!(
            "已抓 {} · 入库 {} · 重复 {} · 低于门槛 {} · 规则否决 {}",
            p.scraped, p.inserted, p.duplicates, p.below_threshold, p.rejected
        )
    };

    // ---------- 行尾动作区：「重新打分」 ----------
    //
    // 统计胶囊行（岗位总数 / 当前列表 / 已排除）被移除后，那一行右侧的
    // `RescorePanel` 需要一个新家：放在这里，与「已入库 N 条」同一条工具带，
    // 语义上也对得上（重算的对象正是已入库的岗位）。
    let tail = rescore.map(|panel| {
        let total = rescore_total.unwrap_or_else(|| Signal::derive(|| 0usize));
        move || {
            view! {
                <span class="scrape-tool">
                    <span class="scrape-total" data-testid="scrape-total" title="库里已入库的岗位数">
                        "已入库 " <b>{move || total.get().to_string()}</b> " 条"
                    </span>
                    {panel.run()}
                </span>
            }
            .into_any()
        }
    });

    view! {
        <Card
            test_id="scrape-panel"
            title="抓取"
            class="scrape-card"
            subtitle=Signal::derive(move || status_label.get())
            actions=Box::new(move || {
                view! {
                    <span data-testid="scrape-status">
                        <StatusDot state=status_state.get() label=status_label.get() />
                    </span>
                    <button
                        class="btn btn-sm btn-ghost"
                        data-testid="scrape-login-btn"
                        on:click=on_login
                    >
                        "扫码登录"
                    </button>
                }.into_any()
            }) as Children
        >
            // ---------- 顶条 ----------
            // 顺序刻意固定（用户裁决）：筛选开关 → 开始抓取 → 开始/暂停（自动投递）
            // → 停止抓取 → 进度摘要 → 两个上限输入。「开始」必须紧贴「开始抓取」右边。
            <div class="scrape-bar">
                <button
                    class="chip as-btn scrape-toggle"
                    data-testid="scrape-filter-toggle"
                    on:click=move |_| expanded.update(|v| *v = !*v)
                >
                    {move || if expanded.get() { "收起筛选 ▲" } else { "筛选条件 ▼" }}
                </button>
                <button
                    class="btn btn-primary btn-sm"
                    data-testid="scrape-start-btn"
                    disabled=move || starting.get() || progress.get().running
                    on:click=on_start
                >
                    {move || if starting.get() { "启动中…" } else { "开始抓取" }}
                </button>
                // 自动投递：按钮贴在「开始抓取」右边（用户裁决 图 2）。
                // `AutoBar` 这个版本只产出扁平节点，数据流与 data-testid 不变。
                <AutoBar
                    on_sent=Callback::new(move |_| {
                        if let Some(cb) = on_auto_sent {
                            cb.run(());
                        }
                    })
                    on_error=fail
                />
                <button
                    class="btn btn-danger btn-sm"
                    data-testid="scrape-stop-btn"
                    disabled=move || !progress.get().running
                    on:click=on_stop
                >
                    "停止抓取"
                </button>
                <label class="scrape-lim" title="一次抓取最多落库多少条有效 JD">
                    <span class="dim">"有效 JD 上限"</span>
                    <input
                        class="input input-num scrape-lim-input"
                        type="number"
                        min="1"
                        max="9999"
                        data-testid="scrape-max-jd"
                        prop:value=move || max_jd.get().to_string()
                        on:change=on_max_jd
                    />
                    <span class="dim">"条"</span>
                </label>
                <label class="scrape-lim" title="低于该分值的岗位不入库">
                    <span class="dim">"分值门槛 ≥"</span>
                    <input
                        class="input input-num scrape-lim-input"
                        type="number"
                        min="0"
                        max=score_max.unwrap_or(0)
                        data-testid="scrape-min-score"
                        prop:value=move || min_score.get().to_string()
                        placeholder=min_score_default.map(|v| v.to_string())
                        on:change=on_min_score
                    />
                    <span class="dim">"分（低于不入库）"</span>
                </label>
                <span class="scrape-brief" title="抓取进程当前状态">
                    {move || progress_text()}
                </span>
                <span class="spacer"></span>
                {tail.map(|t| t())}
            </div>

            // ---------- 展开体：九类筛选条件（本地留档） ----------
            <Show when=move || expanded.get()>
                <ScrapeFilterPanel keywords=keywords notify=notify fail=fail />
            </Show>

            // ---------- 进度胶囊（E2E 契约区，文案不许改） ----------
            <div class="row wrap" style="gap:8px;margin-top:10px" data-testid="scrape-progress">
                <Stat label="已抓" value=scraped_text test_id="scrape-scraped" />
                <Stat label="入库" value=inserted_text tone="primary" test_id="scrape-inserted" />
                <Stat label="重复" value=duplicates_text test_id="scrape-duplicates" />
                <Stat label="低于门槛" value=below_threshold_text tone="warn" test_id="scrape-below" />
                <Stat label="规则否决" value=rejected_text tone="bad" test_id="scrape-rejected" />
                <span class="spacer"></span>
                <span class="card-sub">
                    {move || {
                        let p = progress.get();
                        if p.last_title.is_empty() {
                            "最近：—".to_string()
                        } else {
                            format!("最近：{}", p.last_title)
                        }
                    }}
                </span>
                <span class="sr-only">{summary_text}</span>
            </div>

            <div class="feed" data-testid="scrape-feed" node_ref=feed_el>
                <Show
                    when=move || !feed.get().is_empty()
                    fallback=|| {
                        view! {
                            <div class="empty">
                                "尚无事件。点击「开始抓取」后这里会滚动显示进度。"
                            </div>
                        }
                    }
                >
                    <For each=move || feed.get() key=|l| (l.tag.clone(), l.msg.clone(), l.tone) let:line>
                        <div class=format!("feed-line is-{}", line.tone)>
                            <span class="tag">{line.tag}</span>
                            <span class="msg">{line.msg}</span>
                        </div>
                    </For>
                </Show>
            </div>
            <p class="hint">
                "抓取会用真实登录态访问 Boss，每张卡片都有随机延时（1.5~9.8 秒），不要加速也不要并发。"
            </p>
        </Card>
    }
}

/// 从事件里取输入控件的值（select 与 input 都走这里）。
fn event_target_value(ev: &leptos::ev::Event) -> String {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok().map(|i| i.value()))
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlTextAreaElement>()
                    .ok()
                    .map(|i| i.value())
            })
        })
        .or_else(|| {
            ev.target().and_then(|t| {
                t.dyn_into::<web_sys::HtmlSelectElement>()
                    .ok()
                    .map(|i| i.value())
            })
        })
        .unwrap_or_default()
}

/// 滚到事件流底部。新事件追加在末尾，不滚的话用户看到的永远是最旧的一屏。
fn scroll_feed_bottom(el: NodeRef<leptos::html::Div>) {
    if let Some(node) = el.get_untracked() {
        node.set_scroll_top(node.scroll_height());
    }
}

/// `setInterval` 的薄包装。
///
/// **为什么不用 Leptos 的定时器**：0.8 的 `set_interval` 返回值无法在
/// 闭包里「从内部取消自己」，而这里需要「抓取结束后立刻停表」——
/// 靠信号重入会多跑一轮，日志会重复。
struct IntervalHandle {
    id: i32,
}

impl IntervalHandle {
    fn new<F: Fn() + 'static>(f: F) -> Self {
        thread_local! {
            static CURRENT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
        }
        let cb = Closure::<dyn FnMut()>::new(f);
        let id = web_sys::window()
            .and_then(|w| {
                w.set_interval_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    800,
                )
                .ok()
            })
            .unwrap_or(0);
        // 回调要活得和定时器一样久
        cb.forget();
        CURRENT.with(|c| c.set(id));
        Self { id }
    }

    fn stop_current() {
        thread_local! {
            static CURRENT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
        }
        CURRENT.with(|c| {
            let id = c.replace(0);
            if id != 0 {
                if let Some(w) = web_sys::window() {
                    w.clear_interval_with_handle(id);
                }
            }
        });
    }
}

/// 页签条数轮询周期的常量（页面层用它起 `IntervalHandle` 不现实，
/// 这里只作为文档锚点保留，避免常量被误删后无从追溯）。
#[allow(dead_code)]
const POLL_MS: u32 = E5;
