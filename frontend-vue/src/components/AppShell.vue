<script setup lang="ts">
/**
 * 应用外壳：**只到顶栏为止**。外壳**不 import 任何 `pages/`**，也不调业务命令，页面树由
 * `App.vue` 作插槽传进来。负责顶栏、`.tb-embed` 落位点、页面骨架槽，以及首次挂载时的一次
 * 全景几何接线。
 *
 * 挂载时**不得** `scrape_close`：WebView 刷新（F5）也会重新走一遍挂载，那条命令会把
 * sidecar 与整棵 Chrome 树关掉——用户看到的正是「刷新一下浏览器就没了」。sidecar 由
 * Rust 进程持有，页面重载只需重查登录态（各页自己的 `login_status`），不需要关进程。
 */
import { windowClose, windowMinimize, windowToggleMaximize } from '../api/commands/window';
import { useCanvasScale } from '../lib_ui/use-canvas-scale';

const props = withDefaults(
  defineProps<{
    /**
     * 聊天对话框是否展开。展开时投递列表列数 = 收起时 − 1，由 `.app-shell.is-chat-open`
     * 承接，规则在 `styles/vue-dashboard.css`。
     */
    isChatOpen?: boolean;
  }>(),
  { isChatOpen: false },
);

/** 窗口控件：失败静默（窗口操作不影响业务，错误不进错误队列）。 */
const onMinimize = (): void => void windowMinimize().catch(() => undefined);
const onMaximize = (): void => void windowToggleMaximize().catch(() => undefined);
const onClose = (): void => void windowClose().catch(() => undefined);

/**
 * 全景几何接线：resize → 先写 `--ui-scale`，再写 `--rail-w` / `--detach-w`（顺序由
 * `syncCanvas()` 内部保证）。挂在外壳而不是 `main.ts`：与窗口同寿命，卸载时有钩子摘监听。
 */
const canvas = useCanvasScale();
defineExpose({ syncCanvas: canvas.sync });
</script>

<template>
  <div class="app-shell" :class="{ 'is-chat-open': props.isChatOpen }" data-testid="app-root">
    <!-- 顶栏本身**不**挂拖动区：整条 46px 都能拖会让内容区跟着整个窗口跑。
         拖动 / 双击最大化只认窗口最上方 20px 那条横带，由 `lib_ui/drag-allow` 按坐标补属性。 -->
    <header class="topbar" data-testid="topbar">
      <div class="brand">
        <span class="brand-mark" aria-hidden="true">职</span>
        <span class="brand-name">AI 求职助手</span>
      </div>

      <!-- 概览的业务尾段（平台卡 · 打分分段 · 敏感库 · token · 配置 · 主题） teleport 到这里。
           `display: contents`（`styles/app-shell.css` 口径见 vue-dashboard.css 的 `.tb-embed`）
           让这个空盒不产生新的 flex 容器：teleport 进来的元素是 `.topbar` 的直接 flex 子项。 -->
      <div class="tb-embed" data-testid="topbar-tail-mount" />

      <!-- 窗口控件（无边框窗口）：最右侧 最小化 / 最大化 / 关闭。
           图标是内联 SVG（stroke = currentColor），不用 `_` `□` `×` 字符：
           字符会随字体缩放且粗细不可控，放大后发虚。 -->
      <div class="win-ctrl" data-testid="win-controls">
        <button class="win-btn" type="button" aria-label="最小化" title="最小化" data-testid="win-minimize-btn" @click="onMinimize">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
            <path d="M3.5 8.5h9" />
          </svg>
        </button>
        <button class="win-btn" type="button" aria-label="最大化" title="最大化" data-testid="win-maximize-btn" @click="onMaximize">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true">
            <rect x="3.5" y="3.5" width="9" height="9" rx="1.6" />
          </svg>
        </button>
        <button class="win-btn win-close" type="button" aria-label="关闭" title="关闭" data-testid="win-close-btn" @click="onClose">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
            <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
          </svg>
        </button>
      </div>
    </header>

    <!-- 页面骨架：页面分发槽。锚点 page-<slug> 由各页面自己挂，外壳不重复挂（C8）。 -->
    <div class="app-body">
      <slot />
    </div>
  </div>
</template>
