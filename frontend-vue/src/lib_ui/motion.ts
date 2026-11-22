/**
 * 一次性动画的触发助手（纯 DOM，不含框架逻辑）。
 *
 * **为什么不直接写在 CSS 基类里**：`animation` 一旦播完，元素就停在终态；
 * 把同一份类名再挂上去不会重播。抽屉要在「每次打开」都滑一次，
 * 只能在挂载后由 JS 打一个 `*-play` 类——节点重新创建，类自然重新生效。
 *
 * 曲线与时长一律由样式表决定（`styles/drawer.css` / `styles/motion.css`），
 * 这里只负责「加类」这一件事，便于 `prefers-reduced-motion` 在 CSS 侧统一降级。
 *
 * Vue 侧的常见坑：不要把这四个类名写进模板的静态 `class` 属性——
 * 由 JS 在 `onMounted` 里加，否则首帧就带着类，动画会被合并掉不播。
 */

/**
 * 与样式表同步的类名常量：改名要连同 `styles/drawer.css` / `styles/motion.css`
 * 一起改，否则动画静默失效（页面看起来「没有动画」，但不报错）。
 */
export const CLASS = {
  DRAWER_IN: 'drawer-in-play',
  ROW_IN: 'row-in',
  LIST_FADE: 'list-fade',
  TOKEN_BUMP: 'token-bump',
} as const;

/**
 * 给元素挂上一个只影响一次性动画的类。
 *
 * 只在节点刚创建、该动画尚未播过时调用——CSS 的 `animation-name` 从「无」变成
 * 有值的瞬间就会起播，因此无需移除旧类再重新加。
 */
export function playOnce(el: Element, cls: string): void {
  el.classList.add(cls);
}

/**
 * 两帧之后再挂类：确保浏览器已经用初始态（`opacity: 0` / `translateX(24px)`）
 * 完成了一次布局，否则同一帧内「初始态 + 动画」可能被合并，进场整段不播。
 *
 * 用 `requestAnimationFrame` 而不是 `setTimeout`：后台标签页里 rAF 会被节流，
 * 但那时用户也看不见，节流比「固定 16ms 猜一帧」更稳。
 *
 * 注意（AGENTS.md C16）：WebView 被系统挂起时 rAF 同样不触发，恢复后才补播——
 * 这是可接受的：它只影响动画，不影响任何状态。
 */
export function playNextFrame(el: Element, cls: string): void {
  const raf =
    typeof globalThis.requestAnimationFrame === 'function'
      ? globalThis.requestAnimationFrame.bind(globalThis)
      : null;
  if (!raf) {
    // rAF 不可用（极老的 WebView / 单测环境）：同步加类，宁可跳一帧也不能整块不可见
    playOnce(el, cls);
    return;
  }
  raf(() => playOnce(el, cls));
}
