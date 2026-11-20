//! 敏感库卡片网格（图 1 的下半部分）。
//!
//! 纯渲染：数据、选中集合与动作都由 `panel.rs` 持有并传进来——
//! 卡片自己不该知道「移出」要调哪个命令。

use leptos::prelude::*;

use super::state::{is_stored, last_line, middle_line, platform_label, status_label, time_text};
use crate::api::types::SensitiveRecord;
use crate::components::widgets::Chip;

#[component]
pub fn SensitiveList(
    items: Signal<Vec<SensitiveRecord>>,
    selected: Signal<Vec<i64>>,
    on_toggle: Callback<i64>,
    on_remove: Callback<i64>,
) -> impl IntoView {
    view! {
        <div class="sensitive-grid" data-testid="sensitive-grid">
            <For
                each=move || items.get()
                key=|row| row.id
                children=move |row: SensitiveRecord| {
                    let id = row.id;
                    let stored = is_stored(&row);
                    let status_cls = if stored {
                        "chip chip-primary sensitive-chip-status"
                    } else {
                        "chip chip-bad sensitive-chip-status"
                    };
                    let platform = platform_label(&row.platform);
                    let company = row.company.clone().unwrap_or_default().trim().to_string();
                    let company = if company.is_empty() { "（无公司名）".to_string() } else { company };
                    let salary = row.salary.clone().unwrap_or_default().trim().to_string();
                    let middle = middle_line(&row);
                    let last = last_line(&row);
                    let stamp = time_text(row.created_at);
                    let title_attr = {
                        let mut parts = vec![company.clone(), row.title.clone().unwrap_or_default()];
                        if let Some(url) = row.job_url.clone() {
                            if !url.is_empty() {
                                parts.push(url);
                            }
                        }
                        parts.join(" · ")
                    };
                    let item_testid = format!("sensitive-item-{id}");
                    let check_testid = format!("sensitive-check-{id}");
                    let remove_testid = format!("sensitive-remove-{id}");
                    view! {
                        <article class="sensitive-card" data-testid=item_testid title=title_attr>
                            <div class="sensitive-card-row">
                                <input
                                    type="checkbox"
                                    class="sensitive-check"
                                    data-testid=check_testid
                                    prop:checked=move || selected.get().contains(&id)
                                    on:change=move |_| on_toggle.run(id)
                                />
                                <Chip>{platform}</Chip>
                                <span class=status_cls>{status_label(&row)}</span>
                                <span class="sensitive-company" title=company.clone()>{company.clone()}</span>
                                <span class="spacer"></span>
                                <span class="sensitive-salary">{salary}</span>
                            </div>
                            <div class="sensitive-card-row">
                                <span class=if stored {
                                    "sensitive-job"
                                } else {
                                    "sensitive-job is-rejected"
                                }>{middle}</span>
                            </div>
                            <div class="sensitive-card-row">
                                <span class=if stored {
                                    "sensitive-note"
                                } else {
                                    "sensitive-note is-rejected"
                                }>{last}</span>
                            </div>
                            <div class="sensitive-card-row sensitive-card-foot">
                                <span class="sensitive-time">{stamp}</span>
                                <span class="spacer"></span>
                                <button
                                    class="btn btn-sm"
                                    data-testid=remove_testid
                                    on:click=move |_| on_remove.run(id)
                                >
                                    "移出"
                                </button>
                            </div>
                        </article>
                    }
                }
            />
        </div>
    }
}
