/**
 * 全景几何：`--ui-scale` 与 `--rail-w` / `--detach-w` 的运行时写入。
 * 先写 `--ui-scale` 再写宽度：将来若有 `calc(... * var(--ui-scale))`，顺序就是唯一的保证。
 * `compute*` 是纯函数不碰 DOM，只有 `syncCanvas()` 写行内样式。
 */

/** 设计基准宽度。 */
export const UI_BASE_WIDTH = 1920;
/** 缩放上限：`--ui-scale = clamp(1, innerWidth / 1920, 1.5)`。 */
export const UI_SCALE_MAX = 1.5;
/** 缩放精度：原型 `Math.round(s * 1000) / 1000`。 */
const SCALE_PRECISION = 1000;

/**
 * 右栏宽基准：≥1360 为 360，以下收窄到 280，两档都乘 `--ui-scale`（与
 * `styles/vue-chat.css` 的 media 同源，`tests/geometry.test.ts` 核对两边）。分档点 1360 是
 * 「50vw − 360 = 320」的临界点，再窄正文区就压不到舒适下限 320。
 */
export const RAIL_W_WIDE = 360;
export const RAIL_W_NARROW = 280;
export const RAIL_NARROW_BELOW = 1360;

/** 对话框宽：`clamp(600px, 50vw, 80vw)`。 */
export const DETACH_MIN_PX = 600;
export const DETACH_VW = 0.5;
export const DETACH_MAX_VW = 0.8;

/**
 * 三窗高度基准与视口留白，公式 height: min(calc(<base>px * var(--ui-scale)), calc(100vh -
 * <gap>px))。写成常量是为了让 `tests/geometry.test.ts` 断言产物 CSS 的数字与之相同。
 */
export const MODAL_BASE_HEIGHT = { config: 640, sensitive: 720, decisions: 620 } as const;
export const MODAL_VIEWPORT_GAP = { config: 96, sensitive: 56, decisions: 96 } as const;

/**
 * `--ui-scale = clamp(1, innerWidth / 1920, 1.5)`，保留 3 位小数：
 * 只放大不缩小，非法输入一律回 1。
 */
export function computeUiScale(innerWidth: number): number {
  if (!Number.isFinite(innerWidth)) return 1;
  const raw = Math.min(Math.max(innerWidth / UI_BASE_WIDTH, 1), UI_SCALE_MAX);
  return Math.round(raw * SCALE_PRECISION) / SCALE_PRECISION;
}

/** 右栏宽（已乘 scale）：基准只看窗口宽度，缩放再乘上去。 */
export function computeRailWidth(innerWidth: number, scale = computeUiScale(innerWidth)): number {
  const base = innerWidth < RAIL_NARROW_BELOW ? RAIL_W_NARROW : RAIL_W_WIDE;
  return Math.round(base * scale);
}

/**
 * 对话框宽（**不乘 scale**）：`round(min(max(innerWidth * 0.5, 600), innerWidth * 0.8))`。
 * 600 下限是 CSS 常量，缩放只作用于内部像素常量、不改变百分比关系。
 */
export function computeDetachWidth(innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) return DETACH_MIN_PX;
  return Math.round(Math.min(Math.max(innerWidth * DETACH_VW, DETACH_MIN_PX), innerWidth * DETACH_MAX_VW));
}

/**
 * 写一次全景几何：先 `--ui-scale`，再 `--rail-w` / `--detach-w`。
 * 行内样式只在同一个元素上优先，`styles/vue-chat.css` 里 `:root` 的同名值是 JS 未跑时的兜底；
 * 兜底一旦挂到 `.app-body` 这类更近的祖先，会把行内值整棵遮住（自定义属性是继承的）。
 */
export function syncCanvas(innerWidth: number): { scale: number; railWidth: number; detachWidth: number } {
  const scale = computeUiScale(innerWidth);
  const railWidth = computeRailWidth(innerWidth, scale);
  const detachWidth = computeDetachWidth(innerWidth);

  const root = typeof document !== 'undefined' ? document.documentElement : null;
  if (root) {
    root.style.setProperty('--ui-scale', String(scale));
    root.style.setProperty('--rail-w', `${railWidth}px`);
    root.style.setProperty('--detach-w', `${detachWidth}px`);
  }
  return { scale, railWidth, detachWidth };
}
