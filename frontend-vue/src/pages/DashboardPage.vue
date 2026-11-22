<script setup lang="ts">
/**
 * 概览页（M1）：抓取条 + 筛选面板 + 岗位列表 + 最近事件。
 *
 * 本页只做编排：
 * - 读：进页拉一次 `list_jobs` / `job_tab_counts` / `scrape_progress` /
 *   `get_scoring_spec` / `get_app_config`，之后 `job_tab_counts` 每 5 秒轮询
 *   （**只为页签数字**，不重拉列表——重拉会清掉勾选与滚动位置）。
 * - 写：筛选改动 400ms 防抖后只发 diff（C12）；批量标记走 `set_job_flag` 系；
 *   抓取按钮走 `scrape_start`（**必须包 `{args}`**）与 `scrape_stop`。
 *
 * 所有判定都在 `features/*` 的纯函数里，本文件不做业务计算。
 */
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import AppErrorBar from '../components/AppErrorBar.vue';
import FilterPanel from '../components/FilterPanel.vue';
import JobListView from '../components/JobListView.vue';
import LogCard, { type LogLine } from '../components/LogCard.vue';
import ScrapeBar from '../components/ScrapeBar.vue';
import SensitivePanel from '../components/SensitivePanel.vue';
import TopbarTail from '../components/TopbarTail.vue';
import {
  allowJobs,
  jobTabCounts,
  listJobs,
  markDelivered,
  rejectJobs,
} from '../api/commands/db';
import { listLogs } from '../api/commands/logs';
import { getScoringSpec } from '../api/commands/shell';
import { scrapeProgress, scrapeStart, scrapeStop } from '../api/commands/scrape';
import { getAppConfig, setScrapeDefaults } from '../api/commands/config';
import { sensitiveList, sensitiveRemove } from '../api/commands/sensitive';
import type { AppConfigReply, Job, JobTabCounts, SensitivePage } from '../api/types';
import { setGreetDraft } from '../features/chat/greet-draft';
import {
  LIST_TAB_DEFAULT,
  SORT_DEFAULT,
  defaultDescending,
  emptyTabCounts,
  showBatchActions,
  sortAscending,
  sortOrder,
  tabIncludeRejected,
  tabQuery,
  toggleChecked,
  type ListTab,
  type SortField,
} from '../lib_ui/job-list-model';
import {
  armLongPress,
  clickBlank,
  clickCard,
  exitSelecting,
  ignorePointerTarget,
  interactiveSelector,
  movedBeyond,
  setChecked,
  type PointerSample,
  type SelectionState,
  type TimerHandle,
} from '../lib_ui/longpress';
import {
  buttonState,
  fromProgress,
  scoreModeSegs,
  startDisabled,
  tokenText,
  type ScrapeProgressLike,
} from '../lib_ui/scrape-panel-model';
import {
  SAVE_DEBOUNCE_MS,
  createDraft,
  diffArgs,
  encode,
  applyBackend,
  type FilterDraft,
} from '../lib_ui/scrape-filter-model';
import { loadDraft, saveDraft } from '../features/scrape/filter';
import { pageTestId } from '../lib_ui/nav';
import { parseLogLevel } from '../lib_ui/logfmt';
import { readInitialTheme, flipTheme, applyTheme } from '../lib_ui/theme';

/**
 * 跳页：本页不直接改 `App.vue` 的 nav，交给父级。
 *
 * 为什么不在页面里 import 路由：C13「页面分发是插槽」——页面反过来知道
 * 路由的存在就会形成回环。父级监听 `navigate` 再切换。
 */
const props = defineProps<{ activeNav?: string }>();
const emit = defineEmits<{
  (e: 'navigate', nav: 'dashboard' | 'chat' | 'config' | 'logs'): void;
}>();

const TICK_MS = 5000;
const LOG_POLL_MS = 2000;

const jobs = shallowRef<Job[]>([]);
const counts = ref<JobTabCounts>(emptyTabCounts());
const tab = ref<ListTab>(LIST_TAB_DEFAULT);
const showAll = ref(false);
const sortField = ref<SortField>(SORT_DEFAULT);
const sortDescending = ref(defaultDescending(SORT_DEFAULT));
const listLoading = ref(false);
const listError = ref('');

const selection = ref<SelectionState>({ selecting: false, checked: [] });

const progress = shallowRef<ScrapeProgressLike>(fromProgress(null));
const pausedByUser = ref(false);
const starting = ref(false);
const filterOpen = ref(false);

const scoreMax = ref<number | null>(null);
const minScore = ref<number | null>(null);
const maxJd = ref(0);
const config = shallowRef<AppConfigReply | null>(null);
const tokensTotal = computed(() => config.value?.tokens_total ?? 0);

const draft = ref<FilterDraft>(createDraft());
const savedKnown = ref<Record<string, unknown>>({});
const saving = ref(false);
const filterNote = ref('');

const logLines = shallowRef<LogLine[]>([]);
const logSince = ref<number>(0);

const sensitiveOpen = ref(false);
const sensitivePage = shallowRef<SensitivePage>({ items: [], total: 0 });
const sensitiveLoading = ref(false);
const sensitiveError = ref('');

const errorMessage = ref('');
const errorCode = ref('');
const themeLabel = ref(readInitialTheme() === 'dark' ? '深色' : '浅色');

let tickTimer: ReturnType<typeof setInterval> | null = null;
let logTimer: ReturnType<typeof setTimeout> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;
let pressTimer: TimerHandle | null = null;
let pressStart: PointerSample | null = null;
let pressJobId = '';

const segs = computed(() => scoreModeSegs('rule'));
const state = computed(() => buttonState(progress.value.running, pausedByUser.value));

// ---------------------------------------------------------------- 读

async function refreshList(): Promise<void> {
  listLoading.value = true;
  listError.value = '';
  try {
    const [items, c] = await Promise.all([
      listJobs({
        include_rejected: showAll.value || tabIncludeRejected(tab.value),
        rejected_only: false,
        ascending: sortAscending(sortDescending.value),
        limit: 500,
        sort: sortField.value,
        tab: tabQuery(tab.value, showAll.value),
        order: sortOrder(sortDescending.value),
      }),
      jobTabCounts(),
    ]);
    jobs.value = items;
    counts.value = c;
  } catch (e) {
    listError.value = errText(e);
    errorMessage.value = `岗位列表加载失败：${listError.value}`;
    errorCode.value = 'list_jobs';
  } finally {
    listLoading.value = false;
  }
}

/** 只刷页签数字。5 秒一次，不碰列表本体（`state.rs:170-186` 的旧版行为）。 */
async function refreshCounts(): Promise<void> {
  try {
    counts.value = await jobTabCounts();
  } catch {
    /* 轮询失败不打断用户：下一轮自会重试 */
  }
}

async function refreshProgress(): Promise<void> {
  try {
    progress.value = fromProgress(await scrapeProgress());
  } catch {
    /* 同上：进度是辅助信息 */
  }
}

async function loadLogs(): Promise<void> {
  try {
    const reply = await listLogs(logSince.value);
    const incoming = reply.items
      .map((r) => ({ ts: r.ts, seq: r.seq, level: parseLogLevel(r.level), message: r.message }))
      .filter((r) => r.ts > logSince.value || logSince.value === 0);
    if (incoming.length > 0) {
      const seen = new Set(logLines.value.map((l) => `${l.ts}|${l.level}|${l.message}`));
      const merged = [...logLines.value, ...incoming.filter((l) => !seen.has(`${l.ts}|${l.level}|${l.message}`))];
      logLines.value = merged.slice(-200);
      logSince.value = Math.max(...incoming.map((l) => l.ts));
    }
  } catch {
    /* 日志失败不提示：它本身就是诊断通道 */
  }
}

async function loadSpecAndConfig(): Promise<void> {
  try {
    const spec = await getScoringSpec();
    scoreMax.value = spec.score_max;
    minScore.value = spec.score_min_default;
  } catch (e) {
    errorMessage.value = `计分规格读取失败：${errText(e)}`;
    errorCode.value = getScoringSpec.name;
  }
  try {
    const cfg = await getAppConfig();
    config.value = cfg;
    if (scoreMax.value === null) scoreMax.value = cfg.score_max;
    if (minScore.value === null) minScore.value = cfg.min_score;
    maxJd.value = cfg.max_jd;
    // C12：后端权威优先；它一个字段都没给才回落本地草稿。
    const next = createDraft();
    const adopted = applyBackend(next, cfg.scrape_defaults);
    draft.value = adopted > 0 ? next : (loadDraft() ?? next);
    savedKnown.value = adopted > 0 ? { ...(cfg.scrape_defaults ?? {}) } : {};
    if (adopted === 0) saveDraft(draft.value);
  } catch (e) {
    errorMessage.value = `配置读取失败：${errText(e)}`;
    errorCode.value = 'get_app_config';
  }
}

// ---------------------------------------------------------------- 写：筛选

function onDraft(next: FilterDraft): void {
  draft.value = next;
  saveDraft(next); // 本地兜底立刻写，不等防抖
  if (saveTimer !== null) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void persistDraft(), SAVE_DEBOUNCE_MS);
}

/** 只发 diff：`{}` 一律不发（发空写会清空后端的其它项）。 */
async function persistDraft(): Promise<void> {
  saveTimer = null;
  const diff = diffArgs(savedKnown.value as never, draft.value);
  if (Object.keys(diff).length === 0) return;
  saving.value = true;
  try {
    await setScrapeDefaults(diff);
    savedKnown.value = { ...savedKnown.value, ...diff };
    filterNote.value = '筛选条件已保存';
  } catch (e) {
    filterNote.value = `保存失败：${errText(e)}`;
    errorMessage.value = `筛选条件保存失败：${errText(e)}`;
    errorCode.value = 'set_scrape_defaults';
  } finally {
    saving.value = false;
  }
}

function applyNow(): void {
  if (saveTimer !== null) clearTimeout(saveTimer);
  void persistDraft();
}

function resetFilter(): void {
  onDraft(createDraft());
  applyNow();
}

function onReadExpect(): void {
  filterNote.value = '期望职位来自配置页，本轮未接线';
}

// ---------------------------------------------------------------- 写：列表操作

function onSort(field: SortField): void {
  if (field === sortField.value) sortDescending.value = !sortDescending.value;
  else {
    sortField.value = field;
    sortDescending.value = defaultDescending(field);
  }
  void refreshList();
}

function onTab(next: ListTab): void {
  tab.value = next;
  void refreshList();
}

function onShowAll(next: boolean): void {
  showAll.value = next;
  void refreshList();
}

async function batch(op: 'reject' | 'allow'): Promise<void> {
  const ids = selection.value.checked.slice();
  if (!showBatchActions(ids.length)) return;
  try {
    const hit = op === 'reject' ? await rejectJobs(ids) : await allowJobs(ids);
    filterNote.value = `${op === 'reject' ? '不投递' : '允许投递'}：影响 ${hit} 条（勾选 ${ids.length} 条）`;
    selection.value = exitSelecting();
    await refreshList();
  } catch (e) {
    errorMessage.value = `批量操作失败：${errText(e)}`;
    errorCode.value = op === 'reject' ? 'reject_jobs' : 'allow_jobs';
  }
}

async function onGreet(jobId: string): Promise<void> {
  const job = jobs.value.find((j) => j.job_id === jobId);
  setGreetDraft(job?.company ?? '', job?.title ?? '');
  // M1 只把意图存好并切页；好友匹配与填入输入框是 M2 的事。
  emit('navigate', 'chat');
}

// ---------------------------------------------------------------- 长按

function cardOf(target: EventTarget | null): HTMLElement | null {
  if (!(target instanceof HTMLElement)) return null;
  return target.closest<HTMLElement>('[data-card]');
}

function onGridPointerDown(e: PointerEvent): void {
  const card = cardOf(e.target);
  if (!card) return;
  const el = e.target as HTMLElement;
  if (ignorePointerTarget(el.tagName, el.matches(interactiveSelector()))) return;
  pressJobId = card.dataset.card ?? '';
  pressStart = { x: e.clientX, y: e.clientY };
  pressTimer = armLongPress(() => {
    pressTimer = null;
    if (pressJobId === '') return;
    selection.value = {
      selecting: true,
      checked: selection.value.selecting
        ? toggleChecked(selection.value.checked, pressJobId)
        : [pressJobId],
    };
  });
}

function onGridPointerMove(e: PointerEvent): void {
  if (!pressTimer || !pressStart) return;
  if (movedBeyond(pressStart, { x: e.clientX, y: e.clientY })) cancelPress();
}

function cancelPress(): void {
  if (pressTimer) pressTimer.cancel();
  pressTimer = null;
  pressStart = null;
}

function onGridClick(e: MouseEvent): void {
  const card = cardOf(e.target);
  if (!card) {
    selection.value = clickBlank(selection.value);
    return;
  }
  const id = card.dataset.card ?? '';
  if (id === '') return;
  selection.value = clickCard(selection.value, id);
}

// ---------------------------------------------------------------- 抓取

async function onStart(): Promise<void> {
  if (startDisabled(starting.value, progress.value.running)) return;
  starting.value = true;
  try {
    await scrapeStart({
      platform: 'boss',
      keywords: draft.value.exclude
        .split(/[\s,，、;；|]+/)
        .map((s) => s.trim())
        .filter((s) => s !== ''),
      maxJd: maxJd.value,
      minScore: minScore.value ?? 0,
    });
    pausedByUser.value = false;
    await refreshProgress();
  } catch (e) {
    errorMessage.value = `启动抓取失败：${errText(e)}`;
    errorCode.value = 'scrape_start';
  } finally {
    starting.value = false;
  }
}

async function onStop(): Promise<void> {
  try {
    await scrapeStop();
    pausedByUser.value = true;
    await refreshProgress();
  } catch (e) {
    errorMessage.value = `停止抓取失败：${errText(e)}`;
    errorCode.value = 'scrape_stop';
  }
}

// ---------------------------------------------------------------- 敏感库

async function loadSensitive(kind: 'all' | 'stored' | 'rejected'): Promise<void> {
  sensitiveLoading.value = true;
  sensitiveError.value = '';
  try {
    sensitivePage.value = await sensitiveList({ kind, limit: 200 });
  } catch (e) {
    sensitiveError.value = errText(e);
  } finally {
    sensitiveLoading.value = false;
  }
}

async function removeSensitive(ids: number[]): Promise<void> {
  try {
    await sensitiveRemove(ids);
    await loadSensitive('all');
  } catch (e) {
    sensitiveError.value = errText(e);
  }
}

const pageId = pageTestId('dashboard');

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

// ---------------------------------------------------------------- 生命周期

onMounted(() => {
  void loadSpecAndConfig();
  void refreshList();
  void refreshProgress();
  void loadLogs();
  // C16：setInterval 在 WebView 挂起时同样不触发；恢复后下一次立即生效。
  tickTimer = setInterval(() => void refreshCounts(), TICK_MS);
  const pollLogs = (): void => {
    logTimer = setTimeout(() => {
      void loadLogs();
      pollLogs();
    }, LOG_POLL_MS);
  };
  pollLogs();
});

onBeforeUnmount(() => {
  if (tickTimer !== null) clearInterval(tickTimer);
  if (logTimer !== null) clearTimeout(logTimer);
  if (saveTimer !== null) clearTimeout(saveTimer);
  cancelPress();
});

watch(
  () => tab.value,
  () => {
    selection.value = exitSelecting();
  },
);
</script>

<template>
  <section class="page page-dashboard" :data-testid="pageId">
    <AppErrorBar
      :message="errorMessage"
      :code="errorCode"
      @retry="refreshList()"
      @close="
        () => {
          errorMessage = '';
          errorCode = '';
        }
      "
    />

    <TopbarTail
      :score-max="scoreMax"
      :min-score="minScore"
      :segs="segs"
      :tokens-total="tokensTotal"
      :sensitive-open="sensitiveOpen"
      :theme-label="themeLabel"
      @toggle-sensitive="
        () => {
          sensitiveOpen = !sensitiveOpen;
        }
      "
      @open-config="emit('navigate', 'config')"
      @toggle-theme="
        () => {
          const t = flipTheme(themeLabel === '深色' ? 'dark' : 'light');
          applyTheme(t);
          themeLabel = t === 'dark' ? '深色' : '浅色';
        }
      "
    />

    <ScrapeBar
      :progress="progress"
      :state="state"
      :starting="starting"
      :filter-open="filterOpen"
      :max-jd="maxJd"
      :min-score="minScore ?? 0"
      :score-max="scoreMax"
      :scraped-total="progress.inserted"
      @toggle-filter="filterOpen = !filterOpen"
      @start="onStart"
      @stop="onStop"
      @update:max-jd="maxJd = $event"
      @update:min-score="minScore = $event"
    />

    <div v-if="filterOpen" class="card scrape-filter-bar" data-testid="scrape-filter-bar">
      <FilterPanel
        :draft="draft"
        :saving="saving"
        :note="filterNote"
        @update:draft="onDraft"
        @apply="applyNow"
        @cancel="filterOpen = false"
        @reset="resetFilter"
        @read-expect="onReadExpect"
      />
    </div>

    <div @pointerdown="onGridPointerDown" @pointermove="onGridPointerMove" @pointerup="cancelPress" @pointerleave="cancelPress" @pointercancel="cancelPress" @click="onGridClick">
      <JobListView
        :jobs="jobs"
        :counts="counts"
        :tab="tab"
        :show-all="showAll"
        :sort-field="sortField"
        :sort-descending="sortDescending"
        :selection="selection"
        :score-max="scoreMax"
        :loading="listLoading"
        :error="listError"
        @update:tab="onTab"
        @update:show-all="onShowAll"
        @sort="onSort"
        @refresh="refreshList()"
        @toggle="selection = { ...selection, checked: toggleChecked(selection.checked, $event) }"
        @check-all="selection = setChecked(selection.checked.length > 0 ? [] : jobs.map((j) => j.job_id))"
        @batch="batch"
        @greet="onGreet"
      />
    </div>

    <LogCard
      :progress="progress"
      :lines="logLines"
      :score-max="scoreMax"
      :min-score="minScore"
      :live="true"
      @open-decisions="emit('navigate', 'logs')"
    />

    <SensitivePanel
      :open="sensitiveOpen"
      :page="sensitivePage"
      :loading="sensitiveLoading"
      :error="sensitiveError"
      @close="sensitiveOpen = false"
      @reload="loadSensitive"
      @remove="removeSensitive"
    />
  </section>
</template>
