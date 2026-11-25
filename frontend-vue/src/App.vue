<script setup lang="ts">
/**
 * 页面分发槽。**只在这里 import 页面**：`AppShell` 不知道有哪些页面，页面也不知道路由存在。
 * 概览常挂（轮询 / 勾选态 / 滚动位置不因聊天展开而中断）；聊天层与概览是 `.app-body` 的同级
 * flex 子项，靠压缩主列展开，**不是浮层**；只有 config / logs 是浮层窗，关闭时回到概览。
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
 * 投递卡片「打招呼」⇒ **就地**展开对话框并预填草稿。草稿走 `features/chat/greet-draft`
 * （概览写入、聊天层消费），点完就展开，不换页也不自动发送。
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
