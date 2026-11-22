<script setup lang="ts">
/**
 * 决策记录的一行（冻结原型 .dl-row 四列网格：时间 / 行为 / 分数变化 / 原因）。
 * 纯展示组件：只消费页面算好的视图模型，不 import features（AGENTS §2.5）。
 */
defineProps<{
  row: {
    id: number;
    time: string;
    kind: string;
    verdict: string;
    verdictTitle: string;
    reason: string;
    reasonEmpty: boolean;
    rowClass: string;
  };
}>();
</script>

<template>
  <div :class="row.rowClass" data-testid="decision-row">
    <time>{{ row.time }}</time>
    <span class="card-sub">{{ row.kind }}</span>
    <span class="dl-verdict" :title="row.verdictTitle">{{ row.verdict }}</span>
    <span class="dl-reason" :class="{ 'is-empty': row.reasonEmpty }" :title="row.reason">{{
      row.reason
    }}</span>
  </div>
</template>
