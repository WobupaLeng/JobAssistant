//! 配置页：装配 `features/config`。
//!
//! 首帧与 `Boot` 的三份一次性数据并行拉取；不受 `page_tick` 驱动
//! （`pages/mod.rs` 不传 page_tick 给本页，签名不可改）。

use leptos::prelude::*;

use crate::components::shell::Boot;
use crate::features::config::{ConfigPanel, ConfigState};

#[component]
pub fn ConfigPage(on_notice: Callback<String>) -> impl IntoView {
    let boot = use_context::<Boot>().expect("Boot 必须在 AppShell 注入");
    let fail = Callback::new(move |e: String| boot.error.set(Some(e)));

    let state = ConfigState::new();

    // 首帧拉一次。`Effect` 会在挂载后运行且只在这里带副作用。
    let kicked = StoredValue::new(false);
    Effect::new(move |_| {
        if kicked.get_value() {
            return;
        }
        kicked.set_value(true);
        state.reload(fail);
    });

    view! { <ConfigPanel state=state on_notice=on_notice fail=fail /> }
}
