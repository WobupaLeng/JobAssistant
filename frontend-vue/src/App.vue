<script setup lang="ts">
/**
 * 页面分发的**唯一槽位**（AGENTS.md C13）。
 *
 * 这里只做一件事：持有 `nav`，把页面塞进 `AppShell` 的默认插槽。
 * 页面组件目前还不存在（ui lane 的里程碑），所以插槽里是一段可见占位；
 * 接真实页面时**只改本文件的模板**，外壳一行不动：
 *
 * ```vue
 * <AppShell :nav="nav" @navigate="onNavigate">
 *   <DashboardPage v-if="nav === 'dashboard'" />
 *   <ChatPage v-else-if="nav === 'chat'" />
 *   <ConfigPage v-else-if="nav === 'config'" />
 *   <LogsPage v-else />
 * </AppShell>
 * ```
 *
 * 不要在这里 import 命令或业务状态：分发层只该知道「哪个 nav 对应哪个页面」。
 */
import { ref } from 'vue';

import AppShell from './components/AppShell.vue';
import { type Nav } from './lib_ui/nav';
import DashboardPage from './pages/DashboardPage.vue';

const nav = ref<Nav>('dashboard');

function onNavigate(next: Nav): void {
  nav.value = next;
}
</script>

<template>
  <div class="app-shell" data-testid="app-shell-slot">
    <AppShell :nav="nav" @navigate="onNavigate">
      <DashboardPage v-if="nav === 'dashboard'" />
      <template v-else>
        <!-- 占位页：ui lane 接管后逐页替换 -->
        <section class="card page-placeholder" data-testid="page-placeholder" :data-page="nav">
          界面重构中：本页由 ui lane 的 pages/* 接管
        </section>
      </template>
    </AppShell>
  </div>
</template>
