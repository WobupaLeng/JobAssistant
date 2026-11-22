<script setup lang="ts">
/**
 * 求职期望（`list_expectations`）—— 只读展示。
 *
 * 位置：Boss 列表上方一行 chip 云。数据是后端从 Boss 账号里读回来的
 * 「期望职位」，用来确认抓取口径跟账号设置是否一致，所以只显示不编辑。
 */
import type { Expectation } from '../../api/types';

const props = withDefaults(
  defineProps<{
    items: readonly Expectation[];
    loading?: boolean;
    error?: string;
  }>(),
  { loading: false, error: '' },
);
</script>

<template>
  <div class="expect-row" data-testid="chat-expect-row">
    <span class="section-label">求职期望</span>
    <span v-if="props.loading" class="cfg-hint" data-testid="chat-expect-loading">读取中…</span>
    <span v-else-if="props.error !== ''" class="cfg-hint" data-testid="chat-expect-error">{{ props.error }}</span>
    <span v-else-if="props.items.length === 0" class="cfg-hint" data-testid="chat-expect-empty">没有读到期望职位</span>
    <template v-else>
      <span
        v-for="e in props.items"
        :key="`${e.code}-${e.name}`"
        class="chip"
        :data-testid="`chat-expect-${e.code}`"
        :title="[e.city, e.salary, e.source].filter((x) => x).join(' · ')"
      >{{ e.name }}</span>
    </template>
  </div>
</template>
