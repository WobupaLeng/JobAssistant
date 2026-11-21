//! WASM 入口（bin target）。
//!
//! **为什么入口在 `main.rs` 而不是 `lib.rs`**：trunk 打包时找的是
//! cargo 产出的 **bin** wasm 文件（`<name>.wasm`），只有 `[lib]` 时
//! 它报 `cargo artifacts not found for target crate`。
//! 全部逻辑仍在库里，这里只做一次转发——保持 `lib.rs` 可被单元测试引用。

fn main() {
    aja_frontend::start();
}
