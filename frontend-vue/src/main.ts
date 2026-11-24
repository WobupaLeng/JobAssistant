/**
 * Vue 入口。
 *
 * 顺序有讲究：
 * 1. `bootstrapTheme()` 先跑：它把 `data-theme` 打到 `<html>` 上。首帧那一刻
 *    `index.html` 的内联脚本已经打过一次，这里补的是「系统偏好在首帧后才变」
 *    与「storage 被禁用」两种情况；两处判据必须一致，否则会闪一帧。
 * 2. 样式表 `styles/index.css` 只在这里 import 一次——它内部按 C4 的顺序
 *    `@import` 那 11 份分域 CSS，顺序就是层叠契约，别在别处再引。
 * 3. 最后才 `mount`：先有样式、先有主题，再有 DOM。
 */
import { createApp } from 'vue';

import { bootstrapTheme } from './lib_ui/theme';
import { DEFAULT_LIGHT_ANGLE, isInside } from './lib_ui/light-edge';
import { applyFontScale, applyPalette, readFontScale, readPalette } from './lib_ui/palette';
import '../styles/index.css';
import './app.css';
import App from './App.vue';

const theme = bootstrapTheme();
// M1：启动即应用已存调色板与文字大小（与 DashboardPage 的运行时应用一致）。
applyPalette(readPalette(), theme);
applyFontScale(readFontScale());

// M4：卡片高光随指针（光源）变化。事件委托，卡片组件无需逐个改造。
//
// 两层，各配一块渐变，永不互换（用户 m01973）：
//   默认对角弧 —— `.card::after` / `.hl-surface::after`，左上、右下两道弧（135deg），常驻；
//   指针光斑   —— `.is-lit::before`，以指针为圆心的径向光斑（半径 300px，中心纯白）。
//
// 规则（用户 m01609 裁决）：**指针就是光源，光斑以指针为圆心**——中心 100% 纯白、
// 不按指针到边框的距离打折；亮度自中心向两侧对称衰减，300px 处归零。
// 因此这里写进 --hl-x / --hl-y 的是指针在卡内的真实相对位置（百分比）。
// 位置变化走 `background-position` 的 transition，光标在卡内移动时光斑是「流过去」的。
//
// 转换（用户 m01748 / m01776 / m01804 / m01838 / m01933 裁决）：两段同时、逐帧、
// 同一段时长（--hl-restore-ms = 220ms）：
//   进卡：默认对角弧从当前亮度淡到 0，指针光斑从第一帧起就是全亮；
//   离卡：指针光斑留在最后停下的位置逐渐变暗（--hl-fade 1 → 0），
//         默认对角弧逐渐变亮（→ 1）直到 --spec-strength。
// 两者动**不同**的属性：默认弧动它自己的 opacity，光斑动卡片上的 --hl-fade。
// 都去补间 ::before 的 opacity 的话，两条动画写同一个属性，后挂的会顶掉先挂的。
//
// 状态一律自己记账，**不读** `getComputedStyle(card, '::after').opacity`：
// Chromium 对伪元素的 computed style 不返回 Web Animations 补间中的值，读到的永远是
// CSS 里的静态满值，于是每次进卡都从满亮重来一遍（用户 m01999「一直存在」、
// m02013「移出还闪一次」就是这么来的）。记账 + `cancel` 前先落盘，才接得上。
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
    // progress 不在 Animation 上（它在 KeyframeEffect 上），lib.dom 里没有，这里取同一条时间线：
    // 补间是 linear + 无 delay，所以「已播放时长 / 总时长」就是进度。
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

/** 默认对角弧（卡片自己的 ::after）从 from 补到 to。
 *  fill 'both'：没有它，动画一结束 opacity 就退回 CSS 的静态值——默认弧与指针光斑
 *  是同一条边框环上的两层，退了就等于「过渡结束的瞬间跳一下」。首帧也要靠它兜住。 */
function fadeDefaultArc(card: HTMLElement, st: LitState, from: number, to: number): void {
  const frames = { duration: HL_RESTORE_MS, easing: 'linear', delay: 0, fill: 'both' } as const;
  const anim = card.animate([{ opacity: from }, { opacity: to }], { ...frames, pseudoElement: '::after' });
  st.arcAnim = anim;
  trackArc(st, anim, from, to);
}

/** 进卡（用户 m01838）：默认对角弧从当前亮度淡到 0，指针光斑从第一帧起就是全亮。 */
function enterLight(card: HTMLElement, st: LitState): void {
  fadeDefaultArc(card, st, st.arc, 0);
}

/** 离卡（用户 m01804）：光斑逐渐变暗、默认光带逐渐变亮，两条同时走完。 */
function restoreDefaultLight(card: HTMLElement, st: LitState): void {
  const frames = { duration: HL_RESTORE_MS, easing: 'linear', delay: 0, fill: 'both' } as const;
  fadeDefaultArc(card, st, st.arc, 1);
  // 光斑的亮度只由 --hl-fade 表达（CSS 里逐个 stop 乘上它），不进状态：
  // 它只会从 1 淡到 0，中途再进卡就由 stopSpot 取消、回到 1。
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
  const to = ev.relatedTarget as HTMLElement | null;
  if (to && from.contains(to)) return;
  const st = stateOf(from);
  if (!st.lit) return;
  // 顺序要紧：先把当前两段补间的亮度记进状态再取消（否则默认弧跳到满值、光斑跳到 1），
  // 然后摘 .is-lit 让 ::before 的光斑停在最后位置，最后挂两段同时起跑的淡出/淡入。
  stopSpot(from, st);
  st.lit = false;
  from.classList.remove('is-lit');
  from.style.setProperty('--light-angle', `${DEFAULT_LIGHT_ANGLE}deg`);
  restoreDefaultLight(from, st);
});

createApp(App).mount('#app');