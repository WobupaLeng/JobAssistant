<script setup lang="ts">
/**
 * 决策记录页，固定窗双视图：决策日志（`list_decisions`）与运行日志（`list_logs` seq 游标增量）。
 * 增量走 seq 游标：`since` 用回执 `now` 保证不漏，`afterSeq` 过滤闭区间重送保证不重。自动刷新仅
 * 在「运行日志 + 开关开」时每 2 秒发一次 IPC，挂起时自然停摆，离开本页即杀。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { clearLogs, listLogs } from '../api/commands/logs';
import { countDecisions, decisionCounts, listDecisions } from '../api/commands/db';
import { allLevels, decisionTone, levelCn, type LogLevel } from '../lib_ui/logfmt';
import {
  AUTO_REFRESH_MS,
  DECISION_FILTERS,
  DECISION_PAGE_SIZE,
  INITIAL_CURSOR,
  LOG_VIEWS,
  advanceCursor,
  countLevels,
  decisionCountLabel,
  decisionRowFromRecord,
  filterDecisions,
  filterRuns,
  levelCountsText,
  mergeRunRows,
  rowsFromReply,
  runCountLabel,
  toggleLevel,
  type DecisionFilter,
  type DecisionRowView,
  type LogCursor,
  type LogView,
  type RunRow,
} from '../features/logs/model';
import DecisionRow from '../components/logs/DecisionRow.vue';
import RunLine from '../components/logs/RunLine.vue';

defineProps<{ activeNav?: string }>();
const emit = defineEmits<{
  /** 浮层窗只有「回到概览」一个去向：聊天不是页（SPEC-001 AC9）。 */
  (e: 'navigate', nav: 'dashboard'): void;
}>();

const view = ref<LogView>('decision');

const runRows = ref<RunRow[]>([]);
const decisions = ref<DecisionRowView[]>([]);
const runLoading = ref(true);
const decLoading = ref(true);
const decLoadingMore = ref(false);
const decDone = ref(false);
/** 库里决策总数（不受已加载条数影响）。 */
const decTotal = ref(0);
/**
 * 各档（按 `decisionTone` 归属）的**全库**条数。界面只加载了前几页，用已加载行去数必然
 * 偏小，所以计数一律走后端 `decision_counts`。
 */
const decToneCounts = ref<Record<'bad' | 'primary' | 'other', number>>({
  bad: 0,
  primary: 0,
  other: 0,
});
const runError = ref('');
const decError = ref('');

const levels = ref<LogLevel[]>([]);
const runKeyword = ref('');
const decKeyword = ref('');
const decFilter = ref<DecisionFilter>('all');
const runAuto = ref(true);
const confirmingClear = ref(false);
const justCleared = ref(false);
const refreshing = ref(false);

let cursor: LogCursor = { ...INITIAL_CURSOR };

const visibleRuns = computed(() => filterRuns(runRows.value, levels.value, runKeyword.value));
const visibleDecisions = computed(() =>
  filterDecisions(decisions.value, decFilter.value, decKeyword.value),
);
const runFiltered = computed(() => levels.value.length > 0 || runKeyword.value.trim() !== '');
const decFiltered = computed(() => decFilter.value !== 'all' || decKeyword.value.trim() !== '');
const runCount = computed(() =>
  runCountLabel(visibleRuns.value.length, runRows.value.length, runFiltered.value),
);
/** 全量（不受筛选影响）的级别分档条数：只看总数时，出错还得自己一屏屏数。 */
const runLevelsText = computed(() => levelCountsText(countLevels(runRows.value)));
/**
 * 生效中的筛选条件文案。级别筛选最容易被误判成「没生效」——同一种级别的行占了全部时，
 * 点开它行数当然不变；把条件直接写在计数旁边，用户一眼就能确认自己点到了什么。
 */
const runFilterText = computed(() => {
  const parts: string[] = [];
  const picked = allLevels().filter((lv) => levels.value.includes(lv));
  if (picked.length > 0) parts.push(`级别 ${picked.map(levelCn).join(' / ')}`);
  const kw = runKeyword.value.trim();
  if (kw !== '') parts.push(`关键字「${kw}」`);
  return parts.join(' · ');
});
/**
 * 是否跟在最新一条后面滚（正序时最新在最下面）。
 * 初值 false：首次载入停在最上面：第一行在最上方、从上往下读，
 * 一进来就跳到末尾等于把第一条藏起来。滚到底部后自动接管跟随，再往上翻就脱钩。
 */
const stickBottom = ref(false);
/** 是否停在最上面。和 stickBottom 一起把「回到最新」限制在「看中间某段历史」时出现。 */
const atTop = ref(true);
const runBox = ref<HTMLElement | null>(null);
/** 「全部」的全库条数；翻页可能先于计数返回，这里兜住已加载条数。 */
const decAllTotal = computed(() => Math.max(decTotal.value, decisions.value.length));

/** 某一档的全库条数（「全部」用总计数，其余用后端分档计数）。 */
function decFilterTotal(slug: DecisionFilter): number {
  switch (slug) {
    case 'reject':
      return decToneCounts.value.bad;
    case 'accept':
      return decToneCounts.value.primary;
    case 'other':
      return decToneCounts.value.other;
    default:
      return decAllTotal.value;
  }
}

const decCount = computed(() =>
  decisionCountLabel(
    visibleDecisions.value.length,
    // 总数走后端计数：界面只加载了前几页，decisions.length 不等于库里总条数
    decFilterTotal(decFilter.value),
    decFiltered.value,
  ),
);

/** 决策视图的生效筛选条件文案（与运行日志同口径）。 */
const decFilterText = computed(() => {
  const parts: string[] = [];
  const hit = DECISION_FILTERS.find((f) => f.slug === decFilter.value);
  if (hit !== undefined && hit.slug !== 'all') parts.push(`行为 ${hit.label}`);
  const kw = decKeyword.value.trim();
  if (kw !== '') parts.push(`关键字「${kw}」`);
  return parts.join(' · ');
});

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

async function loadRuns(full: boolean): Promise<void> {
  if (full) {
    cursor = { ...INITIAL_CURSOR };
    runRows.value = [];
    justCleared.value = false;
    runLoading.value = true;
  }
  runError.value = '';
  try {
    const reply = await listLogs(cursor.since);
    const incoming = rowsFromReply(reply.items);
    runRows.value = mergeRunRows(runRows.value, incoming, cursor.afterSeq);
    cursor = advanceCursor(cursor, incoming, reply.now);
  } catch (e) {
    runError.value = errText(e);
  } finally {
    if (full) runLoading.value = false;
  }
}

/** 第一页（最新 10 条）：进页面与手动刷新都走这里，会把已加载的旧页丢掉重来。 */
async function loadDecisions(): Promise<void> {
  decLoading.value = true;
  decLoadingMore.value = false;
  decDone.value = false;
  decError.value = '';
  try {
    const [rows, total, counts] = await Promise.all([
      listDecisions(DECISION_PAGE_SIZE, null, null),
      countDecisions().catch(() => 0),
      decisionCounts().catch(() => []),
    ]);
    decisions.value = rows.map(decisionRowFromRecord);
    decTotal.value = total;
    const tones = { bad: 0, primary: 0, other: 0 };
    for (const c of counts) {
      const t = decisionTone(c.decision);
      if (t === 'bad') tones.bad += c.count;
      else if (t === 'primary') tones.primary += c.count;
      else if (t === 'default') tones.other += c.count; // 重算(accent) 不计入这三档
    }
    decToneCounts.value = tones;
    decDone.value = rows.length < DECISION_PAGE_SIZE;
  } catch (e) {
    decError.value = errText(e);
  } finally {
    decLoading.value = false;
  }
}

/**
 * 无限滚动：拿当前最后一条的 id 当 `beforeId` 再取 10 条更旧的（`afterId` 只能取比游标
 * 更新的记录，往下翻必须用 `beforeId`）。不足一页即到底；到底或正在加载时直接返回。
 */
async function loadMoreDecisions(): Promise<void> {
  if (decLoading.value || decLoadingMore.value || decDone.value) return;
  const last = decisions.value[decisions.value.length - 1];
  if (!last) {
    decDone.value = true;
    return;
  }
  decLoadingMore.value = true;
  decError.value = '';
  try {
    const rows = await listDecisions(DECISION_PAGE_SIZE, null, last.id);
    const seen = new Set(decisions.value.map((r) => r.id));
    const fresh = rows.map(decisionRowFromRecord).filter((r) => !seen.has(r.id));
    decisions.value = [...decisions.value, ...fresh];
    decDone.value = rows.length < DECISION_PAGE_SIZE;
    if (decTotal.value < decisions.value.length) decTotal.value = decisions.value.length;
  } catch (e) {
    decError.value = errText(e);
  } finally {
    decLoadingMore.value = false;
  }
}

/** 距底部 120px 内就预取下一页。 */
function onDecisionScroll(ev: Event): void {
  const el = ev.target as HTMLElement | null;
  if (!el) return;
  if (el.scrollHeight - el.scrollTop - el.clientHeight <= 120) void loadMoreDecisions();
}

/**
 * 运行日志的跟随：列表按时间**正序**（第一行在最上面、最新一条在最底下），
 * 所以「跟随最新」= 贴到底部。8px 容差吃掉滚动条像素取整；
 * 用户往上翻看历史时自动跟随停止，点「回到最新」再恢复。
 */
function onRunScroll(ev: Event): void {
  const el = ev.target as HTMLElement | null;
  if (!el) return;
  atTop.value = el.scrollTop <= 8;
  stickBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight <= 8;
}

async function jumpLatest(): Promise<void> {
  stickBottom.value = true;
  atTop.value = false;
  await nextTick();
  const el = runBox.value;
  el?.scrollTo({ top: el.scrollHeight });
}

/*
 * 筛选条件一变就回到列表第一行：过滤后行数变少，滚动位置会停在原处（浏览器按比例夹住），
 * 看着像「点了没反应」。回到顶部才能看见筛选结果的第一条。
 */
watch([levels, runKeyword], async () => {
  stickBottom.value = false;
  atTop.value = true;
  await nextTick();
  runBox.value?.scrollTo({ top: 0 });
});

/*
 * 新行进来时：首次载入停在第一行（第一行在最上方）；之后只在用户已经贴着底部时跟随，
 * 往上翻看历史就不动——否则每条新行都把视线顶走。
 */
watch(
  () => runRows.value.length,
  async (_now, prev) => {
    await nextTick();
    const el = runBox.value;
    if (!el) return;
    if (prev === 0) {
      el.scrollTo({ top: 0 });
      atTop.value = true;
      return;
    }
    if (stickBottom.value) el.scrollTo({ top: el.scrollHeight });
  },
);

/* 切回运行日志时回到第一行：正序视图的默认读法是从上往下，不是从末尾往回看。 */
watch(view, async (v) => {
  if (v !== 'runtime') return;
  stickBottom.value = false;
  await nextTick();
  const el = runBox.value;
  el?.scrollTo({ top: 0 });
});

async function refresh(): Promise<void> {
  if (refreshing.value) return;
  refreshing.value = true;
  try {
    await Promise.all([loadRuns(true), loadDecisions()]);
  } finally {
    refreshing.value = false;
  }
}

async function clearRuns(): Promise<void> {
  confirmingClear.value = false;
  runError.value = '';
  try {
    await clearLogs();
    runRows.value = [];
    cursor = { ...INITIAL_CURSOR };
    justCleared.value = true;
  } catch (e) {
    runError.value = errText(e);
  }
}

function onToggleLevel(lv: LogLevel): void {
  levels.value = toggleLevel(levels.value, lv);
}

function close(): void {
  emit('navigate', 'dashboard');
}

/* Esc 关闭：决策记录是最上面那一层，它不需要查「上层」。 */
function onKeydown(ev: KeyboardEvent): void {
  if (ev.key === 'Escape') close();
}

/* 自动刷新：setTimeout 链，卸载即杀；只有运行日志 + 开关开时才发 IPC。 */
let autoTimer: ReturnType<typeof setTimeout> | null = null;
let killed = false;
function scheduleAuto(): void {
  autoTimer = setTimeout(async () => {
    if (killed) return;
    if (runAuto.value && view.value === 'runtime') await loadRuns(false);
    if (!killed) scheduleAuto();
  }, AUTO_REFRESH_MS);
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  void loadRuns(true);
  void loadDecisions();
  scheduleAuto();
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
  killed = true;
  if (autoTimer !== null) clearTimeout(autoTimer);
});
</script>

<template>
  <!-- 窗体锚点 decisions-panel 已唯一，页容器不再重复挂 page-logs。 -->
  <section class="page page-logs">
    <div
      class="decisions-modal hl-surface is-open pan-modal"
      data-testid="decisions-panel"
      role="dialog"
      aria-modal="true"
      aria-label="决策记录"
    >
      <div class="dl-head">
        <span class="card-title">决策记录</span>
        <span class="card-sub">预筛 / 打分 / 入库 每一次判定的理由，按时间倒序</span>
        <span class="spacer"></span>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          aria-label="关闭决策记录"
          data-testid="decisions-close-btn"
          @click="close"
        >
          ×
        </button>
      </div>

      <div class="dl-tabs">
        <div class="tabs" role="tablist" aria-label="日志视图">
          <button
            v-for="v in LOG_VIEWS"
            :key="v.slug"
            type="button"
            role="tab"
            class="tab"
            :class="{ 'is-active': view === v.slug }"
            :aria-selected="view === v.slug"
            :data-testid="`logs-tab-${v.slug}`"
            @click="view = v.slug"
          >
            {{ v.label }}
          </button>
        </div>
        <span class="spacer"></span>
        <span
          v-if="runAuto && view === 'runtime'"
          class="log-live"
          data-testid="logs-auto-hint"
          title="每 2 秒增量拉取，离开本页自动停止"
        >
          <span class="dot is-running"></span>实时
        </span>
        <button
          type="button"
          class="btn btn-sm"
          data-testid="logs-refresh"
          :disabled="refreshing"
          @click="refresh"
        >
          {{ refreshing ? '刷新中…' : '刷新' }}
        </button>
      </div>

      <!-- 运行日志：筛选条与全部日志行同处一个滚动容器（整块只有一个滚动条），
           筛选条吸顶，滚日志时不会把级别/搜索条件滚丢 -->
      <div v-if="view === 'runtime'" class="log-view dl-content" data-testid="logs-view-runtime">
        <div
          ref="runBox"
          class="run-log-box"
          data-testid="logs-run-feed"
          role="log"
          aria-live="off"
          @scroll.passive="onRunScroll"
        >
          <div class="row wrap log-tools is-sticky">
            <div class="tabs" role="group" aria-label="按级别筛选">
              <button
                v-for="lv in allLevels()"
                :key="lv"
                type="button"
                class="tab"
                :class="{ 'is-active': levels.includes(lv) }"
                :aria-pressed="levels.includes(lv)"
                :data-testid="`logs-level-${lv}`"
                @click="onToggleLevel(lv)"
              >
                {{ levelCn(lv) }}
              </button>
            </div>
            <input
              v-model="runKeyword"
              class="input log-search"
              type="search"
              data-testid="logs-search"
              placeholder="搜索日志内容"
            />
            <span class="spacer"></span>
            <label class="log-auto" title="每 2 秒增量拉取；关闭开关或离开本页即停止">
              <input v-model="runAuto" type="checkbox" data-testid="logs-auto-toggle" />
              <span>自动刷新</span>
            </label>
            <span class="log-count" data-testid="logs-run-count">{{ runCount }}</span>
            <span
              v-if="runFiltered"
              class="log-count log-filter-state"
              data-testid="logs-run-filter-state"
              title="当前生效的筛选条件"
              >筛选 {{ runFilterText }}</span
            >
            <span
              class="log-count log-level-counts"
              data-testid="logs-run-level-counts"
              title="全量分档条数（不受上面级别筛选影响）"
              >{{ runLevelsText }}</span
            >
            <button
              v-if="!stickBottom && !atTop"
              type="button"
              class="btn btn-sm btn-ghost log-jump"
              data-testid="logs-jump-latest-btn"
              @click="jumpLatest"
            >
              回到最新
            </button>
            <button
              v-if="!confirmingClear"
              type="button"
              class="btn btn-sm btn-danger"
              data-testid="logs-clear-btn"
              :disabled="runRows.length === 0"
              @click="confirmingClear = true"
            >
              清空
            </button>
            <span v-else class="log-confirm" data-testid="logs-clear-confirm">
              <span class="log-confirm-text">清空全部运行日志？不可撤销</span>
              <button
                type="button"
                class="btn btn-sm btn-danger"
                data-testid="logs-clear-yes"
                @click="clearRuns"
              >
                确认清空
              </button>
              <button
                type="button"
                class="btn btn-sm btn-ghost"
                data-testid="logs-clear-cancel"
                @click="confirmingClear = false"
              >
                取消
              </button>
            </span>
          </div>

          <div v-if="runError" class="log-inline-error" data-testid="logs-run-error">
            {{ runError }}
          </div>
          <div v-if="runLoading" class="empty" data-testid="logs-run-loading">
            正在加载运行日志…
          </div>
          <div v-else-if="runRows.length === 0" class="empty" data-testid="logs-run-empty">
            {{
              justCleared
                ? '已清空。下一次抓取或操作写入新日志后会出现在这里。'
                : '还没有运行日志。抓取、登录、自动投递等操作都会往这里写一行。'
            }}
          </div>
          <div
            v-else-if="visibleRuns.length === 0"
            class="empty"
            data-testid="logs-run-filtered-empty"
          >
            没有匹配的日志：换个关键字，或取消级别筛选
          </div>
          <template v-else>
            <RunLine v-for="r in visibleRuns" :key="r.key" :row="r" />
          </template>
        </div>
      </div>

      <!-- 决策日志：与运行日志同口径——筛选条与全部行同处一个滚动容器，筛选条吸顶 -->
      <div v-else class="log-view dl-content" data-testid="logs-view-decision">
        <div class="dl-rows" data-testid="decisions-rows" @scroll.passive="onDecisionScroll">
          <div class="row wrap log-tools is-sticky">
            <div class="tabs" role="group" aria-label="按行为筛选">
              <button
                v-for="f in DECISION_FILTERS"
                :key="f.slug"
                type="button"
                class="tab"
                :class="{ 'is-active': decFilter === f.slug }"
                :aria-pressed="decFilter === f.slug"
                :data-testid="`logs-kfilter-${f.slug}`"
                @click="decFilter = f.slug"
              >
                {{ f.label }}
                <span class="tab-num" :data-testid="`logs-kfilter-count-${f.slug}`">{{
                  decFilterTotal(f.slug)
                }}</span>
              </button>
            </div>
            <input
              v-model="decKeyword"
              class="input log-search"
              type="search"
              data-testid="logs-search"
              placeholder="搜索公司 / 岗位 / 原因"
            />
            <span class="spacer"></span>
            <span v-if="decFiltered" class="log-count log-filter-state" data-testid="logs-decision-filter-state"
              >筛选 {{ decFilterText }}</span
            >
            <span class="log-count" data-testid="logs-decision-count">{{ decCount }}</span>
          </div>

          <div v-if="decError" class="log-inline-error" data-testid="logs-decision-error">
            {{ decError }}
          </div>
          <div v-if="decLoading" class="empty" data-testid="logs-decision-loading">
            正在加载决策记录…
          </div>
          <div v-else-if="decisions.length === 0" class="empty" data-testid="logs-decision-empty">
            还没有决策记录。每一条被排除或入库的岗位、以及每次全量重算改动的分数，都会记在这里。
          </div>
          <div
            v-else-if="visibleDecisions.length === 0"
            class="empty"
            data-testid="logs-decision-filtered-empty"
          >
            没有匹配的决策记录：换个行为筛选或关键字
          </div>
          <template v-else>
            <DecisionRow v-for="r in visibleDecisions" :key="r.id" :row="r" />
          </template>
          <div v-if="decLoadingMore" class="dl-more" data-testid="decisions-loading-more">
            正在加载更早的决策…
          </div>
          <div v-else-if="decDone && decisions.length > 0" class="dl-more" data-testid="decisions-end">
            没有更早的记录了
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
