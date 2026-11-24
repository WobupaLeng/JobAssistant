/**
 * 全景几何：`--ui-scale` 与 `--rail-w` / `--detach-w` 的运行时写入。
 *
 * 两件事必须**按顺序**做（原型 `prototypes/ui-v1/index.html:3221-3226`）：
 *
 *   1. `syncUiScale()` 先写 `--ui-scale`；
 *   2. `syncDetachWidth()` 再写 `--detach-w`。
 *
 * 原型注释给的理由是「`--ui-scale` 变了会连带 `--rail-w` / `--detach-w` 的 px
 * 解释，必须在 `syncDetachWidth()` 之前写，否则会拿旧比例算一帧」。Vue 版两者
 * 都是纯 JS 算出的无单位数值，不存在解释顺序问题，但**顺序照抄**——将来谁把
 * `--detach-w` 改成 `calc(... * var(--ui-scale))` 时，就只剩这一条保证了。
 *
 * 纯函数（`computeUiScale` / `computeRailWidth` / `computeDetachWidth`）不碰 DOM，
 * 单测直接喂窗口宽度即可；`syncCanvas()` 才落地写行内样式。
 */

/** 设计基准宽度（用户裁决 K，v7）。 */
export const UI_BASE_WIDTH = 1920;
/** 缩放上限：`--ui-scale = clamp(1, innerWidth / 1920, 1.5)`。 */
export const UI_SCALE_MAX = 1.5;
/** 缩放精度：原型 `Math.round(s * 1000) / 1000`。 */
const SCALE_PRECISION = 1000;

/**
 * 右栏宽基准：≥1360 为 360，以下收窄到 280，两档都乘 `--ui-scale`
 * （与 `styles/vue-chat.css` 的 media 同源，`tests/geometry.test.ts` 核对两边）。
 *
 * 用户 m00933「增加 boss 列表宽度」：原 300 / 260 下，一条消息里的
 * 「摘要 · 时间」会被挤成两行、时间戳换行，纵向空出一截；360 让摘要与时间并排躺平。
 *
 * 为什么分档点从 1280 挪到 1360：右栏在对话框里占的是固定宽度，对话框本身是
 * `clamp(600, 50vw, 80vw)`。基准提到 360 后，1280 宽窗口下对话框只有 640px，
 * 聊天正文区会被压到 280px（规格 §2.2 的舒适下限是 320）。分档点取 1360 =
 * 「50vw − 360 = 320」的临界点，1280 这一档因此走收窄的 280，正文区回到 360。
 * 窗口最小宽本就是 1200，窄档只在 1200–1359 这段生效。
 */
export const RAIL_W_WIDE = 360;
export const RAIL_W_NARROW = 280;
export const RAIL_NARROW_BELOW = 1360;

/** 对话框宽：`clamp(600px, 50vw, 80vw)`，下限 600 是用户裁决（m00332）。 */
export const DETACH_MIN_PX = 600;
export const DETACH_VW = 0.5;
export const DETACH_MAX_VW = 0.8;

/**
 * 三窗固定尺寸（原型 `prototypes/ui-v1/index.html` L950 / L981 / L1249）。
 *
 * 宽度按原型保留（Vue 版各有自己的可用宽度约束），高度逐字照抄：
 *   config    height: min(calc(640px * var(--ui-scale)), calc(100vh - 96px))
 *   sensitive height: min(calc(720px * var(--ui-scale)), calc(100vh - 56px))
 *   decisions height: min(calc(620px * var(--ui-scale)), calc(100vh - 96px))
 * 写成常量是为了让 `tests/geometry.test.ts` 能断言「产物 CSS 里的数字 = 这里的数字」，
 * 改一处漏一处会被测试抓住。
 */
export const MODAL_BASE_HEIGHT = { config: 640, sensitive: 720, decisions: 620 } as const;
export const MODAL_VIEWPORT_GAP = { config: 96, sensitive: 56, decisions: 96 } as const;

/**
 * `--ui-scale = clamp(1, innerWidth / 1920, 1.5)`，保留 3 位小数。
 *
 * - ≤1920 → 恒为 1（**只放大不缩小**，1280 / 1440 / 1600 都是 1）
 * - ≥1920 → 线性放大（2560 → 1.333，3840 → 2.0 被 clamp 封顶 1.5）
 * - 非法输入（NaN / 负数 / 0）→ 1，保证样式变量永远拿得到正值
 */
export function computeUiScale(innerWidth: number): number {
  if (!Number.isFinite(innerWidth)) return 1;
  const raw = Math.min(Math.max(innerWidth / UI_BASE_WIDTH, 1), UI_SCALE_MAX);
  return Math.round(raw * SCALE_PRECISION) / SCALE_PRECISION;
}

/** 右栏宽（已乘 scale）：300 / 260 的选择只看窗口宽度，缩放再乘上去。 */
export function computeRailWidth(innerWidth: number, scale = computeUiScale(innerWidth)): number {
  const base = innerWidth < RAIL_NARROW_BELOW ? RAIL_W_NARROW : RAIL_W_WIDE;
  return Math.round(base * scale);
}

/**
 * 对话框宽（**不乘 scale**）：原型 `syncDetachWidth()` 是
 * `round(min(max(innerWidth * 0.5, 600), innerWidth * 0.8))`。
 *
 * 600 下限也不乘 scale——`clamp(600px, 50vw, 80vw)` 里的 600 是 CSS 常量，
 * 原型的 `--detach-w` 计算里同样没有 `* var(--ui-scale)`（L2368-2372），
 * 注释还特意写了「50vw 口径不变 —— 缩放只作用于内部像素常量，不改变百分比关系」。
 */
export function computeDetachWidth(innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) return DETACH_MIN_PX;
  return Math.round(Math.min(Math.max(innerWidth * DETACH_VW, DETACH_MIN_PX), innerWidth * DETACH_MAX_VW));
}

/**
 * 写一次全景几何：先 `--ui-scale`，再 `--rail-w` / `--detach-w`。
 *
 * 行内样式优先于样式表（**只在同一个元素上**才优先），所以这三个变量在运行期的真实来源就是这里；
 * `styles/vue-chat.css` 里 `:root` 上的同名字面值是「JS 还没跑」时的兜底。兜底一旦挂到
 * `.app-body` 这类更近的祖先，就会把这里的行内值整棵遮住（自定义属性是继承的）——
 * 槽位 C 产物冒烟实测到过：对话框因此只剩 1px。
 * 返回写进去的无单位数值，方便测试与调用方复用。
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
