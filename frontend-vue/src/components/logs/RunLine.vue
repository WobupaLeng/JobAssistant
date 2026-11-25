<script setup lang="ts">
/**
 * 运行日志的一行（.run-log-line：时间 / 序号 / 级别 / 消息）。
 * 纯展示组件：只复用 lib_ui/logfmt 的格式化，不 import features（AGENTS §2.5）。
 * 时间列 `white-space: nowrap`：日期与时分秒必须在一行，否则看起来像没有时间戳。
 */
import { fmtLogTime, levelLabel, type LogLevel } from '../../lib_ui/logfmt';

defineProps<{
  row: {
    ts: number;
    seq: number;
    level: LogLevel;
    levelRaw: string;
    message: string;
    key: string;
    seqLabel: string;
    stampFull: string;
  };
}>();
</script>

<template>
  <div
    class="run-log-line"
    :class="`is-${row.level}`"
    :data-testid="`logs-run-line-${row.key}`"
    :title="`${row.stampFull} · ${row.levelRaw} · 序号 ${row.seq}`"
  >
    <span class="run-log-time">{{ fmtLogTime(row.ts) }}</span>
    <span class="run-log-seq" :title="`后端日志序号（越大越新）：${row.seq}`">{{ row.seqLabel }}</span>
    <span class="run-log-level" :title="row.levelRaw">{{ levelLabel(row.level) }}</span>
    <span class="run-log-msg">{{ row.message }}</span>
  </div>
</template>