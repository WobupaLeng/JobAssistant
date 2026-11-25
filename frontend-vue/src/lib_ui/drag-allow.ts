/**
 * 无边框窗口的拖动兜底：`data-tauri-drag-region` 只认被点到的那个元素且不冒泡，顶栏里的可点
 * 物件会把 pointerdown 抢走。做法是在捕获阶段（早于 Tauri 的监听）命中可交互控件就交还原行为。
 * 属性写成空串等于没写，停用要 removeAttribute；残留按下的拖动会让下次单击被误判。
 */

/** 与 Tauri 约定的属性名（`data-tauri-drag-region`）。 */
export const DRAG_ATTR = 'data-tauri-drag-region';

/** 可交互控件：命中这些就交还原行为，不冒充拖动区。 */
const INTERACTIVE = 'button, a, input, select, textarea, label, [contenteditable="true"]';

/**
 * 排除区：浮层里按下应是「什么都不发生」或「关闭弹层」，不能拖窗口。
 * **只排除盒子本身，不排除它的子树**：`.drawer` 是所有浮层的容器，用 closest
 * 会把整块界面判成排除区，兜底一律不生效。
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
    if (host.matches(EXCLUDED)) return;
    // 真控件：原行为（点按钮、点输入框），顺手把上次可能留下的冒充属性收掉。
    if (host.closest(INTERACTIVE) !== null) {
      if (host.hasAttribute(DRAG_ATTR)) host.removeAttribute(DRAG_ATTR);
      return;
    }
    if (state.inFlight) {
      // 上一次拖动的收尾没回来：这一下不该成为新起点，否则窗口会瞬移。
      // **必须在这里把账结掉**，只拦这一次，否则窗口再也拖不动。
      state.inFlight = false;
      state.moved = false;
      return;
    }
    state.moved = false;
    state.inFlight = true;
    // 属性必须写在**按下那一刻命中的元素**上（Tauri 只看 `ev.target` 那个节点），
    // 而且要保证生效值非空：WebView2 里 `setAttribute(D, '')` 取回来是 null，
    // 插件会判「不是拖动区」。所以先写成 `'true'`。
    if (host.getAttribute(DRAG_ATTR) === null || host.getAttribute(DRAG_ATTR) === '') {
      host.setAttribute(DRAG_ATTR, 'true');
    }
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
