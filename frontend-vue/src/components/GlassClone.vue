<!--
  Liquid Glass 第二层克隆层（纯展示，SPEC-003 裁定 3 / 规格 2.4）。

  模式（裁定 6）：
    - backdrop（P0 抽屉 / 敏感库）：克隆面板背后窗内内容的非交互副本，反平移对齐（use-glass-clone）。
      active 控制面板开合时建 / 拆副本；source 是背景源选择器（默认 .dashboard，即滚动容器）。

  P1 岗位卡不再使用本组件：卡片只有 132px 高，10px 湍流位移会把卡底的高光描边和文字推离原位
  （卡片是信息密度最高的表面积，折射在这里只换来可读性损失），故卡片退回 L1 纯玻璃（裁定 7 作废）。

  整层 aria-hidden + pointer-events:none（vue-glass.css），副本另加 inert（裁定 6）；非交互控件，
  不挂 data-testid（C8 只约束可交互控件），data-glass-clone 仅作样式 / QA 标记。组件只到 lib_ui，不 import features。
-->
<script setup lang="ts">
import { ref } from 'vue';
import { useBackdropGlass } from '../lib_ui/use-glass-clone';

const props = withDefaults(
  defineProps<{
    /** 面板是否展开（开则建副本＋挂监听，关则拆除）。 */
    active?: boolean;
    /** 背景源选择器（默认概览滚动容器 .dashboard）。 */
    source?: string;
  }>(),
  { active: true, source: '.dashboard' },
);

const wrapEl = ref<HTMLElement | null>(null);
const backdropEl = ref<HTMLElement | null>(null);

useBackdropGlass({
  wrap: wrapEl,
  backdrop: backdropEl,
  active: () => props.active,
  source: props.source,
});
</script>

<template>
  <div
    ref="wrapEl"
    class="glass-clone-wrap"
    aria-hidden="true"
  >
    <div class="glass-clone is-backdrop" data-glass-clone>
      <div ref="backdropEl" class="glass-clone-backdrop"></div>
    </div>
  </div>
</template>
