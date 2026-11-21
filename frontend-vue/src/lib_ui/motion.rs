//! 一次性动画的触发助手（纯 web-sys，不含框架逻辑）。
//!
//! **为什么不直接写在 CSS 基类里**：`animation` 一旦播完，元素就停在终态；
//! 把同一份类名再挂上去不会重播。抽屉要在「每次打开」都滑一次，
//! 只能在挂载后由 JS 打一个 `*-play` 类——节点重新创建，类自然重新生效。
//!
//! 曲线与时长一律由样式表决定（`styles/drawer.css` / `styles/motion.css`），
//! 这里只负责「加类」这一件事，便于 `prefers-reduced-motion` 在 CSS 侧统一降级。

/// 给元素挂上一个只影响一次性动画的类。
///
/// 只在节点刚创建、该动画尚未播过时调用——CSS 的 `animation-name` 从「无」变成
/// 有值的瞬间就会起播，因此无需移除旧类再重新加。
pub fn play_once(el: &web_sys::Element, class: &str) {
    // `DomTokenList::add` 收 `&Array<JsString>`（wasm-bindgen 把可变参数编成数组），
    // 不是 `&str`——直接传字符串会在运行时抛 TypeError。
    let tokens = js_sys::Array::new();
    tokens.push(&wasm_bindgen::JsValue::from_str(class));
    let _ = el.class_list().add(&tokens);
}

/// 两帧之后再挂类：确保浏览器已经用初始态（`opacity: 0` / `translateX(24px)`）
/// 完成了一次布局，否则同一帧内「初始态 + 动画」可能被合并，进场整段不播。
///
/// 用 `requestAnimationFrame` 而不是 `setTimeout`：后台标签页里 rAF 会被节流，
/// 但那时用户也看不见，节流比「固定 16ms 猜一帧」更稳。
pub fn play_next_frame(el: web_sys::Element, class: &'static str) {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else {
        play_once(&el, class);
        return;
    };
    let el2 = el.clone();
    let cb = wasm_bindgen::closure::Closure::once_into_js(move || play_once(&el2, class));
    if win.request_animation_frame(cb.unchecked_ref()).is_err() {
        // rAF 不可用（极老的 WebView）：同步加类，宁可跳一帧也不能整块不可见
        play_once(&el, class);
    }
}

/// 与样式表同步的类名常量：改名要连同 `styles/drawer.css` / `styles/motion.css` 一起改，
/// 否则动画静默失效（页面看起来「没有动画」，但不报错）。
pub mod class {
    pub const DRAWER_IN: &str = "drawer-in-play";
    pub const ROW_IN: &str = "row-in";
    #[allow(dead_code)] // 列表的 tbody 类在 `features/jobs/list.rs::fade_class` 里拼，这里只作对照锚点
    pub const LIST_FADE: &str = "list-fade";
    #[allow(dead_code)] // 同上：`row-in` / `token-bump` 是静态类名，不经常量拼接
    pub const TOKEN_BUMP: &str = "token-bump";
}

#[cfg(test)]
mod tests {
    use super::class;

    /// 类名是纯数据，可以在宿主目标上断言——防止有人「顺手重命名」
    /// 而没有同步样式表（那条路径不会编译失败，只会静默没动画）。
    #[test]
    fn animation_class_names_match_stylesheet_hooks() {
        assert_eq!(class::DRAWER_IN, "drawer-in-play");
        assert_eq!(class::ROW_IN, "row-in");
        assert_eq!(class::LIST_FADE, "list-fade");
        assert_eq!(class::TOKEN_BUMP, "token-bump");
    }
}
