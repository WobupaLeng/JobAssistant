<script setup lang="ts">
/**
 * 抓取条：折叠筛选 + 三态按钮 + 限额 + 一句话进度（C9 文案）。
 *
 * 所有文案与禁用规则来自 `features/scrape/panel-model.ts`，组件只画。
 * `scrape_start` 由父级发（它要包一层 `{args}`，并且要处理返回值），
 * 这里只 emit 意图。
 */
import { computed } from 'vue';
import { MAX_JD_CEILING } from '../lib_ui/scrape-panel-model';
import {
  buttonClass,
  buttonLabel,
  progressText,
  srSummary,
  startDisabled,
  type ScrapeButtonState,
  type ScrapeProgressLike,
} from '../lib_ui/scrape-panel-model';

const props = withDefaults(
  defineProps<{
    progress: ScrapeProgressLike;
    state: ScrapeButtonState;
    starting?: boolean;
    filterOpen?: boolean;
    maxJd?: number;
    minScore?: number;
    /** 满分来自 `get_scoring_spec`；未取到前门槛输入框不设上限。 */
    scoreMax?: number | null;
    scrapedTotal?: number;
  }>(),
  { starting: false, filterOpen: false, maxJd: 0, minScore: 0, scoreMax: null, scrapedTotal: 0 },
);

const emit = defineEmits<{
  (e: 'toggle-filter'): void;
  (e: 'start'): void;
  (e: 'stop'): void;
  (e: 'update:maxJd', v: number): void;
  (e: 'update:minScore', v: number): void;
}>();

const brief = computed(() => progressText(props.progress));
const summary = computed(() => srSummary(props.progress));
const label = computed(() => buttonLabel(props.state));
const cls = computed(() => buttonClass(props.state));
const startOff = computed(() => startDisabled(props.starting, props.state === 'running'));

function onMaxJd(e: Event): void {
  const n = Number.parseInt((e.target as HTMLInputElement).value, 10);
  emit('update:maxJd', Number.isFinite(n) ? Math.min(MAX_JD_CEILING, Math.max(1, n)) : props.maxJd);
}

function onMinScore(e: Event): void {
  const raw = Number.parseInt((e.target as HTMLInputElement).value, 10);
  const max = props.scoreMax ?? Number.MAX_SAFE_INTEGER;
  emit('update:minScore', Number.isFinite(raw) ? Math.min(max, Math.max(0, raw)) : props.minScore);
}

function onMain(): void {
  if (props.state === 'running') emit('stop');
  else emit('start');
}
</script>

<template>
  <div class="card scrape-card">
    <div class="scrape-bar">
      <button
        class="chip as-btn scrape-toggle"
        :aria-expanded="filterOpen"
        type="button"
        data-testid="scrape-filter-toggle-btn"
        @click="emit('toggle-filter')"
      >
        {{ filterOpen ? '收起筛选 ▲' : '筛选条件 ▼' }}
      </button>
      <button
        :class="cls"
        type="button"
        :disabled="startOff"
        :data-state="state"
        data-testid="scrape-start-btn"
        @click="onMain"
      >
        {{ starting ? '启动中…' : label }}
      </button>
      <button
        class="btn btn-sm btn-danger"
        type="button"
        :disabled="state !== 'running'"
        data-testid="scrape-stop-btn"
        @click="emit('stop')"
      >
        停止抓取
      </button>
      <label class="scrape-limit">
        <span class="scrape-limit-label">最多 JD</span>
        <input
          class="input input-num"
          type="number"
          min="1"
          :max="MAX_JD_CEILING"
          :value="maxJd"
          data-testid="scrape-max-jd-input"
          aria-label="最多 JD"
          @change="onMaxJd"
        />
      </label>
      <label class="scrape-limit">
        <span class="scrape-limit-label">最低分</span>
        <input
          class="input input-num"
          type="number"
          min="0"
          :max="scoreMax ?? undefined"
          :value="minScore"
          data-testid="scrape-min-score-input"
          aria-label="最低分"
          @change="onMinScore"
        />
      </label>
      <span class="scrape-brief" data-testid="scrape-brief">{{ brief }}</span>
      <span class="spacer"></span>
    </div>
    <p class="sr-only" data-testid="scrape-sr-summary" aria-live="polite">{{ summary }}</p>
  </div>
</template>
