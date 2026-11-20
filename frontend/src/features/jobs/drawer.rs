//! 岗位详情抽屉。
//!
//! **为什么是抽屉不是模态**：用户要对照列表看详情，模态会把列表压暗到看不清；
//! 抽屉从右侧滑出，左侧列表仍可读、可继续切换下一个岗位。
//!
//! 结构与文案照抄只读参考 `skeleton-web/src/features/jobs/JobDrawer.tsx`
//! （用户要求「与最初版相似即可，不要自由发挥」）：遮罩 + 面板、头部（标题 + 公司/薪资/
//! 城市 chips + 分数 + `/ 满分 分`）、第二行 chips（来源 / 是否达门槛 / 制式 / 平台）、
//! 打分依据条、kv 信息表、JD 区。样式见 `styles/drawer.css`。
//!
//! 列表接口默认不返回完整 JD（正文体积太大），因此打开时若 `jd_text` 为空，
//! 用 `get_job` 补拉一次。**补拉失败不能把整个抽屉干掉**——其余字段已经够看。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::Job};
use crate::components::widgets::Chip;
use crate::lib_ui::job::score_tone;
use crate::lib_ui::motion::{class, play_next_frame};
use crate::lib_ui::time::fmt_time;

/// 空字段统一显示破折号。直接给空串会让抽屉出现「空洞」，
/// 看起来像数据加载失败而不是「这个字段确实没有」。
fn dash(s: &str) -> String {
    if s.trim().is_empty() {
        "—".to_string()
    } else {
        s.to_string()
    }
}

#[component]
pub fn JobDrawer(
    job: Job,
    min_score: Option<u8>,
    score_max: Option<u8>,
    on_close: Callback<()>,
) -> impl IntoView {
    // 补拉完整 JD。用 RwSignal 而不是派生 Memo：它只会被赋值一次。
    let job_id = job.job_id.clone();
    let full = RwSignal::new(job.clone());
    let jd_err = RwSignal::new(String::new());
    let jd_loading = RwSignal::new(job.jd_text.is_empty());

    if job.jd_text.is_empty() {
        spawn_local(async move {
            match api::get_job(&job_id).await {
                Ok(v) => full.set(v),
                Err(e) => jd_err.set(e),
            }
            jd_loading.set(false);
        });
    }

    // Esc 关闭。必须挂在 window 上而不是面板元素上：
    // 用户焦点可能还停在列表里，面板自己收不到键盘事件。
    //
    // 监听器必须随抽屉卸载一起摘掉（`on_cleanup`）：用 `Closure::forget` 会永久泄漏，
    // 且关掉抽屉后按 Esc 仍会触发上一次的 `on_close`，把下一个刚打开的抽屉也关掉。
    // `on_cleanup` 的入参要求 `Send`，而 `Closure` 内部含 `Rc` 不满足；
    // 监听器句柄改用 js_sys 的 `Function` 持有——它由 GC 管，天然 `Send`，
    // 节点卸载时 `removeEventListener` 摘掉，`Function` 随之可回收。
    {
        use wasm_bindgen::JsCast;
        let handler = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |ev: web_sys::KeyboardEvent| {
                if ev.key() == "Escape" {
                    ev.prevent_default();
                    let cb = on_close;
                    cb.run(());
                }
            },
        )
        .into_js_value()
        .unchecked_into::<js_sys::Function>();
        if let Some(win) = web_sys::window() {
            let _ = win.add_event_listener_with_callback(
                "keydown",
                handler.clone().unchecked_into::<js_sys::Function>().unchecked_ref(),
            );
            let win2 = win.clone();
            on_cleanup(move || {
                let _ = win2
                    .remove_event_listener_with_callback("keydown", handler.unchecked_ref());
            });
        }
    }

    // 面板挂载后补一帧再打动画类：初始态（opacity:0 / translateX）必须先被浏览器算一次，
    // 否则初始态与动画在同一帧生效，进场会被整段跳过。
    let panel = NodeRef::<leptos::html::Aside>::new();
    Effect::new(move |_| {
        if let Some(el) = panel.get() {
            play_next_frame(el.into(), class::DRAWER_IN);
        }
    });

    let tone_now = move || score_tone(full.get().match_score, min_score);
    let score_cls = move || format!("score-num drawer-score is-{}", tone_now().as_str());

    // 是否达门槛：只有「门槛已知且已打分」时才给结论；否则不渲染这枚 chip
    // ——宁可不说，也不说错（旧版同样如此）。
    let passes = move || -> Option<bool> {
        let min = min_score?;
        let j = full.get();
        if j.match_score == 0 {
            return None;
        }
        Some(j.match_score >= min)
    };

    let reason_text = move || {
        let j = full.get();
        if !j.score_reason.trim().is_empty() {
            j.score_reason.clone()
        } else if jd_loading.get() {
            "读取中…".to_string()
        } else {
            "无打分说明".to_string()
        }
    };

    let jd_text = move || {
        let j = full.get();
        if !j.jd_text.trim().is_empty() {
            j.jd_text.clone()
        } else if !jd_err.get().is_empty() {
            format!("（读取职位描述失败：{}）", jd_err.get())
        } else if jd_loading.get() {
            "读取中…".to_string()
        } else {
            "（该岗位没有 JD 原文，抓取时通常按「无 JD」降权处理）".to_string()
        }
    };

    let pass_chip = move || {
        let min = min_score?;
        Some(if passes()? {
            view! {
                <Chip tone="primary" title="分数达到当前入库门槛">
                    {format!("达到门槛 {min}")}
                </Chip>
            }
            .into_any()
        } else {
            view! {
                <Chip tone="bad" title="分数低于当前入库门槛">
                    {format!("低于门槛 {min}")}
                </Chip>
            }
            .into_any()
        })
    };

    let j0 = job.clone();
    let title = if j0.title.trim().is_empty() {
        "（无标题）".to_string()
    } else {
        j0.title.clone()
    };
    let company = dash(&j0.company);
    let salary = dash(&j0.salary);
    let location = dash(&j0.location);
    let source = dash(&j0.score_source);
    let platform = dash(&j0.platform);
    let experience = dash(&j0.experience);
    let degree = dash(&j0.degree);
    let industry = dash(&j0.company_industry);
    let scale = dash(&j0.company_scale);
    let stage = dash(&j0.company_stage);
    let status = dash(&j0.status);
    // HR 活跃度是一枚 chip，只在有值时出现。用 `RwSignal` 持有：`Show` 的 `children`
    // 要求 `Fn`，直接把 `String` 移进 `view!` 会让闭包退化成 `FnOnce`（编译报 E0525）。
    let active = RwSignal::new(j0.hr_active_status.clone());
    let hiring = if j0.hiring_count > 0 {
        format!("{} 个岗位", j0.hiring_count)
    } else {
        "—".to_string()
    };
    let hr = match (dash(&j0.hr_name), dash(&j0.hr_position)) {
        (n, p) if n == "—" => p,
        (n, p) if p == "—" => n,
        (n, p) => format!("{n} · {p}"),
    };
    // 入库时间：优先 `updated_at`（重算会刷新它），没有再用 `created_at` —— 与旧版一致。
    let when_raw = if j0.updated_at > 0 { j0.updated_at } else { j0.created_at };
    let when = if when_raw > 0 { fmt_time(when_raw) } else { "—".to_string() };
    let href = j0.job_url.clone();

    // `Callback::run` 消耗 self，闭包体里必须先复制一份，否则整个闭包退化成 `FnOnce`。
    let close = move |_: web_sys::MouseEvent| {
        let cb = on_close;
        cb.run(());
    };

    view! {
        <div
            class="drawer-backdrop"
            data-testid="job-drawer-backdrop"
            aria-hidden="true"
            on:click=close
        ></div>
        <aside
            node_ref=panel
            class="drawer"
            data-testid="job-drawer-panel"
            role="dialog"
            aria-label="岗位详情"
        >
            <header class="drawer-head">
                <div class="col" style="gap:8px;min-width:0;flex:1 1 auto">
                    <h2 class="drawer-title" title=title.clone() data-testid="job-drawer-title">
                        {title.clone()}
                    </h2>
                    <div class="row wrap" style="gap:6px">
                        <span class="card-sub" data-testid="job-drawer-company">{company}</span>
                        <Chip tone="default">{salary}</Chip>
                        <Chip tone="default">{location}</Chip>
                        <Show when=move || full.get().is_rejected()>
                            <Chip tone="bad">"规则否决"</Chip>
                        </Show>
                    </div>
                </div>
                <div class="drawer-score-box">
                    <span class=score_cls data-testid="job-drawer-score">
                        {move || {
                            let s = full.get().match_score;
                            if s == 0 { "—".to_string() } else { s.to_string() }
                        }}
                    </span>
                    <span class="card-sub" data-testid="job-drawer-score-max">
                        {move || match score_max {
                            Some(m) => format!("/ {m} 分"),
                            None => "/ — 分".to_string(),
                        }}
                    </span>
                </div>
                <button
                    class="btn btn-sm btn-ghost"
                    data-testid="job-drawer-close"
                    title="关闭（Esc）"
                    aria-label="关闭"
                    on:click=close
                >
                    "✕"
                </button>
            </header>

            <div class="drawer-body">
                <div class="row wrap" style="gap:6px" data-testid="job-drawer-chips">
                    <Chip tone="accent">{source}</Chip>
                    {pass_chip}
                    {match score_max {
                        Some(m) => view! { <Chip tone="default">{format!("制式 {m}")}</Chip> }.into_any(),
                        None => ().into_any(),
                    }}
                    <Chip tone="default">{platform}</Chip>
                </div>

                <section>
                    <div class="section-label-static">"打分依据"</div>
                    <div class="jd-text drawer-reason" data-testid="job-drawer-reason">
                        {reason_text}
                    </div>
                </section>

                <section>
                    <div class="section-label-static">"岗位信息"</div>
                    <dl class="kv" data-testid="job-drawer-kv">
                        <dt>"经验"</dt>
                        <dd>{experience}</dd>
                        <dt>"学历"</dt>
                        <dd>{degree}</dd>
                        <dt>"行业"</dt>
                        <dd>{industry}</dd>
                        <dt>"规模"</dt>
                        <dd>{scale}</dd>
                        <dt>"阶段"</dt>
                        <dd>{stage}</dd>
                        <dt>"在招"</dt>
                        <dd>{hiring}</dd>
                        <dt>"HR"</dt>
                        <dd>
                            {hr}
                            <Show when=move || !active.get().is_empty()>
                                <Chip tone="default">{move || active.get()}</Chip>
                            </Show>
                        </dd>
                        <dt>"状态"</dt>
                        <dd>
                            {status}
                            <Show when=move || full.get().is_rejected()>
                                <Chip tone="bad">"已排除"</Chip>
                            </Show>
                        </dd>
                        <dt>"入库"</dt>
                        <dd>{when}</dd>
                        <dt>"链接"</dt>
                        <dd>
                            {if href.is_empty() {
                                view! { <span class="dim">"—"</span> }.into_any()
                            } else {
                                view! {
                                    <a
                                        href=href.clone()
                                        target="_blank"
                                        rel="noreferrer"
                                        data-testid="job-drawer-link"
                                    >
                                        "在浏览器打开"
                                    </a>
                                }
                                .into_any()
                            }}
                        </dd>
                    </dl>
                </section>

                <section style="display:flex;flex-direction:column;min-height:0">
                    <div class="section-label-static">"职位描述"</div>
                    <pre class="jd-text" data-testid="job-drawer-jd">{jd_text}</pre>
                </section>
            </div>
        </aside>
    }
}
