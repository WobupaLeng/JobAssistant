<script setup lang="ts">
/**
 * 页面分发槽（C13）。
 *
 * **决策溯源：[`ADR-004`](../../docs/adr/ADR-004-legacy-chat-orchestration.md)**（用户裁决「方案 1」：
 * 实现向冻结 SPEC 对齐，聊天整页化否决）；实施图纸
 * [`SPEC-001-legacy-chat-merge`](../../docs/specs/SPEC-001-legacy-chat-merge.md)。
 *
 * 编排口径（`docs/ui/UI-SPEC-chat-detach-v3.md` R1 / R2 / R3 + §16.9 裁决 X）：
 *
 * - **概览常挂**：`DashboardPage` 始终挂载，5 秒轮询、勾选态、滚动位置都不因聊天展开而中断；
 * - **Boss 右栏同屏常驻**：聊天层（`aside.chat-rail` + `.chat-detach`）与概览是 `.app-body`
 *   的同级 flex 子项，对话框 `position: static`，靠压缩主列展开，**不是浮层**；
 * - **只有 config / logs 是浮层窗**：叠在概览之上，关闭时 emit `navigate('dashboard')`；
 * - 聊天不再是「页」，所以这里不再按 nav 分支卸载任何东西；`chatOpen` 由聊天层上抛，
 *   挂到唯一壳节点的 `.is-chat-open` 上（展开态列数 −1 的 CSS 钩子）。
 *
 * **只在这里 import 页面**：`AppShell` 不知道有哪些页面，页面也不知道路由存在，双方都只跟这里的插槽打交道。
 */
import { ref } from 'vue';

import AppShell from './components/AppShell.vue';
import ChatLayer from './pages/ChatPage.vue';
import DashboardPage from './pages/DashboardPage.vue';
import ConfigPage from './pages/ConfigPage.vue';
import LogsPage from './pages/LogsPage.vue';
import type { Nav } from './lib_ui/nav';

const nav = ref<Nav>('dashboard');
/** 聊天对话框展开态：由聊天层上抛，驱动壳的 `.is-chat-open`（右栏让位 / 列数 −1）。 */
const chatOpen = ref(false);

const chat = ref<InstanceType<typeof ChatLayer> | null>(null);

function onNavigate(next: Nav): void {
  nav.value = next;
}

/**
 * 投递卡片「打招呼」⇒ **就地**展开对话框并预填草稿（SPEC §16.5 / A48）。
 *
 * 草稿本身仍走 `features/chat/greet-draft`（概览写入、聊天层消费），
 * 但不再跨页传递：聊天层与概览同屏，点完就展开，不换页也不自动发送。
 */
function onGreet(): void {
  chat.value?.consumeGreetDraft();
}
</script>

<template>
  <AppShell :is-chat-open="chatOpen">
    <DashboardPage @navigate="onNavigate" @greet="onGreet" />

    <!-- 同屏聊天层：右栏常驻 + 对话框就地展开。放在概览之后 = 原型 DOM 顺序
         （主列 → aside.chat-rail → .chat-detach）。 -->
    <ChatLayer ref="chat" @chat-open="chatOpen = $event" />

    <ConfigPage v-if="nav === 'config'" @navigate="onNavigate" />
    <LogsPage v-if="nav === 'logs'" @navigate="onNavigate" />
  </AppShell>
</template>
