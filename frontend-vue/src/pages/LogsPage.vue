<script setup lang="ts">
/**
 * M4 决策记录页（ARCH-003 阶段 3）。固定窗 .decisions-modal，双视图：
 * 决策日志（list_decisions → .dl-row 四列网格）/ 运行日志（list_logs seq 游标增量）。
 * 可独立 mount（无必需 props）；App.vue 接线由总控汇合时做。
 *
 * 运行日志增量走 seq 游标（features/logs/model）：since 用回执 now 保证不漏，
 * afterSeq 过滤闭区间重送保证不重（根 AGENTS.md §7.1，同毫秒多条不会永久丢失）。
 * 自动刷新每 2 秒一次，仅在「运行日志 + 开关开」时发 IPC；setTimeout 链在
 * WebView 挂起时自然停摆（C16），不做 Date.now 追赶；离开本页即杀。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { clearLogs, listLogs } from '../api/commands/logs';
import { listDecisions } from '../api/commands/db';
import { allLevels, levelCn, type LogLevel } from '../lib_ui/logfmt';
import {
  AUTO_REFRESH_MS,
  DECISION_FILTERS,
  DECISION_LIMIT,
  INITIAL_CURSOR,
  LOG_VIEWS,
  advanceCursor,
  decisionCountLabel,
  decisionRowFromRecord,
  filterDecisions,
  filterRuns,
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
const decCount = computed(() =>
  decisionCountLabel(visibleDecisions.value.length, decisions.value.length, decFiltered.value),
);

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

async function loadDecisions(): Promise<void> {
  decLoading.value = true;
  decError.value = '';
  try {
    const rows = await listDecisions(DECISION_LIMIT, null);
    decisions.value = rows.map(decisionRowFromRecord);
  } catch (e) {
    decError.value = errText(e);
  } finally {
    decLoading.value = false;
  }
}

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

/* Esc 关闭（SPEC §6.1 / A10）：决策记录是最上面那一层，它不需要查「上层」。 */
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
  <!-- 窗体锚点 decisions-panel 已唯一，页容器不再重复挂 page-logs（SPEC-001 AC9）。 -->
  <section class="page page-logs">
    <div
      class="decisions-modal is-open"
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

      <!-- 运行日志 -->
      <div v-if="view === 'runtime'" class="log-view dl-content" data-testid="logs-view-runtime">
        <div class="row wrap log-tools">
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

        <div v-if="runError" class="log-inline-error" data-testid="logs-run-error">{{ runError }}</div>
        <div v-if="runLoading" class="empty" data-testid="logs-run-loading">正在加载运行日志…</div>
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
        <div v-else class="run-log-box" data-testid="logs-run-feed" role="log" aria-live="off">
          <RunLine v-for="r in visibleRuns" :key="r.key" :row="r" />
        </div>
      </div>

      <!-- 决策日志 -->
      <div v-else class="log-view dl-content" data-testid="logs-view-decision">
        <div class="row wrap log-tools">
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
          <span class="log-count" data-testid="logs-decision-count">{{ decCount }}</span>
        </div>

        <div v-if="decError" class="log-inline-error" data-testid="logs-decision-error">
          {{ decError }}
        </div>
        <div v-if="decLoading" class="empty" data-testid="logs-decision-loading">正在加载决策记录…</div>
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
        <div v-else class="dl-rows" data-testid="decisions-rows">
          <DecisionRow v-for="r in visibleDecisions" :key="r.id" :row="r" />
        </div>
      </div>
    </div>
  </section>
</template>
