/**
 * 长按多选的状态机。规则全在时间与位移边界上，放组件里没法测。`pointerdown` 起 500ms 定时器；
 * 期间 `pointerup` / `pointercancel` 或移动超过 8px（用户在滚动）即取消；到点才进入多选并勾上
 * 按下的那张卡。同一次按下若以 `pointercancel` 收场（触摸滚动、`touch-action` 接管手势），
 * 随后的 `click` 是这次长按的余波，必须吞掉，不能当成「单击打开详情」。
 */

/** 长按判定窗口（毫秒）。SPEC 定死 500。 */
export const LONG_PRESS_MS = 500;

/** 允许的抖动半径（像素）。超过就当成拖动/滚动。 */
export const LONG_PRESS_MOVE_TOLERANCE = 8;

/** 点在这些东西上不触发长按：按钮要能点、输入框要能聚焦。 */
const INTERACTIVE_SELECTOR = 'button, a, input, label, select, textarea, [data-greet]';

export interface PointerSample {
  readonly x: number;
  readonly y: number;
}

export interface SelectionState {
  readonly selecting: boolean;
  /** 已勾选的 job_id，保持点击顺序。 */
  readonly checked: readonly string[];
}

export function emptySelection(): SelectionState {
  return { selecting: false, checked: [] };
}

/** 距离判定用平方比较，省一次开方（同一循环里每帧都在问）。 */
export function movedBeyond(start: PointerSample, now: PointerSample, tolerance = LONG_PRESS_MOVE_TOLERANCE): boolean {
  const dx = now.x - start.x;
  const dy = now.y - start.y;
  return dx * dx + dy * dy > tolerance * tolerance;
}

/**
 * 事件目标是否该被长按逻辑忽略。传 tagName 与命中标记而不是 DOM 节点，单测不用造 DOM。
 */
export function ignorePointerTarget(tagName: string, matchedInteractive = false): boolean {
  if (matchedInteractive) return true;
  const tag = tagName.toLowerCase();
  return ['button', 'a', 'input', 'label', 'select', 'textarea'].includes(tag);
}

/** 深选择器字符串，供组件 `el.closest()` 用；与 `ignorePointerTarget` 保持同一口径。 */
export function interactiveSelector(): string {
  return INTERACTIVE_SELECTOR;
}

/** 进入多选并勾上按下的那张卡。 */
export function enterSelecting(jobId: string): SelectionState {
  return { selecting: true, checked: [jobId] };
}

/** 退出多选：**同时清空勾选**（残留勾选会让下一次长按带着旧选择）。 */
export function exitSelecting(): SelectionState {
  return emptySelection();
}

/**
 * 多选态下点了一张卡：切它；点完若一张不剩，直接退出多选。
 *
 * 非多选态点卡片不改选择（那是"展开详情"的工作）。
 */
export function clickCard(state: SelectionState, jobId: string): SelectionState {
  if (!state.selecting) return state;
  const has = state.checked.includes(jobId);
  const checked = has ? state.checked.filter((id) => id !== jobId) : [...state.checked, jobId];
  // 取消到一张不剩时**留在多选态**：用户正在逐张取消，最后一张被取消就自动退出多选，
  // 会让紧接着的下一次单击变成「打开详情」，观感是「点一下取消选中、再点一下弹窗」。
  // 真正的出口是空白处单击、切换页签或批量操作完成后（见 exitSelecting 的调用点）。
  return { selecting: true, checked };
}

/** 空白处单击：多选态下退出。 */
export function clickBlank(state: SelectionState): SelectionState {
  return state.selecting ? exitSelecting() : state;
}

/** 全选本页 / 取消全选本页。勾选顺序按传入的可见顺序，保证 diff 稳定。 */
export function setChecked(jobIds: readonly string[]): SelectionState {
  if (jobIds.length === 0) return exitSelecting();
  return { selecting: true, checked: [...jobIds] };
}

/**
 * 定时器句柄的极小封装：单测注入假定时器，避开 `setTimeout` 返回类型的差异
 * （浏览器 number / Node Timeout）。
 */
export interface TimerHandle {
  cancel(): void;
}

export function armLongPress(fire: () => void, ms = LONG_PRESS_MS): TimerHandle {
  const id = setTimeout(fire, ms);
  return {
    cancel(): void {
      clearTimeout(id);
    },
  };
}
