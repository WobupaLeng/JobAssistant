//! 可取消的 `sleep` + 单线程 `spawn`（`setTimeout` 的 `async` 包装）。
//!
//! 为什么不是 `setInterval` + 自引用句柄：自动投递的循环是一个 `loop { … await }`，
//! 用 `setInterval` 得在闭包里再持有一个能被自己取消的 id，写法绕且容易泄漏闭包。
//! Promise 版每轮新建一个 `Timeout`，`await` 结束后自然丢弃。
//!
//! 行为契约：**WebView 挂起时 `setTimeout` 不触发**，`await` 就永远悬着——
//! 循环随之自然停止。这是已知且接受的行为（见任务书），不要改成
//! 用 `Date.now()` 追赶补发的方式绕开它。

use wasm_bindgen::prelude::*;

/// 睡 `ms` 毫秒。
pub async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let ok = web_sys::window()
            .map(|w| {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
                    .is_ok()
            })
            .unwrap_or(false);
        if !ok {
            // 拿不到 window 时不能把循环永久挂着：立即放行，下一个 tick 会重试。
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// 启动一个**不要求 `Send`** 的后台任务。
///
/// `leptos::task::spawn_local` 在当前版本的签名上要求 future 可选地跨线程搬运，
/// 而自动投递的循环持有 `Callback` 与 `Rc`，在 WASM 单线程里本来就只能跑在
/// 当前线程。这里直接用 `wasm_bindgen_futures::spawn_local`（纯 JS 微任务，
/// 无 `Send` 约束），语义与 Leptos 的运行时无关——循环只碰信号与 `invoke`。
pub fn spawn_detached<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    wasm_bindgen_futures::spawn_local(fut);
}
