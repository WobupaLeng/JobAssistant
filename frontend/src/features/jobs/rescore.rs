//! 全量重算面板：按当前口径重扫全库分数。
//!
//! **分页由调用方驱动**：后端 `rescore_jobs` 一次只处理 `limit` 条并回一个游标，
//! 前端必须循环到 `has_more=false` 为止。这是刻意的——一次扫 3 万条会把命令
//! 卡到超时，且中途中断时用户看不出进度。

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, types::*};

/// 每批处理条数。后端 clamp 到 1..=500，这里取 100：
/// 批量越大单次 IPC 越久，界面上的「重算中…」会显得卡死。
const BATCH: u32 = 100;

#[component]
pub fn RescorePanel(
    llm_configured: bool,
    total: Signal<usize>,
    on_done: Callback<()>,
    fail: Callback<String>,
) -> impl IntoView {
    let use_llm = RwSignal::new(false);
    let running = RwSignal::new(false);
    let note = RwSignal::new(String::new());

    let run_all = move |_| {
        if running.get_untracked() {
            return;
        }
        running.set(true);
        note.set("正在重算…".to_string());
        let wanted_llm = use_llm.get_untracked() && llm_configured;
        spawn_local(async move {
            let mut cursor: Option<String> = None;
            let mut scanned = 0u32;
            let mut updated = 0u32;
            let mut failed = 0u32;
            loop {
                let args = RescoreArgs {
                    after_job_id: cursor.clone(),
                    limit: Some(BATCH),
                    use_llm: wanted_llm,
                    llm_prefilter: None,
                    log_changes: false,
                };
                match api::rescore_jobs(args).await {
                    Ok(r) => {
                        scanned += r.scanned;
                        updated += r.updated;
                        failed += r.failed;
                        note.set(format!("已扫描 {scanned} 条，改写 {updated} 条"));
                        if !r.has_more || r.scanned == 0 {
                            break;
                        }
                        cursor = r.next_cursor;
                        if cursor.is_none() {
                            // 后端说还有但没给游标：再循环会算同一批，
                            // 会陷入死循环且分数被反复写。必须停。
                            break;
                        }
                    }
                    Err(e) => {
                        fail.run(format!("重算失败：{e}"));
                        break;
                    }
                }
            }
            note.set(if failed > 0 {
                format!("扫描 {scanned} 条，改写 {updated} 条，{failed} 条失败")
            } else {
                format!("扫描 {scanned} 条，改写 {updated} 条")
            });
            running.set(false);
            on_done.run(());
        });
    };

    view! {
        <div
            class="row wrap"
            data-testid="rescore-panel"
            style="gap:10px;padding:8px 12px;border-radius:12px;background:var(--glass-input);border:1px solid var(--hairline)"
        >
            <span style="font-size:12px;color:var(--text-3);max-width:260px">
                "按当前口径重算全库分数。规则重算不联网；开启大模型后每条都会真实计费。"
            </span>
            <label
                class="switch"
                style=move || if llm_configured { "opacity:1" } else { "opacity:.45" }
            >
                <input
                    type="checkbox"
                    data-testid="rescore-llm-toggle"
                    prop:checked=move || use_llm.get() && llm_configured
                    disabled=move || running.get() || !llm_configured
                    on:change=move |ev| use_llm.set(event_target_checked(&ev))
                />
                "用大模型"
            </label>
            <span class="card-sub" data-testid="rescore-note">
                {move || {
                    let n = note.get();
                    if n.is_empty() { "尚未重算".to_string() } else { n }
                }}
            </span>
            <button
                class="btn btn-sm"
                data-testid="rescore-btn"
                disabled=move || running.get() || total.get() == 0
                on:click=run_all
            >
                {move || if running.get() { "重算中…" } else { "全量重算" }}
            </button>
        </div>
    }
}
