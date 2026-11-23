<script setup lang="ts">
/**
 * 顶栏尾段：计分模式分段 / 敏感库入口 / token 读数 / 配置 / 主题。
 *
 * 为什么单独一个组件：`AppShell` 必须保持「不 import 页面、不含业务」，
 * 而这一段要读 `get_scoring_spec` 与 token 统计。放在 `components/` 并只
 * 通过 props/emit 通信，数据由持有它的 `DashboardPage` 喂进来（C13 插槽原则）。
 *
 * 落位（SPEC-001 决策 7 / AC10）：
 * 顶栏是一行，本组件由概览页 teleport 进 `AppShell` 的 `.tb-embed`，
 * 顺序按冻结 SPEC §11.3：`scrape-mode-segmented` → `sensitive-open` → token → 配置。
 *
 * 两处口径别改回去：
 * - **满分 / 门槛 chip 不在这里**（legacy-baseline §160「满分口径 `.score-spec` 已迁出顶栏」）——
 *   它挂在 `.card.log-standalone` 卡头（`LogCard`），锚点 `score-spec` 全站恰好一处；
 * - **配置按钮是 `shell-config-btn`**（原型 L1407 / SPEC §11.3 / 断言 A31），
 *   二级导航时代那个 `nav-config` 已随页签退役。
 */
import { tokenText, type ScoreModeSeg } from '../lib_ui/scrape-panel-model';

withDefaults(
  defineProps<{
    segs: readonly ScoreModeSeg[];
    tokensTotal?: number;
    sensitiveOpen?: boolean;
    themeLabel?: string;
  }>(),
  { tokensTotal: 0, sensitiveOpen: false, themeLabel: '深色' },
);

const emit = defineEmits<{
  (e: 'pick-mode', key: 'rule' | 'llm'): void;
  (e: 'toggle-sensitive'): void;
  (e: 'open-config'): void;
  (e: 'toggle-theme'): void;
}>();
</script>

<template>
  <div class="tb-tail">
    <div class="segmented" role="radiogroup" aria-label="打分方式" data-testid="scrape-mode-segmented">
      <button
        v-for="s in segs"
        :key="s.key"
        class="seg"
        :class="{ 'is-active': s.active }"
        type="button"
        role="radio"
        :aria-checked="s.active"
        :disabled="s.disabled"
        :aria-disabled="s.disabled"
        :title="s.disabled ? 'LLM 打分需先在配置里填好 API Key，本轮不可选' : s.label"
        :data-testid="s.testId"
        @click="!s.disabled && emit('pick-mode', s.key)"
      >
        {{ s.label }}
      </button>
    </div>
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      :aria-pressed="sensitiveOpen"
      data-testid="sensitive-open"
      @click="emit('toggle-sensitive')"
    >
      敏感库
    </button>
    <span class="tb-tokens" data-testid="token-total" :title="`累计 token：${tokensTotal}`">
      {{ tokenText(tokensTotal) }} tokens
    </span>
    <button class="btn btn-sm btn-ghost" type="button" data-testid="shell-config-btn" @click="emit('open-config')">
      配置
    </button>
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      data-testid="theme-toggle"
      :title="`切换明暗主题（当前${themeLabel}）`"
      @click="emit('toggle-theme')"
    >
      {{ themeLabel === '深色' ? '☾ 深色' : '☀ 浅色' }}
    </button>
  </div>
</template>
