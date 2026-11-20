//! 概览页：统计 + 投递列表 + 抓取 + 求职期望 + 打分试算 + 详情抽屉。
//!
//! 这是默认落地页。`tools/gui_e2e.py` 只按 testid 定位本页控件，
//! **任何控件都不得被挪到别的页签或折叠区**——那会让 E2E 静默失效。
//!
//! 状态归属（分层规则的具体体现）：
//! - 列表 / 页签 / 排序 / 勾选 / 选中 → `features/jobs/state.rs`（只有本页用）
//! - 关键词 / 门槛                    → 本文件（`ScrapePanel` 与 `Expectations` 共享）
//!
//! **一个反直觉的地方**：`Card`/`Stat`/`Banner` 的 `title`/`label`/`action` 是
//! `AnyView` 而非 `Children`，所以传的是**已构造好的视图**，不是闭包，也不是
//! 裸字符串。写 `label="岗位总数"` 编不过，必须 `label=view! { "岗位总数" }`。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::components::shell::Boot;
use crate::components::widgets::{Banner, Stat, StatSkeleton};
use crate::features::auto::AutoBar;
use crate::features::jobs::drawer::JobDrawer;
use crate::features::jobs::list::JobList;
use crate::features::jobs::rescore::RescorePanel;
use crate::features::jobs::state::JobListState;
use crate::features::scrape::expectations::Expectations;
use crate::features::scrape::panel::ScrapePanel;
use crate::features::score::demo::ScoreDemo;
use crate::features::sensitive::SensitivePanel;
use crate::lib_ui::text::merge_keywords;

/// 页签计数的轮询周期。旧版就是 5 秒，用户已经习惯这个刷新节奏；
/// 改快没有意义（计数只有抓取时才会动），改慢会让「已投递」的即时反馈变钝。
const TAB_POLL_MS: i32 = 5000;

#[component]
pub fn DashboardPage(on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let fail = Callback::new(move |e: String| boot.error.set(Some(e)));

    let list = JobListState::new();

    // 关键词与门槛提到页面层：抓取面板与求职期望都要改关键词。
    let keywords = RwSignal::new(String::new());
    let min_score = RwSignal::new(0u8);

    // 首帧：等 spec 到了再拉列表（门槛着色与默认门槛都依赖它）。
    let kicked = RwSignal::new(false);
    Effect::new(move |_| {
        let spec_now = boot.spec.get();
        if kicked.get_untracked() || spec_now.is_none() {
            return;
        }
        kicked.set(true);
        if let Some(s) = spec_now {
            // 默认门槛只有后端知道，前端不写死数字
            min_score.set(s.score_min_default);
        }
        list.refresh(fail);
    });

    // 页签 / 「查看全部」/ 排序变化 → 重新拉。
    //
    // 键里必须同时含页签与 show_all：只看排序会在切页签时静默不刷新，
    // 界面上页签高亮变了、列表却是旧的，看起来像"筛选没生效"。
    let last_key = RwSignal::new((String::new(), String::new(), false));
    Effect::new(move |_| {
        let key = (
            list.sort.get().as_str().to_string(),
            list.tab
                .get()
                .slug()
                .to_string(),
            list.ascending.get(),
        );
        let prev = last_key.get_untracked();
        if prev == key {
            return;
        }
        last_key.set(key);
        if kicked.get_untracked() {
            list.refresh(fail);
        }
    });

    // 【页签计数 5 秒轮询】只刷三个药丸后面的数字，不动列表：
    // 每 5 秒重拉 500 条岗位会让正在勾选的用户丢掉勾选状态。
    // 页面卸载时撤表（`on_cleanup`），否则切到别的页签后定时器还在跑。
    {
        use wasm_bindgen::JsCast;
        let me = list;
        let poll = move || {
            spawn_local(async move {
                match api::job_tab_counts().await {
                    Ok(c) => me.counts.set(c),
                    // 轮询失败不弹错：后端重启中会连续失败，弹窗会糊屏。
                    // 下一次成功就自愈，真有持久故障由用户点「刷新」兜住。
                    Err(_) => {}
                }
            });
        };
        poll();
        let handle = web_sys::window()
            .and_then(|w| {
                let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(poll.clone());
                let id = w
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(),
                        TAB_POLL_MS,
                    )
                    .ok();
                // 回调要活到定时器被撤掉为止
                cb.forget();
                id
            })
            .unwrap_or(0);
        on_cleanup(move || {
            if handle != 0 {
                if let Some(w) = web_sys::window() {
                    w.clear_interval_with_handle(handle);
                }
            }
        });
    }

    let excluded = move || list.jobs.get().iter().filter(|j| j.is_rejected()).count() as u32;

    // `Stat::value` 收 `ReadSignal<String>`：计数来自 `RwSignal<usize>`，
    // 在这里一次性映射成文本，组件本身保持纯展示。
    let total_text = Signal::derive(move || list.total.get().to_string());
    let listed_text = Signal::derive(move || list.jobs.get().len().to_string());
    let excluded_text = Signal::derive(move || excluded().to_string());

    let score_max = move || boot.spec.get().map(|s| s.score_max);
    let score_min = move || boot.spec.get().map(|s| s.score_min_default);
    let llm_configured = move || boot.llm.get().map(|l| l.configured).unwrap_or(false);

    // ---------- 敏感库抽屉（与岗位详情抽屉互斥） ----------
    //
    // 这里用 `SensitivePanel` 而不是 `SensitiveDrawer`：入口按钮必须落在工具行里
    // （SPEC §2.1「页头右侧 btn btn-sm」），而 `SensitiveDrawer` 内部自持一个
    // `open` 信号、外部点不中它——`drawer_entry.rs` 的模块注释已写明「两种都只挂
    // 一次，不要同时挂」，所以只挂 `SensitivePanel` 这一种。
    let sensitive_open = RwSignal::new(false);
    // 敏感库条数刷新信号：抓取入库 / 自动投递发出后各递增一次，
    // 抽屉开着时才会真的重拉（`SensitivePanel` 内部判 `open`）。
    let sensitive_reload = RwSignal::new(0u32);
    let refresh_sensitive = Callback::new(move |_: ()| sensitive_reload.update(|n| *n += 1));

    view! {
        // 迁移横幅：只在真的检测到十分制残留时出现
        {move || {
            if !list.has_legacy.get() {
                return None;
            }
            let me = list;
            let notice = on_notice;
            let f = fail;
            Some(
                view! {
                    <Banner
                        test_id="legacy-banner"
                        action=Box::new(move || view! {
                            <button
                                class="btn btn-sm btn-warn"
                                data-testid="migrate-btn"
                                on:click=move |_| me.migrate(notice, f)
                            >
                                "迁移到百分制"
                            </button>
                        }.into_any()) as Children
                    >
                        "检测到 "
                        <b>"未迁移的十分制数据"</b>
                        "，分数口径与当前规则不一致。迁移会把旧分数×10 并打上标记，只执行一次。"
                    </Banner>
                },
            )
        }}

        <section class="card" style="padding:12px 16px">
            <div class="row wrap" style="gap:10px">
                // 首屏骨架：三枚胶囊用同形状的骨架顶住，数据一到就地替换（不塌高、不跳位）。
                {move || {
                    if list.loading.get() {
                        view! {
                            <StatSkeleton test_id="overview-total-skeleton" />
                            <StatSkeleton test_id="overview-listed-skeleton" />
                            <StatSkeleton test_id="overview-rejected-skeleton" />
                        }.into_any()
                    } else {
                        view! {
                            <Stat
                                label="岗位总数"
                                value=total_text
                                test_id="overview-total"
                            />
                            <Stat
                                label="当前列表"
                                value=listed_text
                                test_id="overview-listed"
                            />
                            <Stat
                                label="已排除"
                                value=excluded_text
                                tone="warn"
                                test_id="overview-rejected"
                            />
                        }.into_any()
                    }
                }}
                <span class="spacer" style="min-width:12px"></span>
                // 敏感库入口（SPEC §2.1：页头右侧 `btn btn-sm`，文案「敏感库 {n} 条」）。
                // 按钮由 `SensitivePanel` 自己渲染（`show_entry` 默认 true），
                // 这样条数来自它内部那份已加载的列表，页面层不必再查一次 IPC。
                <SensitivePanel
                    open=sensitive_open
                    reload=sensitive_reload
                    on_open=Callback::new(move |_| list.selected.set(None))
                />
                <RescorePanel
                    llm_configured=llm_configured()
                    total=Signal::derive(move || list.total.get())
                    on_done=Callback::new(move |_| list.refresh(fail))
                    fail=fail
                />
            </div>
        </section>

        // 自动投递控制条：独立一条，放在投递列表之前（SPEC §1.2 第 2 项）。
        //
        // `on_sent` 复用页面现有的刷新路径 `list.refresh`——它会顺带重拉
        // 概览计数、页签计数与列表，自动投递把一条标成「已投递」后三处都要跟着变。
        // `on_error` 走页面现有的顶部提示条（`on_notice`），与其余面板一致；
        // 控制条内部另有自己的 `auto-error`，两处不冲突。
        <AutoBar
            on_sent=Callback::new(move |_| {
                list.refresh(fail);
                refresh_sensitive.run(());
            })
            on_error=on_notice
        />

        <JobList state=list max_score=score_max().unwrap_or(0) notify=on_notice fail=fail />

        <ScrapePanel
            keywords=keywords
            min_score=min_score
            score_max=score_max()
            min_score_default=score_min()
            on_inserted=Callback::new(move |_| {
                list.refresh(fail);
                // 抓取入库会同时写敏感库（stored 条目），抽屉开着时要能看见新条目
                refresh_sensitive.run(());
            })
            notify=on_notice
            fail=fail
        />

        <Expectations
            on_pick=Callback::new(move |names: Vec<String>| {
                let merged = merge_keywords(&keywords.get_untracked(), &names);
                keywords.set(merged.text);
                if !merged.added.is_empty() {
                    on_notice.run(format!("已填入 {} 个关键词", merged.added.len()));
                } else {
                    on_notice.run("这些期望职位已经在关键词里了".to_string());
                }
            })
            fail=fail
        />

        <ScoreDemo spec=boot.spec.get() llm=boot.llm.get() fail=fail />

        {move || {
            list.selected.get().map(|job| {
                let me = list;
                view! {
                    <JobDrawer
                        job=job
                        min_score=score_min()
                        score_max=score_max()
                        on_close=Callback::new(move |_| me.selected.set(None))
                    />
                }
            })
        }}
    }
}
