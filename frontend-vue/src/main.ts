/**
 * Vue 入口，顺序固定：先 `bootstrapTheme()` 打 `data-theme`（判据须与 index.html
 * 的内联脚本一致，否则闪一帧）→ 再 import `styles/index.css`（内部按 C4 顺序
 * `@import`，顺序即层叠契约，别处不许再引）→ 最后 `mount`。
 */
import { createApp } from 'vue';

import { bootstrapTheme } from './lib_ui/theme';
import { DEFAULT_LIGHT_ANGLE, isInside } from './lib_ui/light-edge';
import { applyFontScale, applyPalette, readFontScale, readPalette } from './lib_ui/palette';
import { installDragAllow } from './lib_ui/drag-allow';
import '../styles/index.css';
import './app.css';
import App from './App.vue';

const theme = bootstrapTheme();
// M1：启动即应用已存调色板与文字大小（与 DashboardPage 的运行时应用一致）。
applyPalette(readPalette(), theme);
applyFontScale(readFontScale());

// 卡片高光随指针移动（事件委托，组件无需改造）。两层渐变永不互换：常驻对角弧在
// `.card::after`，指针光斑在 `.is-lit::before`（写 --hl-x / --hl-y）。进卡 / 离卡两段补间
// 同 220ms 但动不同属性（弧动 opacity、光斑动 --hl-fade），写同一个会互相顶掉。状态自己
// 记账，不读伪元素的 computed opacity——Chromium 不返回补间中的值。
const HL_SURFACES = '.card, .hl-surface, .sensitive-card';
/** 恢复时长，与 tokens.css 的 --hl-restore-ms 一致（改一处须同改另一处）。 */
const HL_RESTORE_MS = 220;

interface LitState {
  /** 指针是否停在卡内。 */
  lit: boolean;
  /** 默认弧此刻的透明度。 */
  arc: number;
  /** 默认弧正在跑的补间（撤销前先把 arc 落盘）。 */
  arcAnim: Animation | null;
  /** 指针光斑正在跑的淡出（离卡时挂上，再进卡就收回来）。 */
  spotAnim: Animation | null;
}
const litStates = new WeakMap<HTMLElement, LitState>();

/** 默认弧 CSS 里的静态透明度（--spec-strength）：没有任何补间时，::after 就是这个亮度。 */
function arcRest(card: HTMLElement): number {
  const v = Number(getComputedStyle(card).getPropertyValue('--spec-strength'));
  return Number.isFinite(v) && v > 0 ? v : 1;
}

function stateOf(card: HTMLElement): LitState {
  const hit = litStates.get(card);
  if (hit !== undefined) return hit;
  const fresh: LitState = { lit: false, arc: arcRest(card), arcAnim: null, spotAnim: null };
  litStates.set(card, fresh);
  return fresh;
}

/** 逐帧把默认弧的透明度记进状态，跨帧的连续淡出/淡入全靠这一笔账。 */
function trackArc(st: LitState, anim: Animation, from: number, to: number): void {
  const tick = (): void => {
    if (anim.playState === 'finished') {
      st.arc = to;
      return;
    }
      // 补间是 linear 无 delay，用「已播放时长 / 总时长」当进度。
    const timing = anim.effect?.getComputedTiming();
    const played = typeof anim.currentTime === 'number' ? Number(anim.currentTime) : 0;
    const total = typeof timing?.duration === 'number' ? timing.duration : HL_RESTORE_MS;
    const p = total > 0 ? Math.min(Math.max(played / total, 0), 1) : 1;
    st.arc = from + (to - from) * p;
  };
  anim.addEventListener('finish', tick);
  anim.addEventListener('cancel', () => {
    st.arc = from;
  });
  anim.addEventListener('frame', tick);
  tick();
}

/** 默认对角弧从 from 补到 to。`fill: 'both'` 必需：否则动画一结束 opacity
 *  就退回 CSS 静态值，过渡结束的瞬间会跳一下。 */
function fadeDefaultArc(card: HTMLElement, st: LitState, from: number, to: number): void {
  const frames = { duration: HL_RESTORE_MS, easing: 'linear', delay: 0, fill: 'both' } as const;
  const anim = card.animate([{ opacity: from }, { opacity: to }], { ...frames, pseudoElement: '::after' });
  st.arcAnim = anim;
  trackArc(st, anim, from, to);
}

/** 进卡：默认对角弧从当前亮度淡到 0，指针光斑从第一帧起就是全亮。 */
function enterLight(card: HTMLElement, st: LitState): void {
  fadeDefaultArc(card, st, st.arc, 0);
}

/** 离卡：光斑逐渐变暗、默认对角弧逐渐变亮，两条同时走完。 */
function restoreDefaultLight(card: HTMLElement, st: LitState): void {
  const frames = { duration: HL_RESTORE_MS, easing: 'linear', delay: 0, fill: 'both' } as const;
  fadeDefaultArc(card, st, st.arc, 1);
  // 光斑亮度只由 --hl-fade 表达，不进状态：它只会 1 → 0，再进卡时由 stopSpot 取消。
  st.spotAnim = card.animate([{ '--hl-fade': '1' }, { '--hl-fade': '0' }], frames);
}

/** 撤销两段补间。默认弧的当前值先记进 st.arc，再 cancel——cancel 会立刻把它打回静态满值。 */
function stopSpot(card: HTMLElement, st: LitState): void {
  st.spotAnim?.cancel();
  st.spotAnim = null;
  st.arcAnim?.cancel();
  st.arcAnim = null;
}

document.addEventListener('pointermove', (ev) => {
  const card = (ev.target as HTMLElement | null)?.closest?.(HL_SURFACES) as HTMLElement | null;
  if (!card) return;
  const rect = card.getBoundingClientRect();
  const x = ev.clientX - rect.left;
  const y = ev.clientY - rect.top;
  if (!isInside(x, y, rect.width, rect.height)) return;
  const st = stateOf(card);
  // 先写好位置，两种分支都要：点亮前写是为了首帧不从默认 50%/0% 滑过来。
  card.style.setProperty('--hl-x', `${((x / Math.max(rect.width, 1)) * 100).toFixed(2)}%`);
  card.style.setProperty('--hl-y', `${((y / Math.max(rect.height, 1)) * 100).toFixed(2)}%`);
  // 首次进卡才动手：之后每次 pointermove 都还在 .is-lit 上，重启动画会每移一下就闪。
  if (!st.lit) {
    stopSpot(card, st);
    st.lit = true;
    card.classList.add('is-lit');
    enterLight(card, st);
  }
});

// 指针离开卡片：光斑留在原地，默认对角弧逐帧变亮回满值。
document.addEventListener('pointerout', (ev) => {
  const from = (ev.target as HTMLElement | null)?.closest?.(HL_SURFACES) as HTMLElement | null;
  if (!from) return;
  // 只比「最近的高光面」：指针进了嵌套的另一个面（岗位列表 → 岗位卡）时，外层这一圈也要还原，
  // 否则外层默认弧停在 0、光斑又被内层卡片盖住，看起来就是外框光带永久消失。
  const to = (ev.relatedTarget as HTMLElement | null)?.closest?.(HL_SURFACES) ?? null;
  if (to === from) return;
  const st = stateOf(from);
  if (!st.lit) return;
  // 顺序要紧：先记账再取消补间，然后摘 .is-lit，最后挂两段淡出/淡入。
  stopSpot(from, st);
  st.lit = false;
  from.classList.remove('is-lit');
  from.style.setProperty('--light-angle', `${DEFAULT_LIGHT_ANGLE}deg`);
  restoreDefaultLight(from, st);
});

/**
 * 窗口标题带：只有窗口最上方 20px 能拖窗口 / 双击最大化，页面其余空白处不拖。
 * `data-tauri-drag-region` 只认被点到的那一个元素、不冒泡，所以由 `lib_ui/drag-allow`
 * 按按下坐标补属性。细节见该文件，这里只挂上。
 */
installDragAllow(document);

/**
 * 窗口落定过渡：resize 期间不给尺寸补间（会让内容滞后），只在停下 140ms 后给
 * `.app-shell` 挂一次性的 200ms 淡入（`styles/motion.css` 的 shell-settle）。
 * 时长须与 shell-settle 一致。
 */
const RESIZE_IDLE_MS = 140;
let resizeTimer = 0;
window.addEventListener('resize', () => {
  window.clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(() => {
    const shell = document.querySelector<HTMLElement>('.app-shell');
    if (!shell) return;
    shell.classList.remove('shell-resized');
    // 同一 class 连续挂两次不会重播动画，先读一次布局强制回流再挂。
    void shell.offsetWidth;
    shell.classList.add('shell-resized');
    shell.addEventListener('animationend', () => shell.classList.remove('shell-resized'), { once: true });
  }, RESIZE_IDLE_MS);
});

createApp(App).mount('#app');