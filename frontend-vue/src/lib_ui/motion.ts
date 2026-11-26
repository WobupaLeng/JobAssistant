/**
 * 一次性动画的触发助手（纯 DOM，不含框架逻辑）。`animation` 播完元素停在终态，同一份类名
 * 再挂上去不会重播，所以由 JS 在挂载后打一个 `*-play` 类；曲线与时长归样式表，这里只加类，
 * 好让 `prefers-reduced-motion` 在 CSS 侧统一降级。类名不能写进模板的静态 `class`。
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
 * 给元素挂上一个只影响一次性动画的类。只在节点刚创建、动画尚未播过时调用——CSS 的
 * `animation-name` 从无到有就会起播。
 */
export function playOnce(el: Element, cls: string): void {
  el.classList.add(cls);
}

/**
 * 重播一次性动画：值跳变（分数 / token 累计）用，节点不重建也要再播一次。
 * 类已经挂在元素上时再 `add` 不会起播，必须「摘类 → 强制重排 → 挂回」，让计算样式真正
 * 回落到无动画态。`offsetWidth` 只为触发重排，读到的值不用。
 */
export function replayOnce(el: HTMLElement, cls: string): void {
  el.classList.remove(cls);
  void el.offsetWidth;
  el.classList.add(cls);
}

/**
 * 两帧之后再挂类：确保浏览器已经用初始态完成一次布局，否则同一帧内「初始态 + 动画」会被
 * 合并，进场整段不播。用 `requestAnimationFrame` 而不是固定延时：后台标签页会被节流，
 * 但那时用户也看不见。
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
