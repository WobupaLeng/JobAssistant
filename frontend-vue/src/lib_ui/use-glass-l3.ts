/**
 * Liquid Glass 第三层：WebGL2 抽屉透镜（规格 §3 / SPEC-003 裁定 3/4/5）。
 *
 * 折射源限定「自绘卡片层」——WebGL readPixels 读不到 DOM 合成 / 亚克力，故不读背景：
 * 每帧把可见 .job-card 几何（rect + 圆角 + 底色，运行时读 computed style）实例化画进 FBO，
 * 再画一个覆盖抽屉矩形的透镜，片元就地算 squircle SDF + Snell 法线偏移 UV 采样该 FBO，
 * 混 --glass-3 tint 与边缘高光。canvas 只在透镜矩形内绘，其余全透明（Vue 透明底 C5，
 * 不画规格 §3.3 step3 的 body 不透明 quad，否则会挡亚克力——映射偏差记 HANDOFF）。
 *
 * 分层：纯光学常量 / 数学在 glass.ts（vitest 单测）；本模块是 DOM + WebGL 编排 composable
 * （先例 use-glass-clone / use-canvas-scale），零 invoke、不 import features/pages/api。
 * 颜色一律运行时 getComputedStyle 读 token，TS 无颜色字面量（红线 8 / 裁定 4）。
 *
 * 预算（规格 §3.4）：抽屉关 / 被上层叠层盖住 → 完全停 rAF 并清屏；静止 500ms 停 rAF 保留最终 draw；
 * visibilityState hidden 停；dpr≤2；实例≤64；webgl2 创建失败 → data-l3="off"（第二层照常）。
 * reduced-motion / reduced-transparency 的「canvas 根本不创建」由 GlassCanvas.vue 的 v-if 承接。
 */
import { onBeforeUnmount, onMounted, type Ref } from 'vue';

import { GLASS_L3, clampDpr } from './glass';
import { hasHigherOverlay } from './overlay-stack';

/** 透镜矩形（CSS px，视口左上原点）。 */
interface LensRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Renderer {
  resize(): void;
  render(rect: LensRect, radius: number, progress: number): void;
  clear(): void;
  dispose(): void;
}

/** 解析 computed 颜色串（rgba/rgb）为归一化 vec4；transparent / 无法解析 → 全 0。无数值字面量颜色。 */
function parseColor(css: string): [number, number, number, number] {
  const m = /rgba?\(([^)]+)\)/.exec(css);
  if (!m) return [0, 0, 0, 0];
  const p = m[1].split(',').map((s) => Number.parseFloat(s.trim()));
  return [(p[0] ?? 0) / 255, (p[1] ?? 0) / 255, (p[2] ?? 0) / 255, p.length > 3 ? (p[3] ?? 1) : 1];
}

const CARD_VS = `#version 300 es
layout(location=0) in vec2 aCorner;
layout(location=1) in vec4 aRect;
layout(location=2) in float aRadius;
layout(location=3) in vec4 aColor;
uniform vec2 uResolution;
uniform float uDpr;
out vec2 vLocal;
out vec2 vHalf;
out float vRadius;
out vec4 vColor;
void main() {
  vec2 px = aRect.xy + aCorner * aRect.zw;
  vec2 clip = (px * uDpr / uResolution) * 2.0 - 1.0;
  gl_Position = vec4(clip, 0.0, 1.0);
  vLocal = (aCorner - 0.5) * aRect.zw;
  vHalf = aRect.zw * 0.5;
  vRadius = aRadius;
  vColor = aColor;
}`;

const CARD_FS = `#version 300 es
precision highp float;
in vec2 vLocal;
in vec2 vHalf;
in float vRadius;
in vec4 vColor;
out vec4 frag;
float boxSDF(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void main() {
  float d = boxSDF(vLocal, vHalf, max(vRadius, 0.001));
  float shape = 1.0 - smoothstep(-1.0, 1.0, d);
  if (shape <= 0.001) discard;
  float edge = smoothstep(-2.0, 0.0, d);
  vec3 col = vColor.rgb * (1.0 + edge * 0.15);
  float a = vColor.a * shape;
  frag = vec4(col * a, a);
}`;

const LENS_VS = `#version 300 es
layout(location=0) in vec2 aPos;
uniform vec2 uResolution;
uniform float uDpr;
out vec2 vScreen;
void main() {
  gl_Position = vec4(aPos, 0.0, 1.0);
  vScreen = vec2(aPos.x * 0.5 + 0.5, 0.5 - aPos.y * 0.5) * uResolution / uDpr;
}`;

const LENS_FS = `#version 300 es
precision highp float;
in vec2 vScreen;
uniform sampler2D uCards;
uniform vec2 uResolution;
uniform float uDpr;
uniform vec4 uLensRect;
uniform float uLensRadius;
uniform float uBevel;
uniform float uRefractEdge;
uniform float uIOR;
uniform vec4 uTint;
uniform float uProgress;
out vec4 frag;
vec2 squircleNormal(vec2 p, vec2 hs, float r, out float dist) {
  r = max(r, 0.001);
  vec2 d = max(vec2(0.0), abs(p) - (hs - r));
  if (d.x == 0.0 && d.y == 0.0) {
    dist = -min(hs.x - abs(p.x), hs.y - abs(p.y));
    return vec2(0.0);
  }
  vec2 q = d / r;
  float pn = pow(pow(q.x, 4.0) + pow(q.y, 4.0), 0.25);
  dist = (pn - 1.0) * r;
  float gf = pn > 1e-6 ? pow(pn, -3.0) : 0.0;
  vec2 g = vec2(pow(max(0.0, q.x), 3.0) * gf * sign(p.x), pow(max(0.0, q.y), 3.0) * gf * sign(p.y));
  float gl = length(g);
  return gl > 0.0 ? g / gl : vec2(0.0);
}
float snell(float slope, float n) {
  float ti = atan(slope);
  float st = sin(ti) / n;
  if (abs(st) >= 1.0) return 1.0;
  return sin(ti - asin(st));
}
float bevel(float t) {
  float inv = 1.0 - t;
  float den = pow(max(1e-5, 1.0 - pow(inv, 4.0)), 0.75);
  return (pow(inv, 3.0) / den) * 2.5;
}
void main() {
  vec2 center = uLensRect.xy + uLensRect.zw * 0.5;
  vec2 p = vScreen - center;
  vec2 hs = uLensRect.zw * 0.5;
  float dist;
  vec2 nrm = squircleNormal(p, hs, uLensRadius, dist);
  if (dist > 0.0) { frag = vec4(0.0); return; }
  float edgeDist = -dist;
  float edgeFactor = 0.0;
  vec2 n = vec2(0.0);
  if (edgeDist < uBevel) {
    float t = clamp(edgeDist / uBevel, 0.0, 1.0);
    n = nrm * snell(bevel(t), uIOR);
    edgeFactor = 1.0 - t;
  }
  vec2 uv = vScreen + n * edgeFactor * uRefractEdge;
  vec4 card = texture(uCards, uv * uDpr / uResolution);
  vec3 col = mix(card.rgb, uTint.rgb, uTint.a * 0.5);
  col += pow(edgeFactor, 3.0) * 0.25;
  float a = uProgress;
  frag = vec4(col * a, a);
}`;

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader | null {
  const sh = gl.createShader(type);
  if (!sh) return null;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    gl.deleteShader(sh);
    return null;
  }
  return sh;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram | null {
  const v = compile(gl, gl.VERTEX_SHADER, vs);
  const f = compile(gl, gl.FRAGMENT_SHADER, fs);
  if (!v || !f) {
    if (v) gl.deleteShader(v);
    if (f) gl.deleteShader(f);
    return null;
  }
  const prog = gl.createProgram();
  if (!prog) return null;
  gl.attachShader(prog, v);
  gl.attachShader(prog, f);
  gl.linkProgram(prog);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    gl.deleteProgram(prog);
    return null;
  }
  return prog;
}

/** 创建 WebGL2 渲染器；任何着色器 / 资源失败返回 null（调用方打 data-l3="off"）。 */
function createRenderer(gl: WebGL2RenderingContext, canvas: HTMLCanvasElement): Renderer | null {
  const cardProg = link(gl, CARD_VS, CARD_FS);
  const lensProg = link(gl, LENS_VS, LENS_FS);
  if (!cardProg || !lensProg) {
    if (cardProg) gl.deleteProgram(cardProg);
    if (lensProg) gl.deleteProgram(lensProg);
    return null;
  }

  const cardU = {
    res: gl.getUniformLocation(cardProg, 'uResolution'),
    dpr: gl.getUniformLocation(cardProg, 'uDpr'),
  };
  const lensU = {
    cards: gl.getUniformLocation(lensProg, 'uCards'),
    res: gl.getUniformLocation(lensProg, 'uResolution'),
    dpr: gl.getUniformLocation(lensProg, 'uDpr'),
    rect: gl.getUniformLocation(lensProg, 'uLensRect'),
    radius: gl.getUniformLocation(lensProg, 'uLensRadius'),
    bevel: gl.getUniformLocation(lensProg, 'uBevel'),
    edge: gl.getUniformLocation(lensProg, 'uRefractEdge'),
    ior: gl.getUniformLocation(lensProg, 'uIOR'),
    tint: gl.getUniformLocation(lensProg, 'uTint'),
    progress: gl.getUniformLocation(lensProg, 'uProgress'),
  };

  // 单位四边形（卡片 aCorner，0..1）与裁剪空间四边形（透镜 aPos，-1..1），均 TRIANGLE_STRIP。
  const cornerBuf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, cornerBuf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
  const clipBuf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, clipBuf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);

  const instanceBuf = gl.createBuffer();
  const cardVao = gl.createVertexArray();
  gl.bindVertexArray(cardVao);
  gl.bindBuffer(gl.ARRAY_BUFFER, cornerBuf);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuf);
  // 每实例 9 float：rect4(x,y,w,h) + radius1 + color4(r,g,b,a)。
  const stride = 9 * 4;
  gl.enableVertexAttribArray(1);
  gl.vertexAttribPointer(1, 4, gl.FLOAT, false, stride, 0);
  gl.vertexAttribDivisor(1, 1);
  gl.enableVertexAttribArray(2);
  gl.vertexAttribPointer(2, 1, gl.FLOAT, false, stride, 16);
  gl.vertexAttribDivisor(2, 1);
  gl.enableVertexAttribArray(3);
  gl.vertexAttribPointer(3, 4, gl.FLOAT, false, stride, 20);
  gl.vertexAttribDivisor(3, 1);
  gl.bindVertexArray(null);

  const lensVao = gl.createVertexArray();
  gl.bindVertexArray(lensVao);
  gl.bindBuffer(gl.ARRAY_BUFFER, clipBuf);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
  gl.bindVertexArray(null);

  const fbo = gl.createFramebuffer();
  const fboTex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, fboTex);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

  let width = 0;
  let height = 0;
  let dpr = 1;
  const instances = new Float32Array(GLASS_L3.maxInstances * 9);

  function resize(): void {
    dpr = clampDpr(window.devicePixelRatio || 1);
    width = Math.max(1, Math.round(window.innerWidth * dpr));
    height = Math.max(1, Math.round(window.innerHeight * dpr));
    canvas.width = width;
    canvas.height = height;
    gl.bindTexture(gl.TEXTURE_2D, fboTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, fboTex, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** 读可见卡片几何 + 底色（运行时 computed style，无字面量），填充实例缓冲（每实例 9 float）。 */
  function readCards(): number {
    const cards = document.querySelectorAll<HTMLElement>('.job-card');
    let n = 0;
    for (let i = 0; i < cards.length && n < GLASS_L3.maxInstances; i += 1) {
      const c = cards[i];
      const r = c.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) continue;
      const cs = getComputedStyle(c);
      const col = parseColor(cs.backgroundColor);
      const radius = Number.parseFloat(cs.borderRadius) || 0;
      const o = n * 9;
      instances[o] = r.x;
      instances[o + 1] = r.y;
      instances[o + 2] = r.width;
      instances[o + 3] = r.height;
      instances[o + 4] = radius;
      instances[o + 5] = col[0];
      instances[o + 6] = col[1];
      instances[o + 7] = col[2];
      instances[o + 8] = col[3];
      n += 1;
    }
    return n;
  }

  function clear(): void {
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
  }

  function render(rect: LensRect, radius: number, progress: number): void {
    if (width === 0 || height === 0) resize();
    const count = readCards();
    const root = getComputedStyle(document.documentElement);
    const tint = parseColor(root.getPropertyValue('--glass-3').trim());
    const edge = Number.parseFloat(root.getPropertyValue('--refract-edge').trim()) || 0;

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    // 1) 卡片层 → FBO（透明清屏，实例化圆角矩形）。
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(cardProg);
    gl.uniform2f(cardU.res, width, height);
    gl.uniform1f(cardU.dpr, dpr);
    gl.bindVertexArray(cardVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, instanceBuf);
    gl.bufferData(gl.ARRAY_BUFFER, instances.subarray(0, count * 9), gl.DYNAMIC_DRAW);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);

    // 2) 透镜 → 默认帧缓冲（只绘抽屉矩形内，其余透明）。
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, width, height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(lensProg);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, fboTex);
    gl.uniform1i(lensU.cards, 0);
    gl.uniform2f(lensU.res, width, height);
    gl.uniform1f(lensU.dpr, dpr);
    gl.uniform4f(lensU.rect, rect.x, rect.y, rect.w, rect.h);
    gl.uniform1f(lensU.radius, radius);
    gl.uniform1f(lensU.bevel, GLASS_L3.bevelWidth);
    gl.uniform1f(lensU.edge, edge);
    gl.uniform1f(lensU.ior, GLASS_L3.refractiveIndex);
    gl.uniform4f(lensU.tint, tint[0], tint[1], tint[2], tint[3]);
    gl.uniform1f(lensU.progress, progress);
    gl.bindVertexArray(lensVao);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.bindVertexArray(null);
  }

  function dispose(): void {
    gl.deleteProgram(cardProg);
    gl.deleteProgram(lensProg);
    gl.deleteBuffer(cornerBuf);
    gl.deleteBuffer(clipBuf);
    gl.deleteBuffer(instanceBuf);
    gl.deleteVertexArray(cardVao);
    gl.deleteVertexArray(lensVao);
    gl.deleteFramebuffer(fbo);
    gl.deleteTexture(fboTex);
  }

  return { resize, render, clear, dispose };
}

/**
 * 挂载第三层透镜。canvas 由 GlassCanvas.vue 在非 reduced-motion 时创建（A8）；
 * 这里只做 WebGL 初始化 + 渲染循环 + 预算降级。webgl2 不可用 → data-l3="off"，第二层照常。
 */
export function useGlassL3(canvas: Ref<HTMLCanvasElement | null>): void {
  let el: HTMLCanvasElement | null = null;
  let renderer: Renderer | null = null;
  let raf = 0;
  let running = false;
  let lastSig = '';
  let idleSince = 0;
  let moBody: MutationObserver | null = null;
  let moTheme: MutationObserver | null = null;
  const disposers: Array<() => void> = [];

  function off(): void {
    document.documentElement.dataset.l3 = 'off';
  }

  /** 抽屉正开着且未被上层叠层 / 敏感库盖住（裁定 5：敏感库开时透镜 inactive）。 */
  function activeDrawer(): HTMLElement | null {
    const d = document.querySelector<HTMLElement>('[data-testid="job-detail-drawer"]');
    if (!d) return null;
    const cs = getComputedStyle(d);
    if (cs.display === 'none' || cs.visibility === 'hidden') return null;
    const r = d.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) return null;
    if (hasHigherOverlay('drawer')) return null;
    if (document.querySelector('[data-testid="sensitive-panel"].is-open')) return null;
    return d;
  }

  function stop(): void {
    running = false;
    if (raf) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  function setOpacity(v: string): void {
    if (el && el.style.opacity !== v) el.style.opacity = v;
  }

  function frame(): void {
    raf = 0;
    if (!renderer || !el) {
      running = false;
      return;
    }
    if (document.visibilityState === 'hidden') {
      running = false;
      return;
    }
    const d = activeDrawer();
    if (!d) {
      renderer.clear();
      setOpacity('0');
      running = false;
      lastSig = '';
      return;
    }
    const r = d.getBoundingClientRect();
    const cs = getComputedStyle(d);
    // 进度＝抽屉真实 computed opacity（Vue 抽屉是 24px 微移 + 淡入，opacity 0→1 即动画真值；
    // 规格 §3.6 字面公式 1-r.right/innerWidth 预设整屏滑入，对本抽屉恒≈0，故读真实 opacity——映射偏差记 HANDOFF）。
    const progress = Math.min(Math.max(Number.parseFloat(cs.opacity) || 0, 0), 1);
    const radius = Number.parseFloat(cs.borderRadius) || 0;
    renderer.render({ x: r.x, y: r.y, w: r.width, h: r.height }, radius, progress);
    setOpacity(String(progress));
    const sig = `${r.x},${r.y},${r.width},${r.height},${progress}`;
    const now = performance.now();
    if (sig !== lastSig) {
      lastSig = sig;
      idleSince = now;
    }
    if (now - idleSince >= GLASS_L3.idleStopMs) {
      running = false; // 静止降频：停 rAF，保留最终 draw（规格 §3.4）
      return;
    }
    raf = requestAnimationFrame(frame);
  }

  function start(): void {
    if (running || !renderer) return;
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
    running = true;
    idleSince = performance.now();
    lastSig = '';
    raf = requestAnimationFrame(frame);
  }

  onMounted(() => {
    el = canvas.value;
    if (!el) return; // reduced-motion：canvas 未创建（A8）
    if (typeof WebGL2RenderingContext === 'undefined') {
      off();
      return;
    }
    let gl: WebGL2RenderingContext | null = null;
    try {
      gl = el.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: false, depth: false, stencil: false });
    } catch {
      gl = null;
    }
    if (!gl) {
      off();
      return;
    }
    renderer = createRenderer(gl, el);
    if (!renderer) {
      off();
      return;
    }
    renderer.resize();

    if (typeof MutationObserver !== 'undefined') {
      moBody = new MutationObserver(() => start());
      moBody.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'class'] });
      moTheme = new MutationObserver(() => start());
      moTheme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    }
    const onScroll = (): void => start();
    const onResize = (): void => {
      renderer?.resize();
      start();
    };
    const onVis = (): void => {
      if (document.visibilityState === 'hidden') stop();
      else start();
    };
    window.addEventListener('scroll', onScroll, { passive: true, capture: true });
    window.addEventListener('resize', onResize);
    document.addEventListener('visibilitychange', onVis);
    disposers.push(
      () => window.removeEventListener('scroll', onScroll, true),
      () => window.removeEventListener('resize', onResize),
      () => document.removeEventListener('visibilitychange', onVis),
    );
    start();
  });

  onBeforeUnmount(() => {
    stop();
    moBody?.disconnect();
    moTheme?.disconnect();
    for (const d of disposers) d();
    renderer?.dispose();
    renderer = null;
    el = null;
  });
}