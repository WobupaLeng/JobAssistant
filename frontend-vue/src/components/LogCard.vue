<script setup lang="ts">
/**
 * 概览页底部的日志卡：进度统计 + 运行日志与岗位事件流**合并成一条时间线**。
 *
 * 用户 m00182 / m00374 裁决：运行日志与 ✔ / 跳过 不再分两块显示，主页一条流按时间混排，
 * 事件行补接收时刻；决策记录仍走「决策记录」按钮打开的覆盖页（m00404），**不**进这条线。
 * 页签「全部 / 错误 / 警告 / 信息 / 事件」单选，默认「全部」。
 * 保持 `scrape-feed` 锚点（C9 E2E 靠它数 ✔）。
 * 日志用 `setTimeout` 轮询，挂起时不触发，恢复时靠游标补齐。
 */
import { computed, nextTick, ref, watch } from 'vue';
import {
  lastTitleText,
  srSummary,
  type FeedLine,
  type ScrapeProgressLike,
} from '../lib_ui/scrape-panel-model';
import { fmtLogTime, type LogLevel } from '../lib_ui/logfmt';
import { mergeMix, mixEmptyText, mixTabs, pickMix, type MixRow, type MixTab } from '../lib_ui/log-mix';

export interface LogLine {
  readonly ts: number;
  readonly seq: number;
  readonly level: LogLevel;
  readonly message: string;
}

const props = withDefaults(
  defineProps<{
    progress: ScrapeProgressLike;
    lines: readonly LogLine[];
    /** 满分与门槛都来自 `get_scoring_spec`（C10），拿不到就不显示分数段。 */
    scoreMax?: number | null;
    minScore?: number | null;
    live?: boolean;
    /** 抓取事件流（`scrape_drain` 增量），入库行首 `✔`；与运行日志混排。 */
    feed?: readonly FeedLine[];
  }>(),
  { scoreMax: null, minScore: null, live: true, feed: () => [] },
);

const emit = defineEmits<{
  (e: 'open-decisions'): void;
}>();

const tabs = mixTabs();
const tab = ref<MixTab>('all');

const summary = computed(() => srSummary(props.progress));
const last = computed(() => lastTitleText(props.progress.last_title));
const spec = computed(() =>
  props.scoreMax === null ? '—' : `满分 ${props.scoreMax} · 门槛 ${props.minScore ?? '—'}`,
);

const rows = computed<MixRow[]>(() => pickMix(mergeMix(props.lines, props.feed), tab.value));
const emptyText = computed(() => mixEmptyText(tab.value));

/**
 * 日志行区贴底跟随：新行追加在末尾，只有用户手动往上翻过才停止跟随，
 * 否则卡里永远是最早的几行（卡高只放得下约 8 行），看着像「没有操作信息」。
 */
const rowsBox = ref<HTMLOListElement | null>(null);
const stickBottom = ref(true);

function onRowsScroll(): void {
  const el = rowsBox.value;
  if (el === null) return;
  stickBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight <= 8;
}

async function stickToBottom(): Promise<void> {
  if (!stickBottom.value) return;
  await nextTick();
  const el = rowsBox.value;
  if (el !== null) el.scrollTop = el.scrollHeight;
}

watch(() => rows.value.length, () => void stickToBottom());
// 换页签等于换了一屏内容：从最末尾读起，而不是接着上一个页签的滚动位置。
watch(tab, () => {
  stickBottom.value = true;
  void stickToBottom();
});
</script>

<template>
  <div class="card card-standalone log-card" data-testid="log-card">
    <div class="card-head">
      <span class="card-title">决策日志 / 运行日志</span>
      <span class="log-live" :class="{ 'is-on': live }" data-testid="log-live">
        {{ live ? '实时' : '已暂停' }}
      </span>
      <span class="chip" data-testid="score-spec">{{ spec }}</span>
      <span class="card-sub" data-testid="scrape-last-title">{{ last }}</span>
      <span class="spacer"></span>
      <!-- 单选页签（用户 m01229 / m00321）：一次只看一类，默认「全部」= 一条混排流。
           「事件」只看 ✔ / 跳过。「决策记录」按钮打开覆盖页，决策不混进这条线（m00404）。 -->
      <div class="log-tabs" role="tablist" aria-label="日志筛选" data-testid="log-tabs">
        <button
          v-for="t in tabs"
          :key="t.key"
          class="log-tab"
          :class="{ 'is-active': tab === t.key }"
          :data-tab="t.key"
          type="button"
          role="tab"
          :aria-selected="tab === t.key"
          :data-testid="`log-tab-${t.key}`"
          @click="tab = t.key"
        >
          {{ t.label }}
        </button>
      </div>
      <button class="btn btn-sm btn-ghost" type="button" data-testid="log-decisions-btn" @click="emit('open-decisions')">
        决策记录
      </button>
    </div>
    <div class="card-body">
      <p class="sr-only" data-testid="scrape-sr-summary" aria-live="polite">{{ summary }}</p>
      <!-- `scrape-feed` 锚点保留（C9：E2E 在这个容器里数 ✔）。事件行已并入下面的同一条
           时间线，不再是独立的第二块列表。 -->
      <div class="log-feed" data-testid="scrape-feed">
        <ol ref="rowsBox" class="log-rows" data-testid="log-rows" @scroll.passive="onRowsScroll">
          <li v-if="rows.length === 0" class="empty" data-testid="log-empty">
            {{ emptyText }}
          </li>
          <li
            v-for="r in rows"
            :key="r.key"
            class="log-row"
            :class="r.kind === 'feed' ? `is-feed is-${r.tone}` : ''"
            data-testid="log-row"
            :data-level="r.kind === 'feed' ? 'event' : r.level"
          >
            <span class="log-time">{{ fmtLogTime(r.ts) }}</span>
            <template v-if="r.kind === 'log'">
              <span class="log-level" :class="`is-${r.level}`">{{ r.level.toUpperCase() }}</span>
              <span class="log-msg">{{ r.message }}</span>
            </template>
            <template v-else>
              <span class="log-level feed-tag" :class="`is-${r.tone}`">{{ r.tag }}</span>
              <span class="log-msg">{{ r.msg }}</span>
            </template>
          </li>
        </ol>
      </div>
    </div>
  </div>
</template>