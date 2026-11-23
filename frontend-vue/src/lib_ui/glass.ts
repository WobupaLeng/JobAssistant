/**
 * Liquid Glass 光学参数常量（规格 UI-SPEC-liquid-glass-l2-l3 §2.2-Q3 / SPEC-003 裁定 4）。
 *
 * SVG 滤镜属性不吃 CSS 变量，故 index.html 的 <filter> 数值静态写死；本模块是同源的
 * TS 真值，tests/glass.test.ts 断言三方一致（ThemeParams ↔ tokens.css 折射令牌 ↔ index.html defs），
 * 改令牌不改常量必须红。纯常量、无 DOM 依赖（lib_ui 分层，不反向引用 api/pages/features/components）。
 *
 * detect() 运行时探针在 use-glass-support.ts（打 data-l2="off"）；反平移纯函数 computeGlassOffset
 * 与同屏预算常量在本模块（L2-2 / L2-3）；DOM 编排（克隆 /  sanitize / IO）在 use-glass-*.ts composable。
 */

/** 单个主题的湍流折射参数（feTurbulence + feDisplacementMap，规格 Q3-A/B）。 */
export interface GlassThemeParams {
  /** feTurbulence 空间频率（1/波长），与 tokens.css --refract-turbulence 同源。 */
  baseFrequency: number;
  /** 噪声叠加层数：深 3 / 浅 2（浅底对比高，少一层细节防脏，规格 Q4）。 */
  numOctaves: number;
  /** feDisplacementMap 最大位移像素：--refract-edge 的 10/14 折（深 14→10，浅 5→5，规格 Q3-B）。 */
  scale: number;
  /** 随机种子：深浅必须相同，切主题噪波图案不跳变（规格 Q4 / 6.2-③）。 */
  seed: number;
}

/** 深色（默认）：tokens.css --refract-turbulence 0.008 / --refract-edge 14px。 */
export const GLASS_DARK: GlassThemeParams = {
  baseFrequency: 0.008,
  numOctaves: 3,
  scale: 10,
  seed: 11,
};

/** 浅色：tokens.css --refract-turbulence 0.006 / --refract-edge 5px（位移 >5 发晕，规格 6.1-⑪）。 */
export const GLASS_LIGHT: GlassThemeParams = {
  baseFrequency: 0.006,
  numOctaves: 2,
  scale: 5,
  seed: 11,
};

/**
 * 同屏折射预算（规格 L2-3 / SPEC-003 §4）：IntersectionObserver 可见卡片只给前 12 张挂滤镜，
 * 超出退回纯第一层。纯常量，IO 编排在 use-glass-budget.ts。
 */
export const GLASS_MAX_FILTERED_CARDS = 12;

/**
 * P0 背景副本反平移偏移（规格 2.2-Q2 / SPEC-003 裁定 6，纯函数）。
 *
 * 克隆副本是「未滚动」的内容快照（cloneNode 不复制 scrollTop），铺在面板局部原点；
 * 要把真实背景里落在面板左上角的那一点对齐到副本原点，需平移 -(host - source + scroll)。
 * 返回值即 --glass-x / --glass-y，CSS 用 translate3d(calc(var(--glass-x) * -1), …) 消费。
 * 滚动时只更新本偏移（反平移对齐），不逐帧重克隆 DOM（裁定 6 轻量策略）。
 *
 * @param hostX/hostY   面板 getBoundingClientRect().x/y（视口坐标）
 * @param sourceX/sourceY 背景源 getBoundingClientRect().x/y（视口坐标）
 * @param scrollLeft/scrollTop 背景源当前滚动偏移
 */
export function computeGlassOffset(
  hostX: number,
  hostY: number,
  sourceX: number,
  sourceY: number,
  scrollLeft: number,
  scrollTop: number,
): { x: number; y: number } {
  return { x: hostX - sourceX + scrollLeft, y: hostY - sourceY + scrollTop };
}