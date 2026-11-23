<!--
  Liquid Glass 第二层克隆层（纯展示，SPEC-003 裁定 3 / 规格 2.4）。

  两种模式（裁定 6 / 7）：
    - material（P1 卡片）：克隆源＝宿主自身玻璃 tint，经 background:inherit 链传入，card-local 不反平移；
      纯 CSS，无 JS 编排。同屏滤镜数量由 use-glass-budget（IO ≤12）在网格层收口。
    - backdrop（P0 抽屉 / 敏感库）：克隆面板背后窗内内容的非交互副本，反平移对齐（use-glass-clone）。
      active 控制面板开合时建 / 拆副本；source 是背景源选择器（默认 .dashboard，即滚动容器）。

  整层 aria-hidden + pointer-events:none（vue-glass.css），副本另加 inert（裁定 6）；非交互控件，
  不挂 data-testid（C8 只约束可交互控件），data-glass-clone 仅作样式 / QA 标记。组件只到 lib_ui，不 import features。
-->
<script setup lang="ts">
import { ref } from 'vue';
import { useBackdropGlass } from '../lib_ui/use-glass-clone';

const props = withDefaults(
  defineProps<{
    /** material＝P1 卡自身材质；backdrop＝P0 面板背景副本。 */
    mode?: 'material' | 'backdrop';
    /** backdrop 模式：面板是否展开（开则建副本＋挂监听，关则拆除）。 */
    active?: boolean;
    /** backdrop 模式：背景源选择器（默认概览滚动容器 .dashboard）。 */
    source?: string;
  }>(),
  { mode: 'material', active: true, source: '.dashboard' },
);

const wrapEl = ref<HTMLElement | null>(null);
const backdropEl = ref<HTMLElement | null>(null);

if (props.mode === 'backdrop') {
  useBackdropGlass({
    wrap: wrapEl,
    backdrop: backdropEl,
    active: () => props.active,
    source: props.source,
  });
}
</script>

<template>
  <div
    ref="wrapEl"
    class="glass-clone-wrap"
    :class="{ 'is-material': mode === 'material' }"
    aria-hidden="true"
  >
    <div class="glass-clone" :class="{ 'is-backdrop': mode === 'backdrop' }" data-glass-clone>
      <div v-if="mode === 'backdrop'" ref="backdropEl" class="glass-clone-backdrop"></div>
    </div>
  </div>
</template>