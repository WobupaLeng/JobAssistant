/**
 * Esc 分层表：**一次 `keydown` 只关最上面那一层**，从上往下是决策记录 → 配置 → 岗位详情
 * 抽屉 / 敏感库 → 聊天对话框。四个消费方共用，各抄一份就会改一处漏三处。
 * 纯函数：`has` 只问「选择器在当前 DOM 里有没有命中」，单测给假的 `has` 即可。
 */

/** 表里的层标识，顺序即优先级（前 = 上）。 */
export type OverlayLayer = 'decisions' | 'config' | 'drawer' | 'sensitive' | 'chat';

/**
 * 各层「正开着」的判据：`decisions` / `config` 是 v-if 挂载，DOM 里有 = 开着；
 * `drawer` / `sensitive` 常驻 DOM，靠 `.is-open` 表达开合；`chat` 不在「上层」里，无选择器。
 */
export const OVERLAY_LAYERS: ReadonlyArray<{ readonly layer: OverlayLayer; readonly open: string | null }> = [
  { layer: 'decisions', open: '[data-testid="decisions-panel"]' },
  { layer: 'config', open: '[data-testid="config-panel"]' },
  { layer: 'drawer', open: '[data-testid="job-detail-drawer"].is-open' },
  { layer: 'sensitive', open: '[data-testid="sensitive-panel"].is-open' },
  { layer: 'chat', open: null },
];

/**
 * `layer` 之上是否还有正开着的层，为真时这一层不许回应 Esc。
 * @param has 选择器是否命中的判定，默认查全局 document
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
