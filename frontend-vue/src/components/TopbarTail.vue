<script setup lang="ts">
/**
 * 顶栏尾段：计分规格 chip / 计分模式分段 / 敏感库入口 / token 读数 / 配置 / 主题。
 *
 * 为什么单独一个组件：`AppShell` 必须保持"不 import 页面、不含业务"，
 * 而这一段要读 `get_scoring_spec` 与 token 统计。放在 `components/` 并只
 * 通过 props/emit 通信，`App.vue` 负责把数据喂进来（C13 插槽原则）。
 */
import { computed } from 'vue';
import { tokenText, type ScoreModeSeg } from '../lib_ui/scrape-panel-model';

const props = withDefaults(
  defineProps<{
    /** 满分。`null` = 还没取到，此时 chip 显示占位而不是猜一个数。 */
    scoreMax?: number | null;
    minScore?: number | null;
    segs: readonly ScoreModeSeg[];
    tokensTotal?: number;
    sensitiveOpen?: boolean;
    themeLabel?: string;
  }>(),
  { scoreMax: null, minScore: null, tokensTotal: 0, sensitiveOpen: false, themeLabel: '深色' },
);

const emit = defineEmits<{
  (e: 'pick-mode', key: 'rule' | 'llm'): void;
  (e: 'toggle-sensitive'): void;
  (e: 'open-config'): void;
  (e: 'toggle-theme'): void;
}>();

const specText = computed(() =>
  props.scoreMax === null ? '计分规格加载中' : `满分 ${props.scoreMax} · 门槛 ${props.minScore ?? '—'}`,
);
</script>

<template>
  <div class="tb-tail">
    <span class="chip" data-testid="score-spec">{{ specText }}</span>
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
      data-testid="sensitive-open-btn"
      @click="emit('toggle-sensitive')"
    >
      敏感库
    </button>
    <span class="spacer"></span>
    <span class="tb-tokens" data-testid="token-total" :title="`累计 token：${tokensTotal}`">
      {{ tokenText(tokensTotal) }} tokens
    </span>
    <button class="btn btn-sm btn-ghost" type="button" data-testid="nav-config" @click="emit('open-config')">
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
