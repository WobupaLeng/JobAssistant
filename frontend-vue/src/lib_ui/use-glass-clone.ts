/**
 * P0 背景折射克隆编排（SPEC-003 裁定 6 / 规格 2.2-Q2 / liquid-glass 技能「克隆背景而非活 DOM」）。
 *
 * 面板（抽屉 / 敏感库）背后窗内内容的非交互视觉副本：开面板克隆一次，反平移对齐真实屏幕坐标，
 * 滚动只更新 --glass-x/y（不逐帧重克隆），内容变更 / resize 节流重克隆。硬约束（裁定 6，不可省）：
 *   - 副本剥离全部 id / data-testid（C8 / qa-testids 锚点唯一性，重复锚点＝QA 红）；
 *   - 剥离既有 .glass-clone-wrap（防递归嵌套滤镜）＋排除面板自身与上层叠层；
 *   - aria-hidden + inert + pointer-events:none（不可交互、读屏不念）；
 *   - 按面板 rect 预裁剪＋20px 出血（vue-glass.css .glass-clone.is-backdrop inset:-20px），禁整窗副本进滤镜。
 * DOM 编排归 composable（先例 use-canvas-scale）；偏移纯函数在 glass.ts。
 */
import { onBeforeUnmount, onMounted, watch, type Ref } from 'vue';
import { computeGlassOffset } from './glass';

/**
 * 面板自身与其上层叠层：克隆背景时必须排除（裁定 6）。敏感库 teleport 到 body，本就不在源内。
 * 状态类另单列一份：`inert` 会整棵子树不响应指针事件，但状态类本身也要从副本剥离，
 * 否则副本上会挂着 `.is-open`（常驻 DOM 的抽屉收起后副本仍是打开态）。
 */
const OVERLAY_SELECTORS = '.drawer,.drawer-backdrop,.sensitive-panel,.sensitive-backdrop';
const STATE_CLASSES = ['is-open', 'is-closing', 'pan-pop', 'pan-modal', 'drawer-anim', 'shell-resized'];
/**
 * 内容变更后的重克隆节流（ms）。
 *
 * 副本只是抽屉背后的**静态模糊底图**：消息文本、未读数、列表行的增删改在模糊层上
 * 看不出区别，而一次重克隆要把整个源容器 `cloneNode(true)` 再逐节点净化
 * （`sanitizeBackdropCopy`），是整屏级别的 DOM 开销。原来监听
 * `{childList, subtree, characterData}` + 200ms 节流，聊天页每来一条消息、每次刷新
 * 列表都会触发一次整屏重克隆——实测足以把主线程压住（用户报「查看消息会很卡」）。
 *
 * 节流窗口从 200ms 拉到 1.5s：确实改变了可见结构（例如整块面板换页）时仍会在
 * 一个呼吸内跟上，但连续的内容更新只会被合并成一次。
 */
const RECLONE_THROTTLE_MS = 1500;

/**
 * 净化背景副本：剥锚点 / 防递归 / 排除叠层 / 不可交互，并把滚动容器中性化（overflow:visible、
 * height:auto）让「未滚动」的完整内容铺开——反平移才能揭示被滚走的部分。
 */
function sanitizeBackdropCopy(root: HTMLElement, width: number): void {
  root.querySelectorAll(`${OVERLAY_SELECTORS},.glass-clone-wrap`).forEach((n) => n.remove());
  root.removeAttribute('id');
  root.removeAttribute('data-testid');
  root.querySelectorAll('[id]').forEach((n) => n.removeAttribute('id'));
  root.querySelectorAll('[data-testid]').forEach((n) => n.removeAttribute('data-testid'));
  root.setAttribute('aria-hidden', 'true');
  root.setAttribute('inert', '');
  // inert 已经能挡住交互（副本走 .glass-clone-wrap 的 pointer-events:none 之外还有这层），
  // 这里再写一次是为了「副本上永远不会有可命中的盒子」这条断言在 DOM 层面直接成立。
  root.style.pointerEvents = 'none';
  root.style.overflow = 'visible';
  root.style.height = 'auto';
  root.style.maxHeight = 'none';
  root.style.width = `${width}px`;
  // 副本是静态快照：把状态类与交互候选一并摘掉，滚轮也穿不过去（不接就落到下面的真内容）。
  root.querySelectorAll('.is-open, .is-closing, .pan-pop, .pan-modal, .drawer-anim, .shell-resized').forEach((n) => {
    for (const c of STATE_CLASSES) n.classList.remove(c);
  });
  root.querySelectorAll('button, a, input, select, textarea, label, [tabindex]').forEach((n) => {
    n.setAttribute('inert', '');
    n.setAttribute('tabindex', '-1');
  });
}

/**
 * 把背景副本接进面板克隆层。opts.wrap 是 .glass-clone-wrap（其 parentElement 即面板宿主），
 * opts.backdrop 是 .glass-clone-backdrop（副本容器，CSS 消费 --glass-x/y 反平移）。
 * active() 为真（面板开）时建副本＋挂监听，为假时拆除。生命周期钩子绑定调用方组件。
 */
export function useBackdropGlass(opts: {
  wrap: Ref<HTMLElement | null>;
  backdrop: Ref<HTMLElement | null>;
  active: () => boolean;
  source: string;
}): void {
  let sourceEl: HTMLElement | null = null;
  let offsetRaf = 0;
  let recloneTimer = 0;
  let contentMo: MutationObserver | null = null;
  let resizeRo: ResizeObserver | null = null;

  function writeOffset(): void {
    const wrap = opts.wrap.value;
    const host = wrap?.parentElement;
    if (!wrap || !host || !sourceEl) return;
    const hr = host.getBoundingClientRect();
    const sr = sourceEl.getBoundingClientRect();
    const { x, y } = computeGlassOffset(hr.x, hr.y, sr.x, sr.y, sourceEl.scrollLeft, sourceEl.scrollTop);
    wrap.style.setProperty('--glass-x', `${x}px`);
    wrap.style.setProperty('--glass-y', `${y}px`);
  }

  function scheduleOffset(): void {
    if (offsetRaf) return;
    offsetRaf = requestAnimationFrame(() => {
      offsetRaf = 0;
      writeOffset();
    });
  }

  function build(): void {
    const backdrop = opts.backdrop.value;
    const wrap = opts.wrap.value;
    if (!backdrop || !wrap?.parentElement) return;
    // 先断观察再改 DOM：副本就在源 subtree 内（面板在 .dashboard 里），否则会自激重克隆死循环。
    contentMo?.disconnect();
    sourceEl = document.querySelector<HTMLElement>(opts.source);
    backdrop.replaceChildren();
    if (!sourceEl) return;
    const copy = sourceEl.cloneNode(true) as HTMLElement;
    sanitizeBackdropCopy(copy, sourceEl.getBoundingClientRect().width);
    backdrop.appendChild(copy);
    writeOffset();
    contentMo = new MutationObserver(() => {
      if (recloneTimer) return;
      recloneTimer = window.setTimeout(() => {
        recloneTimer = 0;
        build();
      }, RECLONE_THROTTLE_MS);
    });
    // 只盯「行级增删」（列表换页、会话进出），不盯 `characterData`：
    // 聊天消息落进 DOM 时改的都是文本节点，模糊层上看不见，却会触发整屏深拷贝。
    contentMo.observe(sourceEl, { childList: true, subtree: true });
  }

  function onScroll(): void {
    scheduleOffset();
  }

  function start(): void {
    build();
    sourceEl?.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', scheduleOffset);
    // 能力守卫：无 ResizeObserver 的环境靠 window resize + scroll 仍可对齐全，不抛错。
    if (typeof ResizeObserver !== 'undefined') {
      resizeRo = new ResizeObserver(scheduleOffset);
      const host = opts.wrap.value?.parentElement;
      if (host) resizeRo.observe(host);
    }
  }

  function stop(): void {
    sourceEl?.removeEventListener('scroll', onScroll);
    window.removeEventListener('resize', scheduleOffset);
    resizeRo?.disconnect();
    resizeRo = null;
    contentMo?.disconnect();
    contentMo = null;
    if (recloneTimer) {
      window.clearTimeout(recloneTimer);
      recloneTimer = 0;
    }
    if (offsetRaf) {
      cancelAnimationFrame(offsetRaf);
      offsetRaf = 0;
    }
    opts.backdrop.value?.replaceChildren();
    sourceEl = null;
  }

  onMounted(() => {
    if (opts.active()) start();
  });
  watch(opts.active, (on) => {
    if (on) start();
    else stop();
  });
  onBeforeUnmount(() => stop());
}
