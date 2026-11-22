/**
 * 错误条 **队列**（M5 补遗：从单条升级为队列）。
 *
 * 为什么必须改：单条时后到的失败会把前一条挤掉（一次并发刷新里
 * `list_jobs` 与 `job_tab_counts` 同时失败，用户只看到后一条），
 * 而且 `app-error-dismiss` 一次清空全部。这里改成：
 *
 * - FIFO 展示**最前一条**（原型里错误条只有一行高，放不下三条）；
 * - 其余折叠成「另有 N 条更早的错误」，一条都不丢；
 * - 每条都能独立关闭，关掉第一条第二条立刻顶上；
 * - sr-only `aria-live` 播报新错误，屏读用户不会漏。
 *
 * 队列语义在 `lib_ui/error-queue.ts`（纯函数、有单测），本组件只负责渲染。
 */
<script setup lang="ts">
import { computed } from 'vue';

import {
  announceText,
  firstError,
  hasError,
  pendingCount,
  pendingText,
  type ErrorItem,
  type ErrorQueue,
} from '../lib_ui/error-queue';

const props = withDefaults(
  defineProps<{
    queue: ErrorQueue;
    /** 展示位。概览页定高，错误条一律 `flex: 0 0 auto`。 */
    testId?: string;
  }>(),
  { testId: 'app-error-bar' },
);

const emit = defineEmits<{
  (e: 'dismiss', id: number): void;
  (e: 'dismiss-all'): void;
  (e: 'retry', item: ErrorItem): void;
}>();

const head = computed(() => firstError(props.queue));
const visible = computed(() => hasError(props.queue));
const rest = computed(() => pendingCount(props.queue));
const restText = computed(() => pendingText(props.queue));
const live = computed(() => announceText(props.queue));
const canRetry = computed(() => head.value?.retryLabel !== null && head.value !== null);
</script>

<template>
  <div
    v-if="visible && head"
    class="app-error error-bar app-error"
    :data-testid="testId"
    role="alert"
  >
    <svg class="app-error-ico" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" stroke-width="1.4" />
      <path d="M8 4.5v4.2M8 11.2v.6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
    </svg>
    <span class="app-error-text">
      <code :data-testid="`${testId}-code`">{{ head.code }}</code>
      <span :data-testid="`${testId}-message`">{{ head.message }}</span>
    </span>
    <span v-if="rest > 0" class="app-error-rest" :data-testid="`${testId}-rest`" :title="restText">
      {{ restText }}
    </span>
    <span class="spacer" />
    <button
      v-if="canRetry"
      class="btn btn-sm btn-ghost"
      :data-testid="`${testId}-retry-btn`"
      @click="emit('retry', head)"
    >
      {{ head.retryLabel }}
    </button>
    <button
      v-if="rest > 0"
      class="btn btn-sm btn-ghost"
      :data-testid="`${testId}-dismiss-all`"
      @click="emit('dismiss-all')"
    >
      全部忽略
    </button>
    <button
      class="btn btn-sm btn-ghost"
      :data-testid="`${testId}-close-btn`"
      title="忽略这一条"
      @click="emit('dismiss', head.id)"
    >
      忽略
    </button>
    <span class="sr-only" :data-testid="`${testId}-live`" aria-live="assertive" aria-atomic="true">
      {{ live }}
    </span>
  </div>
</template>
