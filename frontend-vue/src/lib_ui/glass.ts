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

/* ---- L3：WebGL 抽屉透镜光学（规格 §3.2/§3.3 / SPEC-003 裁定 3/4/5） ----
 * 折射源限定「自绘卡片层」：WebGL readPixels 读不到 DOM 合成 / 亚克力，故不读背景，
 * 改为自画卡片几何到 FBO，透镜片元就地算 squircle SDF + Snell 法线偏移 UV 采样该 FBO。
 * 下列纯函数是 GLSL 的同源 TS 真值（tests/glass.test.ts 数学单测，毫秒级纯逻辑）；
 * 实际渲染在 use-glass-l3.ts 的着色器里端口同一公式。颜色一律运行时读 token，TS 无字面量。 */

/** L3 透镜常量（REFERENCE §2 默认参数 / 规格 §3.4 预算）。 */
export const GLASS_L3 = {
  /** 折射率：浮法玻璃 ~1.5（Snell n1·sinθ1 = n2·sinθ2）。 */
  refractiveIndex: 1.5,
  /** 透镜斜边宽度 px：边缘法线折射只发生在这条带内，平底核心零折射。 */
  bevelWidth: 18,
  /** 实例化卡片上限：超出只画前 64 个最近矩形（规格 §3.4）。 */
  maxInstances: 64,
  /** canvas  backing store 的 dpr 上限（规格 §3.4，防 4K 副本进滤镜）。 */
  dprCap: 2,
  /** 静止降频：抽屉静止 500ms 停 rAF 保留最终 draw（规格 §3.4，禁 requestIdleCallback）。 */
  idleStopMs: 500,
} as const;

/**
 * Snell 折射偏转量（REFERENCE §2 / generate_glass_map.js 同源）。
 * 入射角 thetaI=atan(slope)，折射角 thetaT=asin(sin(thetaI)/n)，返回 sin(thetaI-thetaT)。
 * 全内反射（|sinThetaT|>=1）返回 1.0。n=1.5 时 sinThetaT<=0.667 永不全内反射。
 */
export function snellDeflection(slope: number, n: number = GLASS_L3.refractiveIndex): number {
  const thetaI = Math.atan(slope);
  const sinThetaT = Math.sin(thetaI) / n;
  if (Math.abs(sinThetaT) >= 1) return 1.0;
  const thetaT = Math.asin(sinThetaT);
  return Math.sin(thetaI - thetaT);
}

/** squircle（p=4 超椭圆）SDF 结果：dist 符号距离（内负外正）＋单位外法线。 */
export interface SquircleSDF {
  dist: number;
  nx: number;
  ny: number;
}

/**
 * squircle SDF + 梯度法线（REFERENCE §2 逐字端口）。
 * 平底核心（dx=dy=0）返回负距离与零法线（核心不折射）；圆角带用 p=4 超椭圆。
 */
export function squircleSDF(
  px: number,
  py: number,
  halfW: number,
  halfH: number,
  radius: number,
): SquircleSDF {
  const r = Math.max(radius, 1e-3);
  const dx = Math.max(0, Math.abs(px) - (halfW - r));
  const dy = Math.max(0, Math.abs(py) - (halfH - r));
  if (dx === 0 && dy === 0) {
    return { dist: -Math.min(halfW - Math.abs(px), halfH - Math.abs(py)), nx: 0, ny: 0 };
  }
  const qx = dx / r;
  const qy = dy / r;
  const p = 4.0;
  const pNorm = Math.pow(Math.pow(qx, p) + Math.pow(qy, p), 1 / p);
  const dist = (pNorm - 1) * r;
  const gradFactor = pNorm > 1e-6 ? Math.pow(pNorm, 1 - p) : 0;
  const gx = Math.pow(Math.max(0, qx), p - 1) * gradFactor * Math.sign(px);
  const gy = Math.pow(Math.max(0, qy), p - 1) * gradFactor * Math.sign(py);
  const gradLen = Math.hypot(gx, gy) || 1;
  return { dist, nx: gx / gradLen, ny: gy / gradLen };
}

/**
 * 斜边坡度剖面（generate_glass_map.js L127-133 同源）：t=edgeDist/bevelWidth∈[0,1]，
 * 外缘（t=0）最陡、内缘（t=1）归零。slope=(invT^3 / max(1e-5,1-invT^4)^0.75)·2.5。
 */
export function bevelSlope(t: number): number {
  const invT = 1 - t;
  const denom = Math.pow(Math.max(1e-5, 1 - Math.pow(invT, 4)), 0.75);
  return (Math.pow(invT, 3) / denom) * 2.5;
}

/** dpr 夹值（规格 §3.4 上限 2.0）：非有限 / 非正回退 1，防 backing store 爆炸。 */
export function clampDpr(dpr: number): number {
  if (!Number.isFinite(dpr) || dpr <= 0) return 1;
  return Math.min(dpr, GLASS_L3.dprCap);
}