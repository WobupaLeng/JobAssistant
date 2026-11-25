/**
 * 弹层 / 抽屉的展开与收起：一个状态机，一处类名来源。退场在纯 CSS 里补不了（节点一摘或
 * `display:none` 一翻转就拿不到中间帧），所以要让节点多活一帧：先挂收起态类，等过渡结束
 * 或超时兜底再摘。只碰 `transform`，不做透明度。
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
 * 挂上展开动画类，只在「打开的那一帧」调用。浏览器只在 `animation-name` 变化时重播，
 * 所以常驻节点要先摘再挂，中间垫一次强制回流。
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
 * 开始收起：挂 `.is-closing`，返回在过渡结束（或超时兜底）时兑现的 Promise。调用方
 * `await` 之后再摘节点。元素已不在文档里时直接兑现——整块卸载时 `transitionend` 不会来。
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
