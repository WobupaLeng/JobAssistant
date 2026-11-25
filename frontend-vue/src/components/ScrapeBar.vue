<script setup lang="ts">
/**
 * 抓取条：三态按钮 + 限额 + 条件筛选入口 + 一句话进度。按钮文案与禁用规则来自
 * `lib_ui/scrape-panel-model.ts`，组件只画；`scrape_start` 由父级发，这里只 emit 意图。
 */
import { computed } from 'vue';
import { MAX_JD_CEILING } from '../lib_ui/scrape-panel-model';
import {
  buttonClass,
  buttonLabel,
  mainDisabled,
  progressText,
  srSummary,
  type ScrapeButtonState,
  type ScrapeProgressLike,
} from '../lib_ui/scrape-panel-model';

const props = withDefaults(
  defineProps<{
    progress: ScrapeProgressLike;
    state: ScrapeButtonState;
    starting?: boolean;
    maxJd?: number;
    minScore?: number;
    /** 满分来自 `get_scoring_spec`；未取到前门槛输入框不设上限。 */
    scoreMax?: number | null;
    scrapedTotal?: number;
    /** 条件筛选抽屉是否已展开（只驱动 `aria-expanded`，抽屉本体在父级）。 */
    filterOpen?: boolean;
  }>(),
  { starting: false, maxJd: 0, minScore: 0, scoreMax: null, scrapedTotal: 0, filterOpen: false },
);

const emit = defineEmits<{
  (e: 'start'): void;
  (e: 'stop'): void;
  (e: 'update:maxJd', v: number): void;
  (e: 'update:minScore', v: number): void;
  (e: 'toggle-filter'): void;
}>();

const brief = computed(() => progressText(props.progress));
const summary = computed(() => srSummary(props.progress));
const label = computed(() => buttonLabel(props.state));
const cls = computed(() => buttonClass(props.state));
// 跑起来时主按钮是「暂停抓取」，必须可点（点它 emit stop）；只有启动中禁用。
const startOff = computed(() => mainDisabled(props.starting));

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
        <!-- 口径（用户 m01540）：数的是本次抓取**真正进待投递列表**的条数，
             重复 / 低于门槛 / 被规则否决的都不算，所以文案不说「抓几条」 -->
        <span
          class="scrape-limit-label"
          title="本次抓取真正入库（进待投递列表）的条数上限；重复、低于门槛、被排除的都不算"
        >最多入库</span>
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
      <!-- 条件筛选的常驻入口：工具栏上必须有，不能只靠列表空态。
           抽屉本身挂在 `DashboardPage` 的 `.job-fill` 里，这里只管开关。 -->
      <!-- 常驻入口。用 .btn（不是 .btn-ghost）：幽灵态在抓取条底色上完全透明，
           只剩一行小字，用户会当成「按钮没了」。打开时切到 is-on 显示激活态。 -->
      <button
        class="btn btn-sm scrape-filter-btn"
        :class="{ 'is-on': filterOpen }"
        type="button"
        :aria-expanded="filterOpen ? 'true' : 'false'"
        aria-controls="scrape-filter-bar"
        title="筛选条件：城市 / 关键词 / 学历 / 薪资等"
        data-testid="scrape-filter-toggle-btn"
        @click="emit('toggle-filter')"
      >
        <svg
          class="scrape-filter-ico"
          viewBox="0 0 16 16"
          width="13"
          height="13"
          aria-hidden="true"
        >
          <path
            d="M2 3.5h12M4.5 8h7M6.5 12.5h3"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
          />
        </svg>
        筛选条件
      </button>
      <span class="scrape-brief" data-testid="scrape-brief">{{ brief }}</span>
      <span class="spacer"></span>
    </div>
    <p class="sr-only" data-testid="scrape-sr-summary" aria-live="polite">{{ summary }}</p>
  </div>
</template>
