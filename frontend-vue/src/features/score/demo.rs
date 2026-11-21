//! 打分试算：输入一份岗位，按当前规则打分。
//!
//! **纯规则、不联网**——除非显式打开大模型开关（每条都会真实计费）。
//! 初筛线默认取 `llm_status.prefilter_default`，不在前端写死。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::*};
use crate::components::widgets::{Card, Chip};
use crate::lib_ui::job::score_tone;

#[component]
pub fn ScoreDemo(
    spec: Option<ScoringSpec>,
    llm: Option<LlmStatus>,
    fail: Callback<String>,
) -> impl IntoView {
    let title = RwSignal::new("Java 后端开发工程师".to_string());
    let salary = RwSignal::new("20-40K".to_string());
    let experience = RwSignal::new("5-10年".to_string());
    let jd = RwSignal::new(String::new());
    let use_llm = RwSignal::new(false);
    let prefilter = RwSignal::new(llm.as_ref().map(|l| l.prefilter_default).unwrap_or(0));
    let running = RwSignal::new(false);
    let reply = RwSignal::new(None::<ScoreReply>);

    let llm_ready = llm.as_ref().map(|l| l.configured).unwrap_or(false);
    let min_score = spec.map(|s| s.score_min_default);

    let run_ = move |_| {
        running.set(true);
        let args = ScoreArgs {
            company: String::new(),
            title: title.get_untracked(),
            salary: salary.get_untracked(),
            experience: experience.get_untracked(),
            jd: jd.get_untracked(),
            // 没配大模型时无论开关如何都只能走规则——命令层也会忽略，
            // 但这里先置 false 可以让界面行为与真实请求一致。
            use_llm: use_llm.get_untracked() && llm_ready,
            llm_prefilter: prefilter.get_untracked(),
        };
        spawn_local(async move {
            match api::score_job(args).await {
                Ok(r) => reply.set(Some(r)),
                Err(e) => fail.run(format!("试算失败：{e}")),
            }
            running.set(false);
        });
    };

    let result_tone = move || {
        reply
            .get()
            .map(|r| score_tone(r.score, min_score).as_str().to_string())
            .unwrap_or_else(|| "is-none".to_string())
    };

    view! {
        <Card
            test_id="demo-panel"
            title="打分试算"
            title_test_id="demo-title" 
            subtitle="纯规则，不调大模型、不联网"
        >
            <div class="demo-grid">
                <label class="field">
                    <span class="label">"岗位名称"</span>
                    <input
                        class="input"
                        data-testid="demo-title-input"
                        prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span class="label">"薪资"</span>
                    <input
                        class="input"
                        data-testid="demo-salary"
                        prop:value=move || salary.get()
                        on:input=move |ev| salary.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span class="label">"经验"</span>
                    <input
                        class="input"
                        data-testid="demo-exp"
                        prop:value=move || experience.get()
                        on:input=move |ev| experience.set(event_target_value(&ev))
                    />
                </label>
            </div>

            <label class="field" style="margin-top:8px">
                <span class="label">
                    "职位描述"
                    <span class="field-hint">"（留空会走「无 JD」判定，直接判不入库）"</span>
                </span>
                <textarea
                    class="textarea"
                    data-testid="demo-jd"
                    placeholder="负责后端服务设计与开发，熟悉 Java、Spring Boot、MySQL、Redis、RabbitMQ。"
                    prop:value=move || jd.get()
                    on:input=move |ev| jd.set(event_target_value(&ev))
                ></textarea>
            </label>

            <div class="row wrap" style="gap:12px;margin-top:10px">
                <button
                    class="btn btn-primary"
                    data-testid="demo-score-btn"
                    disabled=move || running.get()
                    on:click=run_
                >
                    {move || if running.get() { "试算中…" } else { "试算" }}
                </button>
                <label
                    class="switch"
                    style=move || if llm_ready { "opacity:1" } else { "opacity:.45" }
                >
                    <input
                        type="checkbox"
                        data-testid="demo-llm-toggle"
                        prop:checked=move || use_llm.get() && llm_ready
                        disabled=move || !llm_ready
                        on:change=move |ev| {
                            use_llm.set(event_target_checked(&ev));
                        }
                    />
                    "大模型打分"
                </label>
                <label class="row" style="gap:6px">
                    <span class="card-sub">"初筛线"</span>
                    <input
                        class="input input-num"
                        type="number"
                        data-testid="demo-prefilter"
                        prop:value=move || prefilter.get().to_string()
                        disabled=move || !llm_ready
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                prefilter.set(v);
                            }
                        }
                    />
                </label>
                <Show when=move || !llm_ready>
                    <span class="card-sub">"未配置密钥，仅规则可用"</span>
                </Show>
            </div>

            <Show when=move || reply.get().is_some()>
                <div class="demo-result" data-testid="demo-result">
                    <span class=move || format!("score-num {}", result_tone())>
                        {move || reply.get().map(|r| r.score).unwrap_or(0)}
                    </span>
                    <div class="col" style="gap:6px;flex:1;min-width:0">
                        <div class="row wrap" style="gap:6px">
                            {move || {
                                reply.get().map(|r| {
                                    let tone = if r.source == "llm" { "accent" } else { "default" };
                                    let src = if r.source == "llm" { "大模型" } else { "规则" };
                                    let mut chips = vec![
                                        view! { <Chip tone=tone>{src}</Chip> }.into_any(),
                                    ];
                                    if r.tokens > 0 {
                                        chips.push(
                                            view! {
                                                <Chip tone="default">
                                                    {format!("{} tokens", r.tokens)}
                                                </Chip>
                                            }
                                                .into_any(),
                                        );
                                    }
                                    if r.rejected_no_jd {
                                        chips.push(
                                            view! { <Chip tone="bad">"无 JD，判不入库"</Chip> }
                                                .into_any(),
                                        );
                                    }
                                    chips
                                })
                            }}
                        </div>
                        <div style="word-break:break-word">
                            {move || reply.get().map(|r| r.reason).unwrap_or_default()}
                        </div>
                    </div>
                </div>
            </Show>
        </Card>
    }
}
