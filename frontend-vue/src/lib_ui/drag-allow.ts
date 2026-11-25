/**
 * 无边框窗口的标题带：**只有窗口最上方 `TITLE_STRIP_HEIGHT` 高的一条横带**能拖窗口、
 * 双击最大化/还原，页面其它空白处一律不拖窗口。
 *
 * 做法：Tauri 的 `data-tauri-drag-region` 只认被点到的那一个元素且不冒泡，所以在捕获阶段
 * （早于 Tauri 的 document `mousedown` 监听）按几何位置把属性补到**命中元素**上
 * （属性值必须非空：WebView2 里 `setAttribute(D, '')` 取回来是 null，插件会判「不是拖动区」）。
 * 下一次按下先摘掉上一次的标记——属性留在节点上，那个节点就会永久变成拖动区，
 * 这也是「整窗都能拖」的老病灶。
 *
 * 双击由 Tauri 自带的 `drag.js` 处理：命中拖动区的 `mousedown` 且 `detail === 2` 时
 * 调 `internal_toggle_maximize`，所以这里**不再**自己发 toggleMaximize，否则一次双击翻两次。
 */

/** 与 Tauri 约定的属性名（`data-tauri-drag-region`）。 */
export const DRAG_ATTR = 'data-tauri-drag-region';

/** 标题带高度（CSS px）：窗口最上方 y=0 到 y=20 之间才是拖动 / 双击区。 */
export const TITLE_STRIP_HEIGHT = 20;

/** 可交互控件：命中这些就交还原行为（点按钮、点输入框），不冒充拖动区。 */
const INTERACTIVE = 'button, a, input, select, textarea, label, summary, [contenteditable="true"]';

/**
 * 排除区：浮层里按下应是「什么都不发生」或「关闭弹层」，不能拖窗口。
 * **只排除盒子本身，不排除它的子树**：`.drawer` 是所有浮层的容器，用 closest
 * 会把整块界面判成排除区，兜底一律不生效。
 */
const EXCLUDED = '[data-overlay], .drawer, .filter-panel, .sensitive-panel, .config-modal, .decisions-modal, .theme-panel, .city-picker-panel';

/** 顶层容器的命中范围：应用骨架之外（body / html 露出的边角）不挂拖动区。 */
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
 * 挂上标题带拖动兜底。返回一个卸载函数（单测里用来还原监听，应用里常年不摘）。
 *
 * @param root 监听挂在哪：应用里是 `document`（要捕获阶段），单测可传容器。
 */
export function installDragAllow(root: Document | HTMLElement = document): () => void {
  const state: DragState = { inFlight: false, moved: false };
  /** 上一次按下时被我们打成拖动区的元素（最多一个，卸载 / 换点都要摘掉）。 */
  let stamped: HTMLElement | null = null;

  const unstamp = (): void => {
    if (stamped === null) return;
    stamped.removeAttribute(DRAG_ATTR);
    stamped = null;
  };

  const onDown = (ev: PointerEvent): void => {
    const host = hostOf(ev.target);
    if (host === null) return;
    // 先清掉上一次的标记：它可能落在新位置（或已被移除）的节点上，不摘就成永久拖动区。
    unstamp();
    if (host.closest(SURFACES) === null) return;
    if (host.matches(EXCLUDED)) return;
    // 真控件：原行为（点按钮、点输入框）。标题带里的窗口控件也在这一档。
    if (host.closest(INTERACTIVE) !== null) return;
    // 几何判据：只有窗口最上方那 20px 才是标题带，页面其余空白处不拖窗口。
    if (ev.clientY >= TITLE_STRIP_HEIGHT) return;
    if (state.inFlight) {
      // 上一次拖动的收尾没回来：这一下不该成为新起点，否则窗口会瞬移。
      // **必须在这里把账结掉**，只拦这一次，否则窗口再也拖不动。
      state.inFlight = false;
      state.moved = false;
      return;
    }
    state.moved = false;
    state.inFlight = true;
    host.setAttribute(DRAG_ATTR, 'true');
    stamped = host;
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
    unstamp();
    root.removeEventListener('pointerdown', onDown as EventListener, cap);
    root.removeEventListener('pointermove', onMove, cap);
    root.removeEventListener('pointerup', onUp, cap);
  };
}