/**
 * Esc 分层表（冻结 SPEC §6.1 / 断言 A10 / SPEC-001 用户故事 12）。
 *
 * 用户逐字口径：**一次 `keydown` 只关最上面那一层**。
 * 从上往下排：决策记录 → 配置 → 岗位详情抽屉 / 敏感库 → 聊天对话框。
 *
 * 为什么放 `lib_ui/`：这张表被四个消费方共用（`pages/LogsPage`、`pages/ConfigPage`、
 * `components/JobDetailDrawer`、`pages/ChatPage` 同屏聊天层）。选择器在各自文件里
 * 抄一份就是四份真源，改一处漏三处正是「三条静默失效」里那一类静默回归。
 * 这里是纯函数：传进来的 `has` 只问「这个选择器在当前 DOM 里有没有命中」，
 * node 单测给一个假的 `has` 即可，不依赖 document。
 */

/** 表里的层标识，顺序即优先级（前 = 上）。 */
export type OverlayLayer = 'decisions' | 'config' | 'drawer' | 'sensitive' | 'chat';

/**
 * 各层「正开着」的判据。
 *
 * - `decisions` / `config` 是 v-if 挂载的浮层窗：DOM 里有 = 开着；
 * - `drawer` / `sensitive` 常驻 DOM，靠 `.is-open` 表达开合；
 * - `chat` 是对话框本身，不在「上层」里出现，故无选择器。
 */
export const OVERLAY_LAYERS: ReadonlyArray<{ readonly layer: OverlayLayer; readonly open: string | null }> = [
  { layer: 'decisions', open: '[data-testid="decisions-panel"]' },
  { layer: 'config', open: '[data-testid="config-panel"]' },
  { layer: 'drawer', open: '[data-testid="job-detail-rail"].is-open' },
  { layer: 'sensitive', open: '[data-testid="sensitive-panel"].is-open' },
  { layer: 'chat', open: null },
];

/**
 * `layer` 之上是否还有正开着的层。为真时**这一层不许回应 Esc**。
 *
 * @param layer 当前要回应 Esc 的层
 * @param has   「选择器是否命中」的判定，默认查全局 document
 */
export function hasHigherOverlay(
  layer: OverlayLayer,
  has: (selector: string) => boolean = defaultHas,
): boolean {
  const at = OVERLAY_LAYERS.findIndex((x) => x.layer === layer);
  if (at <= 0) return false;
  return OVERLAY_LAYERS.slice(0, at).some((x) => x.open !== null && has(x.open));
}

function defaultHas(selector: string): boolean {
  if (typeof document === 'undefined') return false;
  return document.querySelector(selector) !== null;
}
