<script setup lang="ts">
/**
 * 概览页底部的日志卡：进度统计 + 实时日志行。标题逐字「决策日志 / 运行日志」，斜杠两侧各一
 * 空格（并列，不是页签）。进度两套都渲染：可见 Stat 五段给眼睛，sr-only 五段给读屏与 E2E。
 * 日志用 `setTimeout` 轮询，挂起时不触发，恢复时靠游标补齐。
 */
import { computed } from 'vue';
import {
  lastTitleText,
  srSummary,
  type FeedLine,
  type ScrapeProgressLike,
} from '../lib_ui/scrape-panel-model';
import { fmtLogTime, type LogLevel } from '../lib_ui/logfmt';

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
    /** 抓取事件流（`scrape_drain` 增量），入库行首 `✔`。 */
    feed?: readonly FeedLine[];
  }>(),
  { scoreMax: null, minScore: null, live: true, feed: () => [] },
);

const emit = defineEmits<{
  (e: 'open-decisions'): void;
}>();

const summary = computed(() => srSummary(props.progress));
const last = computed(() => lastTitleText(props.progress.last_title));
const spec = computed(() =>
  props.scoreMax === null ? '—' : `满分 ${props.scoreMax} · 门槛 ${props.minScore ?? '—'}`,
);
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
      <button class="btn btn-sm btn-ghost" type="button" data-testid="log-decisions-btn" @click="emit('open-decisions')">
        决策记录
      </button>
    </div>
    <div class="card-body">
      <p class="sr-only" data-testid="scrape-sr-summary" aria-live="polite">{{ summary }}</p>
      <ol class="log-rows" data-testid="log-rows">
        <li v-if="lines.length === 0" class="empty" data-testid="log-empty">
          尚无事件。点击「开始抓取」后这里会滚动显示进度。
        </li>
        <li v-for="l in lines" :key="`${l.ts}-${l.seq}`" class="log-row" data-testid="log-row" :data-level="l.level">
          <span class="log-time">{{ fmtLogTime(l.ts) }}</span>
          <span class="log-level" :class="`is-${l.level}`">{{ l.level.toUpperCase() }}</span>
          <span class="log-msg">{{ l.message }}</span>
        </li>
      </ol>
      <!-- 事件流（`scrape_drain` 的岗位行）。入库行首标记 `✔` 逐字（C9），
           E2E 靠 feed.count("✔") 数入库条数。 -->
      <ol class="job-feed" data-testid="scrape-feed">
        <li v-for="(line, i) in feed" :key="i" class="feed-line" :class="`is-${line.tone}`">
          <span class="feed-tag">{{ line.tag }}</span>
          <span class="feed-msg">{{ line.msg }}</span>
        </li>
      </ol>
    </div>
  </div>
</template>
