/**
 * Liquid Glass 运行时能力探针（规格 2.2-Q6 降级 / SPEC-003 裁定 3）。
 *
 * L2-0 已在目标 WebView2（Chrome/157）实证 filter:url() 湍流位移生效；本探针是产品内的
 * 安全网：在不支持的环境把 <html data-l2="off"> 打上，vue-glass.css 据此整层关滤镜退回第一层。
 * 用 Canvas2D 的 ctx.filter='url(#lg-turb-dark)' 引用文档级 SVG 滤镜，画同一渐变两次逐像素比对，
 * 有位移即支持。颜色一律运行时读 tokens（--accent / --glass-2），TS 无颜色字面量（红线 8 / 裁定 4）。
 * DOM/canvas 编排归 composable（先例 use-canvas-scale），不入纯常量模块 glass.ts。
 */
import { onMounted } from 'vue';

/** 探针画布边长（够 feTurbulence 0.008 波长产生可测位移即可，不必大）。 */
const PROBE_SIZE = 64;
/** 判定阈值：位移像素数 > 16 才算滤镜真生效（排除抗锯齿噪声）。 */
const PROBE_MIN_CHANGED = 16;

/** 探测 filter:url() 是否真产生位移；任何异常都保守返回 false（整层降级，不抛错）。 */
export function detectGlassSupport(): boolean {
  try {
    const canvas = document.createElement('canvas');
    canvas.width = PROBE_SIZE;
    canvas.height = PROBE_SIZE;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) return false;
    const root = getComputedStyle(document.documentElement);
    const a = root.getPropertyValue('--accent').trim() || root.getPropertyValue('--glass-2').trim();
    const b = root.getPropertyValue('--glass-2').trim();
    const paint = (): void => {
      const g = ctx.createLinearGradient(0, 0, PROBE_SIZE, PROBE_SIZE);
      g.addColorStop(0, a);
      g.addColorStop(1, b);
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, PROBE_SIZE, PROBE_SIZE);
    };
    ctx.filter = 'none';
    paint();
    const base = ctx.getImageData(0, 0, PROBE_SIZE, PROBE_SIZE).data;
    ctx.filter = 'url(#lg-turb-dark)';
    paint();
    const filtered = ctx.getImageData(0, 0, PROBE_SIZE, PROBE_SIZE).data;
    let changed = 0;
    for (let i = 0; i < base.length; i += 4) {
      if (Math.abs(base[i] - filtered[i]) > 2) changed += 1;
    }
    return changed > PROBE_MIN_CHANGED;
  } catch {
    return false;
  }
}

/** 挂载时探一次：不支持则整层关闭（data-l2="off"），支持则不打标记（默认开）。 */
export function useGlassSupport(): void {
  onMounted(() => {
    if (!detectGlassSupport()) document.documentElement.dataset.l2 = 'off';
  });
}