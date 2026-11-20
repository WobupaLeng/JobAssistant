//! 单张岗位卡片（旧版图 2 的卡片流单元）。
//!
//! 结构（与旧版 `web/src/App.tsx` 的卡片 DOM 逐行对应）：
//! 1. 分数方块 + 标题 + 「已投递」勾选框
//! 2. 公司 · 薪资 · 平台 chip · 状态 chip
//! 3. 城市 | 经验 | 学历 | 规模 ……… 时间
//! 4. 「单击展开详情 / JD」 · 地址 pin
//!
//! 拆成独立组件而不是塞进 `list.rs`：一张卡片有 12 个 testid 与 4 种条件分支，
//! 留在列表里会让 `list.rs` 超过 600 行，`gui_e2e.py` 的静态契约也更难核对。

use leptos::prelude::*;
use leptos::ev;

use crate::api::types::Job;
use crate::components::widgets::Chip;
use crate::features::scrape::options::platform_name;
use crate::lib_ui::time::fmt_time;

use super::state::{BatchOp, JobListState};

/// 状态芯片三态（SPEC 7.11）：`delivered` → 已投递，`rejected` → 不投递，
/// 其余一律「待投递」。**不要**用 `status` 的原值渲染——库里还有
/// `pending` / `new` / 空串三种历史写法。
pub fn status_of(job: &Job) -> (&'static str, &'static str) {
    if job.rejected != 0 {
        ("bad", "不投递")
    } else if job.status == "delivered" {
        ("primary", "已投递")
    } else {
        ("default", "待投递")
    }
}

/// 分数方块着色档位。**必须**按满分比例算，不许写死 80 / 60 / 40——
/// 满分来自 `api::scoring_spec()`，改口径时这里要跟着动就说明写错了。
///
/// `None` = 不打分（已投递）/ 没有满分（spec 还没回来）。
pub fn score_tier(score: u8, max: Option<u8>, delivered: bool) -> &'static str {
    if delivered {
        return "is-done";
    }
    let Some(max) = max.filter(|m| *m > 0) else {
        return "is-none";
    };
    if score == 0 {
        return "is-none";
    }
    // 用整数比较而不是 `0.8 * m`：`0.6 * 60` 在 f32 里是 36.000004，
    // 60 分对满分 60 会被判成 `is-low`（实测踩到过）。乘 10 再除 10 又会引入
    // 另一轮舍入，所以直接比较 `10 * score` 与 `k * max`，全程整数。
    let s = score as u32 * 10;
    let m = max as u32;
    if s >= 8 * m {
        "is-high"
    } else if s >= 6 * m {
        "is-mid"
    } else if s >= 4 * m {
        "is-low"
    } else {
        "is-bad"
    }
}

/// 元信息行：城市 | 经验 | 学历 | 规模。空项跳过，全空时给「—」。
///
/// 分隔符由 CSS 的 `::before` 画，不拼进字符串——拼进字符串后
/// 省略号截断会把竖线一起吃掉，出现「深圳 · 3-5年 ·」这种尾巴。
pub fn meta_items(job: &Job) -> Vec<String> {
    [
        job.location.as_str(),
        job.experience.as_str(),
        job.degree.as_str(),
        job.company_scale.as_str(),
    ]
    .iter()
    .map(|s| s.trim())
    .filter(|s| !s.is_empty())
    .map(|s| s.to_string())
    .collect()
}

/// 地址只取首行（旧版 `company_address.split('\n')[0]`）。
pub fn addr_first_line(raw: &str) -> String {
    raw.lines().next().unwrap_or("").trim().to_string()
}

/// 卡片点击是否该被当成「展开详情」。
///
/// 点在按钮 / 勾选框 / 链接 / 输入控件上时不算——否则用户点「不投递」
/// 会先弹一层抽屉。旧版用 `closest('button,a,input,label,select,textarea')`，
/// 这里在事件里对 `event.target` 做同样判断。
fn should_open(ev: &ev::MouseEvent) -> bool {
    use wasm_bindgen::JsCast;
    let Some(t) = ev.target() else { return true };
    let Some(el) = t.dyn_ref::<web_sys::Element>() else {
        return true;
    };
    let tag = el.tag_name().to_ascii_lowercase();
    !matches!(
        tag.as_str(),
        "button" | "a" | "input" | "label" | "select" | "textarea"
    )
}

#[component]
pub fn JobCard(
    job: Job,
    state: JobListState,
    max: Option<u8>,
    notify: Callback<String>,
    fail: Callback<String>,
) -> impl IntoView {
    let id = job.job_id.clone();
    let id2 = id.clone();
    let id3 = id.clone();
    let row = job.clone();

    let delivered = job.status == "delivered";
    let rejected = job.rejected != 0;
    let (chip_tone, chip_label) = status_of(&job);
    let tier = score_tier(job.match_score, max, delivered);
    let score_max = max.map(|m| m as u32).unwrap_or(0);
    let score_txt = if score_max == 0 {
        job.match_score.to_string()
    } else {
        format!("{}/{}", job.match_score, score_max)
    };

    let title = if job.title.trim().is_empty() {
        "（无标题）".to_string()
    } else {
        job.title.clone()
    };
    let company = if job.company.trim().is_empty() {
        "（未知公司）".to_string()
    } else {
        job.company.clone()
    };
    let salary = if job.salary.trim().is_empty() {
        "面议".to_string()
    } else {
        job.salary.clone()
    };
    let platform = if job.platform.trim().is_empty() {
        "boss".to_string()
    } else {
        job.platform.clone()
    };
    let platform_label = platform_name(&platform);
    let meta = meta_items(&job);
    let meta_empty = meta.is_empty();
    let when = fmt_time(job.created_at);
    let addr = addr_first_line(&job.company_address);
    let has_addr = !addr.is_empty();

    let card_cls = {
        let mut c = String::from("job-card");
        if delivered {
            c.push_str(" is-delivered");
        }
        if rejected {
            c.push_str(" is-rejected");
        }
        c
    };

    // 标题类名：已投递加删除线（旧版 `line-through`）
    let title_cls = if delivered { "jc-title is-done" } else { "jc-title" };

    let st_click = state;
    let on_open = move |ev: ev::MouseEvent| {
        if !should_open(&ev) {
            return;
        }
        st_click.selected.set(Some(row.clone()));
    };

    // 行内「不投递」/「允许投递」——单条直接用行内标签，不进批量条
    let st_mark = state;
    let id_mark = id3.clone();
    let on_mark = move |ev: ev::MouseEvent| {
        ev.stop_propagation();
        let op = if rejected { BatchOp::Allow } else { BatchOp::Reject };
        st_mark.mark_one(id_mark.clone(), op, notify, fail);
    };

    let mark_label = if rejected { "允许投递" } else { "不投递" };
    let mark_tid = format!("job-allow-{id2}");

    view! {
        <article
            class=card_cls
            data-testid=format!("job-card-{id}")
            data-status=if delivered { "delivered" } else if rejected { "rejected" } else { "pending" }
            on:click=on_open
        >
            <div class="jc-line jc-title-line">
                <span
                    class=format!("jc-score {tier}")
                    title=format!("规则打分：{score_txt}")
                    data-testid=format!("job-score-{id}")
                >
                    {job.match_score.to_string()}
                </span>
                <span class=title_cls title=job.title.clone() data-testid=format!("job-title-{id}")>
                    {title}
                </span>
                <button
                    type="button"
                    class=if delivered { "jc-check is-on" } else { "jc-check" }
                    data-testid=format!("job-check-{id}")
                    title=if delivered { "已投递（点击取消标记）" } else { "标记为已投递" }
                    on:click={
                        let st = state;
                        let id = id.clone();
                        move |ev: ev::MouseEvent| {
                            ev.stop_propagation();
                            let st2 = st;
                            let id = id.clone();
                            if delivered {
                                // 已投递再点 = 放回待投递（allow_jobs 会同时清掉 delivered/rejected）
                                st2.mark_one(id, BatchOp::Allow, notify, fail);
                            } else {
                                st2.mark_delivered_one(id, notify, fail);
                            }
                        }
                    }
                >
                    "✓"
                </button>
            </div>

            <div class="jc-line jc-sub">
                <span class="jc-company" data-testid=format!("job-company-{id}") title=job.company.clone()>
                    {company}
                </span>
                <span class="jc-salary" data-testid=format!("job-salary-{id}")>{salary}</span>
                <Chip tone="default" title=platform_label.clone()>
                    <span data-testid=format!("job-platform-{id3}")>{platform_label}</span>
                </Chip>
                <span class=format!("jc-status chip chip-{chip_tone}") data-testid=format!("job-status-{id}")>
                    {chip_label}
                </span>
            </div>

            <div class="jc-line jc-meta">
                <span class="jc-meta-items" data-testid=format!("job-meta-{id}")>
                    {if meta_empty {
                        view! { <span class="dim">"—"</span> }.into_any()
                    } else {
                        meta.into_iter()
                            .map(|m| view! { <span class="jc-meta-item">{m}</span> })
                            .collect_view()
                            .into_any()
                    }}
                </span>
                <span class="jc-time" data-testid=format!("job-time-{id}")>{when}</span>
            </div>

            <div class="jc-line jc-foot">
                <span class="jc-hint" data-testid=format!("job-hint-{id}")>
                    {if rejected { "已标记不投递 · " } else { "" }}
                    "单击展开详情 / JD"
                </span>
                <button
                    type="button"
                    class="chip as-btn jc-mark"
                    data-testid=mark_tid
                    title=if rejected { "放回待投递" } else { "标记为不投递（不再出现在待投递页签）" }
                    on:click=on_mark
                >
                    {mark_label}
                </button>
                {has_addr.then(|| view! {
                    <span class="jc-addr" data-testid=format!("job-addr-{id}") title=job.company_address.clone()>
                        <span class="jc-pin">"📍"</span>{addr}
                    </span>
                })}
            </div>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> Job {
        Job::default()
    }

    #[test]
    fn status_needs_three_states() {
        let mut j = job();
        assert_eq!(status_of(&j).1, "待投递");
        j.status = "delivered".into();
        assert_eq!(status_of(&j).1, "已投递");
        j.rejected = 1;
        // rejected 优先：既是已投递又被否决时，卡片必须显示「不投递」
        assert_eq!(status_of(&j).1, "不投递");
        // 历史写法一律落到「待投递」
        let mut j2 = job();
        j2.status = "new".into();
        assert_eq!(status_of(&j2).1, "待投递");
    }

    #[test]
    fn score_tier_is_relative_to_spec_max() {
        // 满分 100：80/60/40 是分界（但这三个数字来自比例，不是常量）
        assert_eq!(score_tier(90, Some(100), false), "is-high");
        assert_eq!(score_tier(80, Some(100), false), "is-high");
        assert_eq!(score_tier(79, Some(100), false), "is-mid");
        assert_eq!(score_tier(60, Some(100), false), "is-mid");
        assert_eq!(score_tier(59, Some(100), false), "is-low");
        assert_eq!(score_tier(40, Some(100), false), "is-low");
        assert_eq!(score_tier(39, Some(100), false), "is-bad");
        // 满分换成 50 时同一分数档位必须跟着变，否则就是写死了阈值
        assert_eq!(score_tier(40, Some(50), false), "is-high");
        assert_eq!(score_tier(0, Some(100), false), "is-none");
        assert_eq!(score_tier(50, None, false), "is-none");
        assert_eq!(score_tier(90, Some(100), true), "is-done");
    }

    #[test]
    fn meta_skips_blanks() {
        let mut j = job();
        j.location = "深圳".into();
        j.experience = "  ".into();
        j.degree = "本科".into();
        j.company_scale = "1000-9999人".into();
        assert_eq!(meta_items(&j), vec!["深圳", "本科", "1000-9999人"]);
        assert!(meta_items(&job()).is_empty());
    }

    #[test]
    fn addr_takes_first_line_only() {
        assert_eq!(addr_first_line("科技园路 1 号\n南山区"), "科技园路 1 号");
        assert_eq!(addr_first_line(""), "");
        assert_eq!(addr_first_line("  a  "), "a");
    }
}
