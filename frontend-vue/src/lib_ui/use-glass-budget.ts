/**
 * P1 卡片折射预算（规格 L2-3 / SPEC-003 §4：IntersectionObserver 同屏可见 ≤12 张挂滤镜）。
 *
 * 观察 .job-grid 里的 .job-card，按 DOM 序取前 GLASS_MAX_FILTERED_CARDS 张可见卡保留滤镜，
 * 其余打 .glass-budget-off（vue-glass.css 据此 filter:none 退回纯第一层）。卡片增删（v-for 重渲染）
 * 由 childList MutationObserver 重新观察；classList 切换是属性变更不触发 childList，无自激。
 * DOM 编排归 composable；预算常量在 glass.ts（纯）。
 */
import { onBeforeUnmount, onMounted, type Ref } from 'vue';
import { GLASS_MAX_FILTERED_CARDS } from './glass';

/** 给网格挂同屏折射预算。grid 是 .job-grid 元素 ref；max 默认 12（规格硬上限）。 */
export function useGlassBudget(grid: Ref<HTMLElement | null>, max: number = GLASS_MAX_FILTERED_CARDS): void {
  let io: IntersectionObserver | null = null;
  let mo: MutationObserver | null = null;
  const visible = new Set<HTMLElement>();

  function apply(): void {
    const cards = Array.from(grid.value?.querySelectorAll<HTMLElement>('.job-card') ?? []);
    const active = new Set(cards.filter((c) => visible.has(c)).slice(0, max));
    for (const c of cards) c.classList.toggle('glass-budget-off', !active.has(c));
  }

  function observe(): void {
    io?.disconnect();
    visible.clear();
    io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const el = e.target as HTMLElement;
          if (e.isIntersecting) visible.add(el);
          else visible.delete(el);
        }
        apply();
      },
      { root: null, threshold: 0 },
    );
    grid.value?.querySelectorAll<HTMLElement>('.job-card').forEach((c) => io?.observe(c));
  }

  onMounted(() => {
    // 能力守卫：无 IntersectionObserver 的环境（jsdom 单测 / 旧内核）跳过预算，卡片保留滤镜，
    // 不抛错。真实 WebView2 必有 IO，预算照常生效（与 use-glass-support 探针同一降级思路）。
    if (typeof IntersectionObserver === 'undefined') return;
    observe();
    mo = new MutationObserver(() => observe());
    if (grid.value) mo.observe(grid.value, { childList: true });
  });
  onBeforeUnmount(() => {
    io?.disconnect();
    io = null;
    mo?.disconnect();
    mo = null;
    visible.clear();
  });
}