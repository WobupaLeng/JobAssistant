<script setup lang="ts">
/**
 * 应用外壳：**只到顶栏为止**。
 *
 * 页面（概览 / 配置浮层 / 决策记录浮层）与同屏的聊天层全部归 ui lane，
 * 外壳不 import 任何 `pages/`——`frontend-vue/AGENTS.md` §2 把「`AppShell`
 * 不得反向依赖 `pages`」列为硬约束，页面树由 `App.vue` 作为插槽传进来（C13）。
 *
 * 外壳**不调任何业务命令**（Leptos 版在外壳里 `app_info` / `get_scoring_spec` /
 * `llm_status` 各打一发，那是外壳替页面做了业务层的活）。外壳只负责：
 * 玻璃顶栏、概览业务尾段的落位（`.tb-embed`，见下）、页面骨架槽，
 * 以及首次挂载时的一次 `scrape_close`（关窗收尾：只 stop 不关 stdin 会让 Python 主循环残留）。
 *
 * **二级导航已退役**（冻结 SPEC-001 AC9 + legacy-baseline §9 Q-1 + 原型 L1413
 * 「用户裁决 5」；决策记录见 `App.vue` 的 ADR-004 溯源）：顶栏不再有页签行，也不再有第二个
 * 主题开关——`theme-toggle` 全站恰好一处，住在概览的 `.tb-tail` 里（TopbarTail）。
 *
 * **`.tb-embed` 是顶栏单行的落位点**：平台卡与尾段的**数据在概览页**（满分门槛 / token /
 * 登录态 / 敏感库），组件也就继续由 `DashboardPage` 持有；外壳只提供一个
 * `display: contents` 的空盒，页面用 `<Teleport>` 把那段 DOM 搬进本行（SPEC-001 决策 7）。
 * 独立挂载 `DashboardPage`（单测）时该空盒不存在，Teleport 自动退回就地渲染。
 */
import { onMounted } from 'vue';

import { scrapeClose } from '../api/commands/scrape';
import { windowClose, windowMinimize, windowToggleMaximize } from '../api/commands/window';
import { useCanvasScale } from '../lib_ui/use-canvas-scale';
import { useGlassSupport } from '../lib_ui/use-glass-support';
import GlassCanvas from './GlassCanvas.vue';

const props = withDefaults(
  defineProps<{
    /**
     * 聊天对话框是否展开。展开时投递列表列数 = 收起时 − 1（SPEC §16.9 裁决 X），
     * 靠 `.app-shell.is-chat-open` 这一层 class 承接，规则写在 `styles/vue-dashboard.css`。
     * 挂在这一个壳节点上：它是全应用唯一的 `.app-shell`（SPEC-001 决策 13）。
     */
    isChatOpen?: boolean;
  }>(),
  { isChatOpen: false },
);

onMounted(() => {
  void scrapeClose().catch(() => undefined);
});

/** 窗口控件：失败静默（窗口操作不影响业务，错误不进错误队列）。 */
const onMinimize = (): void => void windowMinimize().catch(() => undefined);
const onMaximize = (): void => void windowToggleMaximize().catch(() => undefined);
const onClose = (): void => void windowClose().catch(() => undefined);

/**
 * 全景几何接线：resize → 先写 `--ui-scale`，再写 `--rail-w` / `--detach-w`
 * （原型 `syncUiScale()` 先行于 `syncDetachWidth()`；顺序由 `syncCanvas()` 内部保证）。
 * 挂在外壳上而不是 `main.ts`：外壳是与窗口同寿命的根组件，卸载时有钩子可以摘监听，
 * 也不会在 mount 之前就去读 `innerWidth`。
 */
const canvas = useCanvasScale();
defineExpose({ syncCanvas: canvas.sync });

/** Liquid Glass 第二层能力探针：不支持 filter:url() 的环境整层降级（data-l2="off"，规格 Q6）。 */
useGlassSupport();
</script>

<template>
  <div class="app-shell" :class="{ 'is-chat-open': props.isChatOpen }" data-testid="app-root">
    <header class="topbar" data-testid="topbar" data-tauri-drag-region>
      <div class="brand">
        <span class="brand-mark" aria-hidden="true">职</span>
        <span class="brand-name">AI 求职助手</span>
      </div>

      <!-- 概览的业务尾段（平台卡 · 打分分段 · 敏感库 · token · 配置 · 主题） teleport 到这里。
           `display: contents`（`styles/app-shell.css` 口径见 vue-dashboard.css 的 `.tb-embed`）
           让这个空盒不产生新的 flex 容器：teleport 进来的元素是 `.topbar` 的直接 flex 子项。 -->
      <div class="tb-embed" data-testid="topbar-tail-mount" />

      <!-- 窗口控件（无边框窗口）：最右侧 _ □ × -->
      <div class="win-ctrl" data-testid="win-controls">
        <button class="win-btn" type="button" aria-label="最小化" data-testid="win-minimize-btn" @click="onMinimize">_</button>
        <button class="win-btn" type="button" aria-label="最大化" data-testid="win-maximize-btn" @click="onMaximize">□</button>
        <button class="win-btn win-close" type="button" aria-label="关闭" data-testid="win-close-btn" @click="onClose">×</button>
      </div>
    </header>

    <!-- 页面骨架：页面分发槽。锚点 page-<slug> 由各页面自己挂，外壳不重复挂（C8）。 -->
    <div class="app-body">
      <slot />
    </div>

    <!-- Liquid Glass 第三层透镜画布（SPEC-003 裁定 5）：挂 .app-shell 内部末尾，
         position:fixed z-40 盖过同层遮罩、低于抽屉面板 41；reduced-motion 时 v-if 不创建（A8）。 -->
    <GlassCanvas />
  </div>
</template>
