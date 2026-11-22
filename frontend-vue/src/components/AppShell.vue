<script setup lang="ts">
/**
 * 应用外壳：**只到顶栏为止**。
 *
 * 页面（概览 / Boss 消息 / 配置 / 决策记录）全部归 ui lane 的 `pages/`。
 * 外壳不 import 任何 `pages/`——`frontend-vue/AGENTS.md` §2 把「`AppShell`
 * 不得反向依赖 `pages`」列为硬约束，页面树由 `App.vue` 作为插槽传进来（C13）。
 *
 * 外壳**不调任何业务命令**（Leptos 版在外壳里 `app_info` / `get_scoring_spec` /
 * `llm_status` 各打一发，那是外壳替页面做了业务层的活）。外壳只负责：
 * 玻璃顶栏、导航页签、主题开关、页面骨架槽，以及首次挂载时的一次
 * `scrape_close`（关窗收尾：只 stop 不关 stdin 会让 Python 主循环残留）。
 */
import { onMounted, ref } from 'vue';

import { scrapeClose } from '../api/commands/scrape';
import { NAV, NAV_BLOCK_CLASS, navTestId, pageTestId, type Nav } from '../lib_ui/nav';
import { applyTheme, flipTheme, isDark, readInitialTheme, type Theme } from '../lib_ui/theme';
// 常量经 import 绑定后在模板里可直接用（不需要挂到 setup 的返回上）。
const NAV_LIST = NAV;
const NAV_CLASS = NAV_BLOCK_CLASS;

const props = withDefaults(
  defineProps<{
    /** 当前页面。由 `App.vue`（页面分发槽）持有与切换，外壳只读。 */
    nav?: Nav;
  }>(),
  { nav: 'dashboard' },
);

const emit = defineEmits<{
  /** 用户点了某页签。外壳不改 `nav`，由持有方决定切不切（C13：分发是槽位）。 */
  (e: 'navigate', nav: Nav): void;
}>();

const theme = ref<Theme>(readInitialTheme());

function toggleTheme(): void {
  theme.value = flipTheme(theme.value);
  applyTheme(theme.value);
}

function go(key: Nav): void {
  emit('navigate', key);
}

onMounted(() => {
  void scrapeClose().catch(() => undefined);
});
</script>

<template>
  <div class="app-shell" data-testid="app-root">
    <header class="topbar" data-testid="topbar">
      <div class="brand">
        <span class="brand-mark" aria-hidden="true">职</span>
        AI 求职助手
      </div>

      <div :class="NAV_CLASS">
        <nav class="tabs nav-tabs" role="tablist" aria-label="主导航">
          <button
            v-for="item in NAV_LIST"
            :key="item.key"
            type="button"
            role="tab"
            class="tab"
            :class="{ 'is-active': item.key === props.nav }"
            :aria-selected="item.key === props.nav ? 'true' : 'false'"
            :data-testid="navTestId(item.key)"
            @click="go(item.key)"
          >
            {{ item.name }}
          </button>
        </nav>
        <!-- 顶栏附加插槽：敏感库入口要紧挨「决策记录」，由页面分发层塞进来。 -->
        <slot name="nav-extra" />
      </div>

      <span class="spacer" />

      <!-- 右侧状态插槽：满分/门槛（C10，值必须来自 get_scoring_spec）、大模型状态、
           以及 C9 要求的那段顶栏进度文案。外壳不去取，避免把业务状态拖进 components 层。 -->
      <slot name="topbar-status" />

      <button
        type="button"
        class="btn btn-sm btn-ghost"
        data-testid="theme-toggle"
        title="切换明暗主题"
        @click="toggleTheme"
      >
        {{ isDark(theme) ? '☾ 深色' : '☀ 浅色' }}
      </button>
    </header>

    <!-- 页面骨架：页面分发槽。data-testid 必须是 page-<slug>，否则 E2E 的页面锚点全断（C8）。 -->
    <div :class="['app-body', `page-${props.nav}`]" :data-testid="pageTestId(props.nav)">
      <slot />
    </div>
  </div>
</template>
