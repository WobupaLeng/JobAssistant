/**
 * 无边框窗口的拖动兜底（用户 m04113「窗口顶部空白区域不能拖到窗口」）。
 *
 * ## 为什么需要这一层
 *
 * 窗口是 `decorations: false` + `transparent: true`，没有系统标题栏，拖动完全靠
 * Tauri 的 `data-tauri-drag-region` 属性。而它的 drag 逻辑挂在**被点到的那一个元素**
 * 上，事件不会从子节点冒泡到祖先 —— 顶栏里每一块可视物件（品牌名、平台卡、打分分段、
 * token 读数、配置 / 敏感库 / 主题、三个窗口键）都把 pointerdown 抢走，
 * 结果「顶栏看着大片空白，按下去却拖不动」。
 *
 * ## 做法
 *
 * 捕获阶段（早于 Tauri 的监听）先判真假：
 * - 命中 button / a / input / select / textarea / label ⇒ 是可交互控件，原行为不动；
 * - 其余命中且不在排除区（默认抽屉面板、模态、弹出层）⇒ 给它补上 drag 属性。
 *
 * ## 两处 WebView2 的反直觉行为
 *
 * 1. **属性置空串等于没有这个属性**：`setAttribute(DRAG, '')` 在 WebView2 里取 `Attribute`
 *    会返回 null，插件据此判定「不是拖动区」。所以停用一律用 removeAttribute。
 * 2. **上一次拖动的「按下」永远等不到 mouseup**：拖动走的是系统 modal loop，
 *    后端 `window_start_dragging` 返回时手势已经结束。残留的按下状态会让**下一次**
 *    单击被当成「从上次落点拖到这里」，窗口直接跳走。这里自己记账：
 *    上一次拖动期间收到过 pointermove ⇒ 下一次 pointerdown 直接吞掉。
 */

/** 与 Tauri 约定的属性名（`data-tauri-drag-region`）。 */
export const DRAG_ATTR = 'data-tauri-drag-region';

/** 可交互控件：命中这些就交还原行为，不冒充拖动区。 */
const INTERACTIVE = 'button, a, input, select, textarea, label, [contenteditable="true"]';

/**
 * 排除区：抽屉 / 模态 / 弹出面板里空白处按下去应该是「什么都不发生」或「关闭弹层」，
 * 拖窗口会与既有的「点外部关闭」打架。
 */
const EXCLUDED = '[data-overlay], .drawer, .filter-panel, .sensitive-panel, .config-modal, .decisions-modal, .theme-panel, .city-picker-panel';

/** 顶层容器的命中范围：只有应用骨架内部才补属性（body / html 本身不补，那里已是拖动区）。 */
const SURFACES = '.app-shell';

interface DragState {
  /** 上一次拖动是否还没等到 pointerup（WebView2 下通常永远等不到）。 */
  inFlight: boolean;
  /** 本次按下之后是否发生过真实的位移。 */
  moved: boolean;
}

function hostOf(target: EventTarget | null): HTMLElement | null {
  if (target instanceof HTMLElement) return target;
  if (target instanceof Text) return target.parentElement;
  return null;
}

/**
 * 挂上拖动兜底。返回一个卸载函数（单测里用来还原监听，应用里常年不摘）。
 *
 * @param root 监听挂在哪：应用里是 `document`（要捕获阶段），单测可传容器。
 */
export function installDragAllow(root: Document | HTMLElement = document): () => void {
  const state: DragState = { inFlight: false, moved: false };

  const onDown = (ev: PointerEvent): void => {
    const host = hostOf(ev.target);
    if (host === null) return;
    if (host.closest(SURFACES) === null) return;
    if (host.closest(EXCLUDED) !== null) return;
    state.moved = false;
    // 真控件：原行为（点按钮、点输入框），顺手把上次可能留下的冒充属性收掉。
    if (host.closest(INTERACTIVE) !== null) {
      if (host.hasAttribute(DRAG_ATTR)) host.removeAttribute(DRAG_ATTR);
      return;
    }
    if (state.inFlight) {
      // 上一次拖动的收尾没回来：这一下不该成为新起点，否则窗口会瞬移。
      ev.preventDefault();
      ev.stopPropagation();
      return;
    }
    state.inFlight = true;
    // 只有「没有」时才补：品牌栏等原生拖动区自己已经带属性，别重复写。
    if (!host.hasAttribute(DRAG_ATTR)) host.setAttribute(DRAG_ATTR, '');
  };

  const onMove = (): void => {
    if (state.inFlight) state.moved = true;
  };

  const onUp = (): void => {
    // 拖过的收不到 up（系统 modal loop），没收到的由下一次 pointerdown 兜；
    // 没拖过就是一次普通单击，当场结束记账。
    if (!state.moved) state.inFlight = false;
  };

  const cap = { capture: true } as const;
  root.addEventListener('pointerdown', onDown as EventListener, cap);
  root.addEventListener('pointermove', onMove, cap);
  root.addEventListener('pointerup', onUp, cap);

  return () => {
    root.removeEventListener('pointerdown', onDown as EventListener, cap);
    root.removeEventListener('pointermove', onMove, cap);
    root.removeEventListener('pointerup', onUp, cap);
  };
}