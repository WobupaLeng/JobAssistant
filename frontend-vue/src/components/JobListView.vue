<script setup lang="ts">
/**
 * 岗位列表卡：页签 / 排序 / 批量条 / 卡片网格 / 保活锚点表。
 *
 * 所有判定（页签过滤、排序方向、计数、批量显隐）都来自
 * `features/jobs/list-model.ts`；长按状态机来自 `features/jobs/longpress.ts`。
 * 组件本身不 import 任何 `features` 之外的领域模块。
 */
import { computed } from 'vue';
import type { Job, JobTabCounts } from '../api/types';
import JobCard from './JobCard.vue';
import {
  LIST_TABS,
  SORT_FIELDS,
  allVisibleChecked,
  batchLabel,
  cardView,
  headCount,
  keepaliveRows,
  showBatchActions,
  sortArrow,
  sortSlug,
  tabCount,
  tabMeta,
  visibleJobs,
  type ListTab,
  type SortField,
} from '../lib_ui/job-list-model';
import type { SelectionState } from '../lib_ui/longpress';

const props = withDefaults(
  defineProps<{
    jobs: readonly Job[];
    counts: JobTabCounts;
    tab: ListTab;
    showAll?: boolean;
    sortField: SortField;
    sortDescending: boolean;
    selection: SelectionState;
    /** 满分来自 `get_scoring_spec`；`null` 表示还没取到，此时不算档位。 */
    scoreMax?: number | null;
    loading?: boolean;
    error?: string;
  }>(),
  { showAll: false, scoreMax: null, loading: false, error: '' },
);

const emit = defineEmits<{
  (e: 'update:tab', tab: ListTab): void;
  (e: 'update:showAll', showAll: boolean): void;
  (e: 'sort', field: SortField): void;
  (e: 'refresh'): void;
  (e: 'toggle', jobId: string): void;
  (e: 'checkAll'): void;
  (e: 'batch', op: 'reject' | 'allow'): void;
  (e: 'greet', jobId: string): void;
  (e: 'open', jobId: string): void;
  /** 空态里的两个入口：展开筛选 / 直接开始抓取（原型 `job-empty-*-btn`）。 */
  (e: 'empty-filter'): void;
  (e: 'empty-start'): void;
}>();

const visible = computed(() => visibleJobs(props.jobs, props.tab, props.showAll));
const views = computed(() => visible.value.map((j) => cardView(j, props.scoreMax)));
const anchors = computed(() => keepaliveRows(visible.value));
const visibleIds = computed(() => visible.value.map((j) => j.job_id));
const selectedCount = computed(() => props.selection.checked.length);
const selectedHere = computed(() => props.selection.checked.filter((id) => visibleIds.value.includes(id)));
const allHere = computed(() => allVisibleChecked(selectedHere.value, visibleIds.value));
const total = computed(() => headCount(props.counts, props.tab, props.showAll));
const batchVisible = computed(() => showBatchActions(selectedCount.value));

/* 列表状态播报（原型 L2193 逐字「列表状态 {mode} · 共 {total} 条 · 已选 {n} 条」）。
 * mode 用四态词 loading / error / empty / default，与右栏 rail-state 同一套口径；
 * sr-only，不参与视觉。原先这里写的是「正在加载岗位 / 已显示 N 条」，与真值不符。 */
const announceText = computed(() => {
  const mode = props.loading
    ? 'loading'
    : props.error !== ''
      ? 'error'
      : visible.value.length === 0
        ? 'empty'
        : 'default';
  return `列表状态 ${mode} · 共 ${total.value} 条 · 已选 ${selectedCount.value} 条`;
});

function onTab(tab: ListTab): void {
  if (tab === props.tab && !props.showAll) return;
  emit('update:showAll', false);
  emit('update:tab', tab);
}

function onShowAll(): void {
  emit('update:showAll', !props.showAll);
}

function tabLabel(tab: ListTab): string {
  return tabMeta(tab).label;
}

function onSort(field: SortField): void {
  emit('sort', field);
}
</script>

<template>
  <div class="card job-list" data-testid="job-list-card">
    <div class="job-tabs-row">
      <div class="job-tabs" role="tablist" aria-label="岗位页签">
        <span class="card-title" data-testid="job-list-title">岗位列表</span>
        <button
          v-for="t in LIST_TABS"
          :key="t.key"
          class="pill"
          :class="{ 'is-active': !showAll && t.key === tab }"
          type="button"
          role="tab"
          :aria-selected="!showAll && t.key === tab"
          :data-testid="`job-tab-${t.slug}-tab`"
          @click="onTab(t.key)"
        >
          {{ t.label }}
          <span class="pill-count" :data-testid="`job-tab-${t.slug}-count`">{{ tabCount(counts, t.key) }}</span>
        </button>
        <button
          class="pill"
          :class="{ 'is-active': showAll }"
          type="button"
          role="tab"
          :aria-selected="showAll"
          data-testid="job-tab-all"
          @click="onShowAll"
        >
          全部
        </button>
        <span class="card-sub" data-testid="job-total">共 {{ total }} 条</span>
        <!-- 原型 L2193 逐字：「列表状态 {mode} · 共 {total} 条 · 已选 {n} 条」。
             mode 用四态词（loading/error/empty/default），与右栏 rail-state 同源口径。 -->
        <span class="sr-only" data-testid="job-state-announce" aria-live="polite">{{ announceText }}</span>
        <span class="spacer"></span>
        <button class="btn btn-sm btn-ghost" type="button" data-testid="job-refresh-btn" @click="emit('refresh')">
          刷新
        </button>
      </div>
      <div class="job-sorts" role="radiogroup" aria-label="排序">
        <button
          v-for="s in SORT_FIELDS"
          :key="s.key"
          class="sort-pill"
          :class="{ 'is-active': s.key === sortField }"
          type="button"
          role="radio"
          :aria-checked="s.key === sortField"
          :data-testid="`job-sort-${sortSlug(s.key)}-btn`"
          @click="onSort(s.key)"
        >
          {{ s.label }}
          <span v-if="s.key === sortField" class="sort-arrow" data-testid="job-sort-arrow">{{ sortArrow(sortDescending) }}</span>
        </button>
      </div>
    </div>

    <div class="job-batch" data-testid="job-batch-bar">
      <label class="batch-check">
        <input
          type="checkbox"
          :checked="allHere"
          data-testid="job-select-all-check"
          aria-label="全选本页"
          @change="emit('checkAll')"
        />
        <span>全选本页</span>
      </label>
      <span class="batch-count" data-testid="job-selected-count">已选 {{ selectedCount }} 条</span>
      <span class="batch-total" data-testid="job-visible-total">列表共 {{ visible.length }} 条</span>
      <span class="batch-actions" :hidden="!batchVisible" data-testid="job-batch-actions">
        <button class="btn btn-sm btn-danger" type="button" data-testid="job-batch-reject-btn" @click="emit('batch', 'reject')">
          {{ batchLabel('reject', selectedCount) }}
        </button>
        <button class="btn btn-sm btn-ghost" type="button" data-testid="job-batch-allow-btn" @click="emit('batch', 'allow')">
          {{ batchLabel('allow', selectedCount) }}
        </button>
      </span>
    </div>

    <div
      class="job-grid"
      :class="{ 'is-selecting': selection.selecting }"
      :data-state="loading ? 'loading' : error ? 'error' : visible.length === 0 ? 'empty' : 'ready'"
      data-testid="job-grid"
    >
      <template v-if="loading">
        <div v-for="i in 6" :key="i" class="job-card is-skeleton" aria-hidden="true">
          <span class="skel-line" style="width: 62%"></span>
          <span class="skel-line" style="width: 40%"></span>
          <span class="skel-line" style="width: 78%"></span>
        </div>
      </template>
      <div v-else-if="error" class="job-state" data-testid="job-state-error">
        <span class="state-glyph is-bad" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="6.6" stroke="currentColor" stroke-width="1.4"/><path d="M8 4.8v3.6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="8" cy="11" r="0.9" fill="currentColor"/></svg></span>
        <span class="state-title" data-testid="job-state-error-title">岗位列表加载失败</span>
        <span class="state-sub">这次没有取到数据，可以重试一次。</span>
        <span class="state-error-detail" :title="error">{{ error }}</span>
        <!-- 原型 :2171 的错误态带一个重试按钮，接线到页面现有的刷新逻辑。 -->
        <span class="state-actions">
          <button
            class="btn btn-sm btn-primary"
            type="button"
            data-testid="job-state-error-retry-btn"
            @click="emit('refresh')"
          >
            重试
          </button>
        </span>
      </div>
      <div v-else-if="visible.length === 0" class="job-state" data-testid="job-state-empty">
        <span class="state-glyph" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 16 16" fill="none"><rect x="2.6" y="3.4" width="10.8" height="9.2" rx="2" stroke="currentColor" stroke-width="1.3"/><path d="M5.4 6.6h5.2M5.4 9.2h3.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg></span>
        <span class="state-title" data-testid="job-empty-title">暂无岗位</span>
        <span class="state-sub" data-testid="job-empty-sub">先展开筛选条件并点击开始抓取</span>
        <span class="state-actions">
          <button class="btn btn-sm" type="button" data-testid="job-empty-filter-btn" @click="emit('empty-filter')">
            展开筛选条件
          </button>
          <button class="btn btn-sm btn-primary" type="button" data-testid="job-empty-start-btn" @click="emit('empty-start')">
            开始抓取
          </button>
        </span>
      </div>
      <template v-else>
        <JobCard
          v-for="v in views"
          :key="v.jobId"
          :view="v"
          :selecting="selection.selecting"
          :selected="selection.checked.includes(v.jobId)"
          @toggle="emit('toggle', $event)"
          @greet="emit('greet', $event)"
          @open="emit('open', $event)"
        />
      </template>
    </div>

    <!-- C8：隐藏锚点表。唯一作用是为 `job-row-{id}` / `score-{id}` 保活，
         让 E2E 脚本在卡片被虚拟化/分页后仍能按 id 定位。不要删。 -->
    <table class="sr-only" aria-hidden="true">
      <tbody data-testid="job-keepalive-anchors">
        <tr v-for="a in anchors" :key="a.jobId" :data-testid="`job-row-${a.jobId}`">
          <td :data-testid="`score-${a.jobId}`">{{ a.score }}</td>
          <td>{{ a.title }}</td>
          <td>{{ a.company }}</td>
          <td>{{ a.location }}</td>
          <td>{{ a.time }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
