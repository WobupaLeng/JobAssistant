//! 设计系统：与 `skeleton-web/src/components/ui/index.tsx` 一一对应。
//!
//! CSS 类名原样沿用（`card` / `card-head` / `chip` / `stat` / `btn` …），
//! 因为 `skeleton-web/src/styles/` 的 1188 行样式表会整体搬到新前端复用。
//! 类名一旦改动，视觉就会出现偏差且难以逐像素比对。

pub mod shell;
pub mod widgets;
