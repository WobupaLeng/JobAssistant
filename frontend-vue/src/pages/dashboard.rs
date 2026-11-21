//! 概览页：列表 + 运行日志 + 抓取 + 求职期望 + 打分试算 + 详情抽屉。
//!
//! 这是默认落地页。`tools/gui_e2e.py` 只按 testid 定位本页控件，
//! **任何控件都不得被挪到别的页签或折叠区**——那会让 E2E 静默失效。
//!
//! 状态归属（分层规则的具体体现）：
//! - 列表 / 页签 / 排序 / 勾选 / 选中 → `features/jobs/state.rs`（只有本页用）
//! - 关键词 / 门槛                    → 本文件（`ScrapePanel` 与 `Expectations` 共享）
//!
//! **布局（用户裁决）**：投递列表撑满剩余高度并自己内部滚动，运行日志是它下方的
//! 独立卡片；其余面板顺序下移，整页由 `.dashboard` 纵向滚动。
//!
//! **一个反直觉的地方**：`Card`/`Stat`/`Banner` 的 `title`/`label`/`action` 是
//! `AnyView` 而非 `Children`，所以传的是**已构造好的视图**，不是闭包，也不是
//! 裸字符串。写 `label="岗位总数"` 编不过，必须 `label=view! { "岗位总数" }`。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::api::types::Job;
use crate::components::shell::Boot;
use crate::components::widgets::Banner;
use crate::features::jobs::drawer::JobDrawer;
use crate::features::jobs::list::JobList;
use crate::features::jobs::rescore::RescorePanel;
use crate::features::jobs::state::JobListState;
use crate::features::logs::panel::LogPanel;
use crate::features::logs::state::LogState;
use crate::features::scrape::expectations::Expectations;
use crate::features::scrape::panel::ScrapePanel;
use crate::features::score::demo::ScoreDemo;
use crate::lib_ui::text::merge_keywords;

/// 页签计数的轮询周期。旧版就是 5 秒，用户已经习惯这个刷新节奏；
/// 改快没有意义（计数只有抓取时才会动），改慢会让「已投递」的即时反馈变钝。
const TAB_POLL_MS: i32 = 5000;

/// 「当前列表」的数字：本次 `list_jobs` 回来的行数。
///
/// 与「已排除」不是互补关系——两者同时存在，用户要能一眼看出
/// 「这一屏里有几条是被规则挡掉的」，所以这里**不**扣掉 `rejected`。
fn listed_count(jobs: &[Job]) -> usize {
    jobs.len()
}

/// 「已排除」的数字：`rejected != 0` 的行数。
fn rejected_count(jobs: &[Job]) -> usize {
    jobs.iter().filter(|j| j.is_rejected()).count()
}

/// 触发列表重拉的键：**排序字段 + 页签 + 方向**三者的快照。
///
/// 三样必须同时在键里。少了页签，切页签时这个 Effect 不认为发生了变化，
/// 界面上的页签高亮变了、列表却还是旧的，看起来像「筛选没生效」——
/// 这正是本页历史上踩过的坑。少了排序字段，点「薪资」不会重排。
///
/// 页签用 `slug()` 而不是 `ListTab` 本身：键要能跨版本比较，
/// `slug` 同时也是 `list_jobs` 的 `tab` 参数，两者同源。
fn refresh_key(list: &JobListState) -> (String, String, bool) {
    (
        list.sort.get().as_str().to_string(),
        list.tab.get().slug().to_string(),
        list.ascending.get(),
    )
}

#[component]
pub fn DashboardPage(on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let fail = Callback::new(move |e: String| boot.error.set(Some(e)));

    let list = JobListState::new();
    // 运行日志卡片：概览页给一份自己的 `LogState`（日志页那份是本页私有的，
    // 两边互不干扰；自动刷新循环各自随组件卸载而停）。
    let logs = LogState::new();

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
        let key = refresh_key(&list);
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

    let excluded = move || rejected_count(&list.jobs.get());

    // `Stat::value` 收 `ReadSignal<String>`：计数来自 `RwSignal<usize>`，
    // 在这里一次性映射成文本，组件本身保持纯展示。
    let total_text = Signal::derive(move || list.total.get().to_string());
    let listed_text = Signal::derive(move || listed_count(&list.jobs.get()).to_string());
    let excluded_text = Signal::derive(move || excluded().to_string());
    // 三枚胶囊与骨架已从页面移除（用户：图 5 里那三个不要）。数字自己仍可被
    // `db_overview` 的回归测试与调试读取，所以函数与信号都保留，只是不上屏。
    let _ = (total_text, listed_text, excluded_text);

    let score_max = move || boot.spec.get().map(|s| s.score_max);
    let score_min = move || boot.spec.get().map(|s| s.score_min_default);

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

        // 投递列表：撑满剩余高度，内部滚动（见 styles/jobs.css 的 `.job-list`）。
        <JobList state=list max_score=score_max().unwrap_or(0) notify=on_notice fail=fail />

        // 运行日志：投递列表下方一张独立卡片，只放运行日志这一路数据。
        <LogPanel state=logs compact=true />

        <ScrapePanel
            keywords=keywords
            min_score=min_score
            score_max=score_max()
            min_score_default=score_min()
            on_inserted=Callback::new(move |_| list.refresh(fail))
            on_auto_sent=Callback::new(move |_| list.refresh(fail))
            notify=on_notice
            fail=fail
            rescore=ViewFn::from(move || {
                view! {
                    <RescorePanel
                        llm_configured=boot.llm.get().map(|l| l.configured).unwrap_or(false)
                        total=Signal::derive(move || list.total.get())
                        on_done=Callback::new(move |_| list.refresh(fail))
                        fail=fail
                    />
                }
            })
            rescore_total=Signal::derive(move || list.total.get())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::jobs::state::{ListTab, SortField};

    fn job(id: &str, status: &str, rejected: i64) -> Job {
        Job {
            job_id: id.to_string(),
            status: status.to_string(),
            rejected,
            ..Default::default()
        }
    }

    /// 「当前列表」来自**当前这一屏列表**而不是 `total`：接到 `total` 上会让
    /// 「已排除」永远显示全库总数，翻页时数字纹丝不动。
    #[test]
    fn overview_counts_come_from_the_current_page_not_the_db() {
        let rows = vec![
            job("p1", "pending", 0),
            job("p2", "pending", 0),
            job("r1", "pending", 1),
        ];
        // 「当前列表」是所有行，包含被排除的
        assert_eq!(listed_count(&rows), 3);
        // 「已排除」只数 rejected != 0
        assert_eq!(rejected_count(&rows), 1);
        // 空列表两个都是 0，不是「未知」也不是某个默认值
        assert_eq!(listed_count(&[]), 0);
        assert_eq!(rejected_count(&[]), 0);
    }

    /// `rejected` 是 i64 的历史遗留列，非 0 即否决。写成 `== 1` 会让
    /// 那些被规则反复加一的岗位（值可能是 2、3）从「已排除」里消失。
    #[test]
    fn rejected_count_accepts_any_non_zero_marker() {
        let rows = vec![job("a", "pending", 2), job("b", "pending", -1), job("c", "pending", 0)];
        assert_eq!(rejected_count(&rows), 2);
    }

    #[test]
    fn refresh_key_changes_when_tab_changes_even_if_sort_is_unchanged() {
        let list = JobListState::new();
        list.sort.set(SortField::Created);
        list.ascending.set(false);
        let before = refresh_key(&list);

        list.tab.set(ListTab::Delivered);
        let after = refresh_key(&list);

        assert_ne!(
            before, after,
            "切页签必须让重拉键变化，否则列表停在旧数据上、页签却已高亮"
        );
        assert_eq!(after.1, "delivered");
    }

    #[test]
    fn refresh_key_changes_when_sort_field_changes() {
        let list = JobListState::new();
        list.tab.set(ListTab::Pending);
        let before = refresh_key(&list);
        list.sort.set(SortField::Salary);
        assert_ne!(before, refresh_key(&list));
    }

    #[test]
    fn refresh_key_changes_when_direction_flips() {
        let list = JobListState::new();
        list.sort.set(SortField::Score);
        list.ascending.set(false);
        let before = refresh_key(&list);
        list.ascending.set(true);
        assert_ne!(before, refresh_key(&list));
    }

    /// 键是快照不是引用：其他信号（列表内容、勾选、计数）变化**不该**
    /// 触发重拉，否则 5 秒一次的计数轮询会连带把整张列表重来一遍。
    #[test]
    fn refresh_key_ignores_signals_that_do_not_change_the_query() {
        use crate::api::types::JobTabCounts;
        let list = JobListState::new();
        list.sort.set(SortField::Created);
        list.tab.set(ListTab::Pending);
        list.ascending.set(true);
        let before = refresh_key(&list);

        list.jobs.set(vec![job("x", "pending", 0)]);
        list.checked.set(vec!["x".to_string()]);
        list.counts.set(JobTabCounts {
            pending: 9,
            delivered: 3,
            rejected: 1,
        });
        list.total.set(99);
        list.show_all.set(true);

        assert_eq!(before, refresh_key(&list));
    }
}
