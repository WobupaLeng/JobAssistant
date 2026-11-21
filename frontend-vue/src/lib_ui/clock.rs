//! 时钟的**唯一**读口：当前毫秒时间戳。
//!
//! 为什么单独一个文件而不是让 `time.rs` 直接调 `js_sys::Date`：
//! `wasm-bindgen` 生成的 JS 胶水在 `cargo test`（host 目标）下调任何 import
//! 都会 `panic!`，而 `frontend-vue/Cargo.toml` 没有 `panic = "abort"`，
//! 这个 panic 在 wasm-bindgen 的 `__wbindgen_throw` 里**跨过未展开帧**，
//! 进程直接 `STATUS_STACK_BUFFER_OVERRUN`（0xc0000409）——不是测试失败，
//! 是整条 `cargo test` 崩掉，后面的用例根本没跑。
//!
//! 所以：**格式化函数一律接收时间戳入参**（纯函数，见 `time.rs` / `logfmt.rs`），
//! 只有真正需要「现在」的界面代码才调这里的 [`now_ms`]。这样 host 目标上的
//! 单测永远碰不到 wasm 胶水，也不需要 `#[cfg(target_arch = "wasm32")]` 去藏问题。
//!
//! 本模块**不依赖 leptos / api**（`lib_ui` 铁律），只用 `js_sys`。

/// 当前时间的 Unix 毫秒（与 `Date.now()` 同源）。
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(test)]
mod tests {
    /// `now_ms` 是唯一碰 wasm 胶水的函数，所以**只在 wasm 目标上测**。
    ///
    /// 这里的 `cfg` 不是「把问题藏起来」：被测对象就是「读浏览器时钟」这一
    /// 副作用本身，宿主目标上没有 `Date` 可读。业务格式化逻辑（`time.rs` /
    /// `logfmt.rs`）已全部纯函数化，在宿主目标上覆盖，不依赖本用例。
    #[cfg(target_arch = "wasm32")]
    #[test]
    fn now_ms_is_a_plausible_epoch_millis() {
        let v = super::now_ms();
        // 2020-01-01 ~ 2100-01-01，防止单位写成秒
        assert!(v > 1_577_836_800_000.0 && v < 4_102_444_800_000.0, "got {v}");
    }
}
