/**
 * 弹层展开 / 收起的 Vue 侧封装：把「挂展开类、关的时候立刻收」收成一句。
 *
 * 纯逻辑（类名、回流、阈值）都在 `pan.ts`，这里只负责 ref 绑定与组件卸载时的清理，
 * 保持 components 层可以直接用、又不必各自重复一遍时序细节。
 *
 * **收起不排队**：用户明确要求「关闭有延时」不能再出现，所以关闭一律「点击即刻执行」，
 * 不挂 `.is-closing`、不排计时器、不等 `transitionend`。展开仍有补间（由 `pan-open` 提供）。
 */
import { getCurrentInstance, onBeforeUnmount, ref } from 'vue';
import type { Ref } from 'vue';
import { PAN_CLASS, playOpen, resetPanel } from './pan';

/**
 * @param onClosed 点击关闭那一下要执行的「真关」动作（通常是 `emit('close')`），**同步**执行。
 * @param onOpened 每次拿到新节点后补一帧挂展开类；默认由本封装代劳。
 */
export function useDrawerPresence(
  onClosed: () => void,
  onOpened?: () => void,
): {
  panel: Ref<HTMLElement | null>;
  animateOpen: () => void;
  animateClose: () => void;
} {
  const panel = ref<HTMLElement | null>(null);
  /** 组件已卸载就别再让已排队的动作改写父级状态。 */
  let alive = true;
  if (getCurrentInstance() !== null) onBeforeUnmount(() => (alive = false));

  /** 打开后由模板 `ref` 拿到节点时调一次；节点还没挂上则静默跳过。 */
  function animateOpen(): void {
    playOpen(panel.value, PAN_CLASS.DRAWER);
    onOpened?.();
  }

  /** 收起：同步执行 onClosed，父级当帧就把面板摘掉。 */
  function animateClose(): void {
    if (!alive) return;
    onClosed();
  }

  onBeforeUnmount(() => resetPanel(panel.value));

  return { panel, animateOpen, animateClose };
}

export { PAN_CLASS };