<!--
  Liquid Glass 第三层透镜画布（规格 §3.5 / SPEC-003 裁定 3/5）。

  挂在 AppShell 的 .app-shell 内部末尾（.app-body 之后）：position:fixed inset:0 z-index:40
  pointer-events:none，DOM 序在后盖过同层遮罩，低于抽屉面板 41。自读 .job-card 几何自绘卡片层
  折射进抽屉矩形，零 invoke、不 import features/pages（components 分层）。

  A8：prefers-reduced-motion / prefers-reduced-transparency 为真时 canvas 根本不创建（v-if）。
  非交互纯展示，不挂可交互锚点；glass-l3-canvas 锚点供 QA 定位（qa-testids 守卫全站唯一）。
  WebGL 初始化 / 渲染循环 / 预算降级全在 use-glass-l3.ts；webgl2 不可用打 data-l3="off"。
-->
<script setup lang="ts">
import { ref } from 'vue';

import { useGlassL3 } from '../lib_ui/use-glass-l3';

/** 同步判定减少动效 / 减少透明度（A8：真则不创建 canvas）。matchMedia 缺失（jsdom）保守视为不禁用。 */
function motionReduced(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return (
    window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
    window.matchMedia('(prefers-reduced-transparency: reduce)').matches
  );
}

const enabled = ref(!motionReduced());
const canvasEl = ref<HTMLCanvasElement | null>(null);

useGlassL3(canvasEl);
</script>

<template>
  <canvas
    v-if="enabled"
    ref="canvasEl"
    class="glass-l3-canvas"
    data-testid="glass-l3-canvas"
    aria-hidden="true"
  />
</template>