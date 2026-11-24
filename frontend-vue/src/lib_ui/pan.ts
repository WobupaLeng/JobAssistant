/**
 * 弹层 / 抽屉的展开与收起：一个状态机，一处类名来源。
 *
 * 为什么需要它：收起（退场）在纯 CSS 里补不了——面板一从 DOM 摘掉或 `display:none`
 * 一翻转，浏览器就再也拿不到中间帧。想让「收起」和「展开」一样有过渡，只
 * 能让节点多活一帧：先挂收起态类，等过渡真正结束再摘。这个「多活一帧」的时序
 * 就是本文件唯一要做的事。
 *
 * 约定（与 styles/motion.css 第 1、6、7 段严格对应）：
 *   - 展开：`.pan-pop`（浮层）/ `.pan-modal`（居中大窗）/ `.drawer-anim`（抽屉）
 *     三者时长都是 `--pan-dur` / `--drawer-dur`，曲线同一条 `--ease-out`。
 *   - 收起：`.drawer-anim` 与 `.is-closing` 同挂，`transform` 由 `.is-closing` 的
 *     目标值补间回去；过渡结束（`transitionend`）或超时兜底后摘掉 `.is-closing`。
 *   - 只碰 transform，不做透明度（产品要求：不要淡入淡出）。
 *
 * 纯 DOM，不依赖 Vue 运行时；组件侧包一层 `useDrawerPresence` 即可。
 */

/** 与 styles/motion.css 同步的类名常量：改名必须连样式一起改，否则动画静默失效。 */
export const PAN_CLASS = {
  /** 浮层：主题面板 / 城市选择器 / 字号与主题菜单。 */
  POP: 'pan-pop',
  /** 居中大窗：配置 / 决策记录。 */
  MODAL: 'pan-modal',
  /** 抽屉：岗位详情 / 条件筛选 / 敏感库。 */
  DRAWER: 'drawer-anim',
  /** 收起中的一帧：过渡目标写在 motion.css 的 `.is-closing` 上。 */
  CLOSING: 'is-closing',
} as const;

/** 与 `--drawer-dur`（320ms）对齐，多留一档作为兜底。收起的「延时」就是这个数。 */
export const PAN_EXIT_MS = 360;

/**
 * 挂上展开动画类。
 *
 * 只在「打开的那一帧」调用：节点若刚刚创建，首帧即带类，关键帧从 from 起播；
 * 节点若常驻（v-show 切换），类从无到有同样会重新起播——浏览器只在
 * `animation-name` 变化时才重播，所以必须先摘再挂，中间垫一次强制回流。
 */
export function playOpen(el: Element | null, cls: string): void {
  if (el === null) return;
  const node = el as HTMLElement;
  node.classList.remove(cls);
  // 强制回流：同一 tick 内摘了又挂，不回流浏览器只看得到最终态，关键帧不会重播。
  void node.offsetWidth;
  node.classList.add(cls);
}

/**
 * 开始收起：挂 `.is-closing`，返回一个在过渡结束（或超时兜底）时兑现的 Promise。
 *
 * 调用方拿到 Promise 之后再真正把节点从 DOM 摘掉，形如：
 *   await closePanel(el);
 *   open.value = false;
 *
 * 拒绝路径：元素已不在文档里（`isConnected === false`）时直接兑现，不等超时——
 * 面板被整块卸载时 `transitionend` 永远不会来。
 */
export function closePanel(el: Element | null, timeoutMs = PAN_EXIT_MS): Promise<void> {
  if (el === null || !el.isConnected) return Promise.resolve();
  const node = el as HTMLElement;
  node.classList.add(PAN_CLASS.CLOSING);

  return new Promise<void>((resolve) => {
    let done = false;
    const finish = (): void => {
      if (done) return;
      done = true;
      node.removeEventListener('transitionend', onEnd);
      window.clearTimeout(timer);
      resolve();
    };
    // 只认 transform 那一条：内部元素的其它过渡会冒泡上来，误判会提前摘面板。
    const onEnd = (e: Event): void => {
      if (e.target === node && (e as TransitionEvent).propertyName === 'transform') finish();
    };
    node.addEventListener('transitionend', onEnd);
    // 兜底：`prefers-reduced-motion` 下过渡被关掉，transitionend 不一定来。
    const timer = window.setTimeout(finish, timeoutMs);
  });
}

/** 收起结束后的清场：把收起态类摘掉，下次展开才不会从「已收起」起播。 */
export function resetPanel(el: Element | null): void {
  if (el === null) return;
  el.classList.remove(PAN_CLASS.CLOSING);
}
