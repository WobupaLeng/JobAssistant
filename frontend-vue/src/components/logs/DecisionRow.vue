<script setup lang="ts">
/**
 * 决策记录的一行（冻结原型 .dl-row 四列网格：时间 / 环节 / 结论 / 理由）。纯展示组件：
 * 只消费页面算好的视图模型，不 import features（AGENTS §2.5）。比原型多带平台与到秒的时间；
 * 「理由」列里公司/岗位与原因分两层、最多两行可见，全文进 tooltip。
 */
defineProps<{
  row: {
    id: number;
    time: string;
    timeTitle: string;
    kind: string;
    platform: string;
    verdict: string;
    verdictTitle: string;
    reason: string;
    why: string;
    reasonEmpty: boolean;
    rowClass: string;
    company: string;
    title: string;
  };
}>();
</script>

<template>
  <div :class="row.rowClass" data-testid="decision-row" :title="row.reason">
    <time :datetime="row.timeTitle" :title="row.timeTitle">{{ row.time }}</time>
    <span class="dl-kind">
      <span class="card-sub">{{ row.kind }}</span>
      <span class="dl-plat" :title="`平台：${row.platform}`">{{ row.platform }}</span>
    </span>
    <span class="dl-verdict" :title="row.verdictTitle">{{ row.verdict }}</span>
    <span class="dl-reason" :class="{ 'is-empty': row.reasonEmpty }">
      <span class="dl-who">{{ row.company }} · {{ row.title }}</span>
      <span class="dl-why">{{ row.why }}</span>
    </span>
  </div>
</template>