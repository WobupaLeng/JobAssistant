<script setup lang="ts">
/**
 * 页面分发槽（C13）。
 *
 * **只在这里 import 页面**：`AppShell` 不知道有哪些页面，页面也不知道路由存在，
 * 双方都只跟这里的插槽打交道。
 *
 * 四页全部接线（M1–M4 汇合）：
 * - `chat` 整页替换（ChatPage 自带左 Boss 列表 + 右对话框）；
 * - `config` / `logs` 是**固定窗浮层**，叠在概览页之上渲染——关闭时 emit
 *   `navigate('dashboard')` 回到概览，浮层下面的概览保持挂载（轮询不中断）。
 */
import { computed, ref } from 'vue';
import AppShell from './components/AppShell.vue';
import DashboardPage from './pages/DashboardPage.vue';
import ChatPage from './pages/ChatPage.vue';
import ConfigPage from './pages/ConfigPage.vue';
import LogsPage from './pages/LogsPage.vue';
import type { Nav } from './lib_ui/nav';

const nav = ref<Nav>('dashboard');
/** 聊天抽屉展开态：ChatPage 通过 `chat-open` 事件置位，驱动外壳 `.is-chat-open`。 */
const chatOpen = ref(false);

function onNavigate(next: Nav): void {
  nav.value = next;
  // 离开聊天页即视为收起：下次进入从默认隐藏态开始（SPEC §0 R2）。
  if (next !== 'chat') chatOpen.value = false;
}

/**
 * 概览页与其上的 config/logs 浮层共用原型那套顶栏（计分模式 + 敏感库 + 配置 + 主题），
 * 页签留给内页——两套顶栏不叠加。只有整页替换的聊天页用外壳导航页签。
 */
const showNav = computed(() => nav.value === 'chat');
</script>

<template>
  <div class="app-shell" data-testid="app-shell-slot">
    <AppShell :nav="nav" :is-chat-open="chatOpen" :show-nav="showNav" @navigate="onNavigate">
      <ChatPage
        v-if="nav === 'chat'"
        :active-nav="nav"
        @navigate="onNavigate"
        @chat-open="chatOpen = $event"
      />
      <template v-else>
        <DashboardPage :active-nav="nav" @navigate="onNavigate" />
        <ConfigPage v-if="nav === 'config'" :active-nav="nav" @navigate="onNavigate" />
        <LogsPage v-if="nav === 'logs'" :active-nav="nav" @navigate="onNavigate" />
      </template>
    </AppShell>
  </div>
</template>
