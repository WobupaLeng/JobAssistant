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
import JobDetailDrawer from '../components/JobDetailDrawer.vue';
import JobListView from '../components/JobListView.vue';
import PlatformCard from '../components/PlatformCard.vue';
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
import { scrapeDrain, scrapeProgress, scrapeStart, scrapeStop } from '../api/commands/scrape';
import { getAppConfig, setLimit, setScrapeDefaults } from '../api/commands/config';
import { sensitiveList, sensitiveRemove } from '../api/commands/sensitive';
import { loginStart, loginStatus } from '../api/commands/scrape';
import { listExpectations } from '../api/commands/chat';
import type {
  AppConfigReply,
  Job,
  JobTabCounts,
  LoginStatus,
  Expectation,
  SensitivePage,
} from '../api/types';
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
  MAX_JD_CEILING,
  buttonState,
  fromProgress,
  toFeedLine,
  trimFeed,
  type FeedLine,
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
  KW_REQUIRED_NOTE,
  searchKeywords,
  type FilterDraft,
} from '../lib_ui/scrape-filter-model';
import { loadDraft, saveDraft } from '../features/scrape/filter';
import { pageTestId } from '../lib_ui/nav';
import { parseLogLevel } from '../lib_ui/logfmt';
import { readInitialTheme, flipTheme, applyTheme } from '../lib_ui/theme';
import {
  EMPTY_DETAIL,
  actionFailPrefix,
  actionNote,
  closeDetail,
  detailTitle,
  openDetail,
  type DetailAction,
  type DetailState,
} from '../lib_ui/job-detail-model';
import { PRIMARY_PLATFORM, pickPlatform } from '../lib_ui/platform-model';
import {
  EMPTY_QUEUE,
  clearErrors,
  dismissError,
  pushError,
  type ErrorItem,
  type ErrorQueue,
} from '../lib_ui/error-queue';

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
/** 搜索关键词：进 `scrape_start.keywords`，**不进** `set_scrape_defaults`。 */
const keywords = ref<string[]>([]);
const expects = shallowRef<readonly Expectation[]>([]);

/** 抓取事件流（`scrape_drain` 增量），入库行首 `✔`（C9）。 */
const feed = shallowRef<FeedLine[]>([]);

const logLines = shallowRef<LogLine[]>([]);
const logSince = ref<number>(0);

const sensitiveOpen = ref(false);
const sensitivePage = shallowRef<SensitivePage>({ items: [], total: 0 });
const sensitiveLoading = ref(false);
const sensitiveError = ref('');

// 错误是**队列**：并发失败时一条都不丢（见 lib_ui/error-queue.ts）。
const errors = ref<ErrorQueue>(EMPTY_QUEUE);

// 岗位详情抽屉。
const detail = ref<DetailState>(EMPTY_DETAIL);
const detailJob = computed<Job | null>(() => {
  const id = detail.value.jobId;
  if (id === null) return null;
  return jobs.value.find((j) => j.job_id === id) ?? null;
});

// 平台卡：本轮只有 Boss 接了登录链路，选中的平台随抓取一起发给 scrape_start。
const platform = ref(PRIMARY_PLATFORM);
const login = shallowRef<LoginStatus | null>(null);
const loginBusy = ref(false);

const themeLabel = ref(readInitialTheme() === 'dark' ? '深色' : '浅色');

let tickTimer: ReturnType<typeof setInterval> | null = null;
let logTimer: ReturnType<typeof setTimeout> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;
let pressTimer: TimerHandle | null = null;
let pressStart: PointerSample | null = null;
let pressJobId = '';

const segs = computed(() => scoreModeSegs('rule'));
const state = computed(() => buttonState(progress.value.running, pausedByUser.value));

// ---------------------------------------------------------------- 错误

/**
 * 记一条错误进队列。`retry` 给出时，错误条上多一个「重试」按钮。
 * 队列上限与去重都在 `pushError` 里（纯函数），这里不做判断。
 */
function fail(message: string, code: string, retry: { label: string; run: () => void } | null = null): void {
  errors.value = pushError(errors.value, message, code, Date.now(), retry?.label ?? null);
  if (retry) retryActions.set(errors.value.items[errors.value.items.length - 1]?.id ?? -1, retry.run);
}

/** 重试动作按条挂：关掉一条不影响其它条的重试。 */
const retryActions = new Map<number, () => void>();

function onRetry(item: ErrorItem): void {
  const run = retryActions.get(item.id);
  errors.value = dismissError(errors.value, item.id);
  retryActions.delete(item.id);
  run?.();
}

function onDismissError(id: number): void {
  errors.value = dismissError(errors.value, id);
  retryActions.delete(id);
}

function onDismissAllErrors(): void {
  errors.value = clearErrors(errors.value);
  retryActions.clear();
}

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
    fail(`岗位列表加载失败：${listError.value}`, 'list_jobs', { label: '重试', run: () => void refreshList() });
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
  // 运行中才拉事件流；停了立刻不再拉（Leptos 的 drain 定时器同口径）。
  if (progress.value.running) await drainFeed();
}

async function loadLogs(): Promise<void> {
  try {
    const reply = await listLogs(logSince.value);
    // C14 / 日志游标：`LogRing::since` 是闭区间，边界那一毫秒会重送一条，
    // 按 `(ts, level, message)` 多重集去重（不是按 ts 丢）。
    const incoming = reply.items
      .map((r) => ({ ts: r.ts, seq: r.seq, level: parseLogLevel(r.level), message: r.message }))
      .filter((r) => r.ts >= logSince.value);
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

/**
 * 事件流：`scrape_drain(50)` 取增量，转成行追加（`panel.rs` 的 drain 同口径）。
 *
 * 只在**运行中**拉：空闲时拉是白烧 CPU（Leptos 注释原话）。失败静默——
 * 事件流是辅助信息，不该盖住真正的错误。
 */
async function drainFeed(): Promise<void> {
  try {
    const updates = await scrapeDrain(50);
    if (updates.length === 0) return;
    const lines: FeedLine[] = [];
    for (const u of updates) {
      const line = toFeedLine(u);
      if (line) lines.push(line);
    }
    if (lines.length > 0) feed.value = trimFeed([...feed.value, ...lines]);
  } catch {
    /* 同上 */
  }
}

async function loadSpecAndConfig(): Promise<void> {
  try {
    const spec = await getScoringSpec();
    scoreMax.value = spec.score_max;
    minScore.value = spec.score_min_default;
  } catch (e) {
    fail(`计分规格读取失败：${errText(e)}`, getScoringSpec.name, { label: '重试', run: () => void loadSpecAndConfig() });
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
    fail(`配置读取失败：${errText(e)}`, 'get_app_config', { label: '重试', run: () => void loadSpecAndConfig() });
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
    fail(`筛选条件保存失败：${errText(e)}`, 'set_scrape_defaults', { label: '重试', run: () => void persistDraft() });
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
  void loadExpects();
}

/**
 * 读求职期望（`list_expectations`）。失败进错误队列但**不挡操作**：
 * 期望只是关键词的推荐来源，读不到不影响手填。
 */
async function loadExpects(): Promise<void> {
  try {
    const list = await listExpectations();
    expects.value = list;
    filterNote.value = `已读取 ${list.length} 条求职期望`;
  } catch (e) {
    fail(`读取期望职位失败：${errText(e)}`, 'list_expectations', { label: '重试', run: () => void loadExpects() });
  }
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
    fail(`批量操作失败：${errText(e)}`, op === 'reject' ? 'reject_jobs' : 'allow_jobs');
  }
}

// ---------------------------------------------------------------- 详情抽屉

/** 卡片单击（非多选态）= 打开详情。长按多选时 click 由 onGridClick 吃掉。 */
function onOpenDetail(jobId: string): void {
  detail.value = openDetail(detail.value, jobId);
}

function onCloseDetail(): void {
  detail.value = closeDetail();
}

/**
 * 抽屉底部三个单条动作。
 *
 * 与批量条共用同一批命令（reject_jobs / allow_jobs / mark_delivered，载荷都是
 * `{args:{jobIds}}`），差别只在「影响几条」的文案。动作成功后关抽屉并刷新，
 * 失败进错误队列——**不让抽屉停在半更新状态**。
 */
async function onDetailAction(action: DetailAction): Promise<void> {
  const job = detailJob.value;
  if (!job) return;
  const ids = [job.job_id];
  const code = action === 'reject' ? 'reject_jobs' : action === 'allow' ? 'allow_jobs' : 'mark_delivered';
  try {
    const hit =
      action === 'reject'
        ? await rejectJobs(ids)
        : action === 'allow'
          ? await allowJobs(ids)
          : (await markDelivered(ids), ids.length);
    filterNote.value = actionNote(action, hit);
    detail.value = closeDetail();
    await refreshList();
  } catch (e) {
    fail(`${actionFailPrefix(action)}${errText(e)}`, code, {
      label: '重试',
      run: () => void onDetailAction(action),
    });
  }
}

// ---------------------------------------------------------------- 平台

function onPickPlatform(id: string): void {
  platform.value = pickPlatform(platform.value, id);
}

/** 读登录态。失败静默：状态点只是提示，不值得开一条错误。 */
async function refreshLogin(): Promise<void> {
  try {
    login.value = await loginStatus(platform.value);
  } catch {
    login.value = null;
  }
}

async function onLogin(id: string): Promise<void> {
  if (loginBusy.value) return;
  platform.value = pickPlatform(platform.value, id);
  loginBusy.value = true;
  try {
    await loginStart(platform.value);
    filterNote.value = '已打开登录窗口，请在弹出的浏览器里扫码';
    await refreshLogin();
  } catch (e) {
    fail(`启动登录失败：${errText(e)}`, 'login_start', { label: '重试', run: () => void onLogin(id) });
  } finally {
    loginBusy.value = false;
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
  // 多选态：单击 = 勾选切换；非多选态：单击 = 打开详情（长按才进多选）。
  // 打开详情前先确认不是点在按钮/链接上（`shouldOpenCard` 同口径，见 JobCard）。
  if (selection.value.selecting) {
    selection.value = clickCard(selection.value, id);
    return;
  }
  const el = e.target as HTMLElement;
  if (el.closest('button, a, input, label, select, textarea, [data-greet]')) return;
  onOpenDetail(id);
}

// ---------------------------------------------------------------- 抓取

async function onStart(): Promise<void> {
  if (startDisabled(starting.value, progress.value.running)) return;
  // 搜索词只认 `keywords`。为空就拦下来，交给后端回落默认词表会
  // 让用户以为"我填的排除词生效了"——其实一个搜索词都没发出去。
  const kws = searchKeywords(keywords.value);
  if (kws.length === 0) {
    filterNote.value = KW_REQUIRED_NOTE;
    filterOpen.value = true;
    return;
  }
  starting.value = true;
  try {
    await scrapeStart({
      platform: platform.value,
      keywords: kws,
      maxJd: maxJd.value,
      minScore: minScore.value ?? 0,
    });
    pausedByUser.value = false;
    await refreshProgress();
    filterNote.value = `已启动抓取 · 关键词 ${kws.join(' / ')} · 门槛 ${minScore.value ?? 0} 分`;
  } catch (e) {
    fail(`启动抓取失败：${errText(e)}`, 'scrape_start', { label: '重试', run: () => void onStart() });
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
    fail(`停止抓取失败：${errText(e)}`, 'scrape_stop');
  }
}

/**
 * 两处限额输入框的保存动作（Leptos `on_max_jd` / `on_min_score` 同口径）。
 *
 * 只走 `set_limit`——`min_score` 与 `max_jd` 都不在 `set_app_config` 的
 * `WRITABLE_KEYS` 里，塞进 patch 会被**静默跳过**（C11）。
 */
async function onMaxJd(v: number): Promise<void> {
  const next = Math.min(MAX_JD_CEILING, Math.max(1, Math.trunc(v)));
  try {
    const reply = await setLimit(next, null);
    maxJd.value = reply.max_jd;
    filterNote.value = `有效 JD 上限已设为 ${reply.max_jd} 条`;
  } catch (e) {
    fail(`保存 JD 上限失败：${errText(e)}`, 'set_limit', { label: '重试', run: () => void onMaxJd(next) });
  }
}

async function onMinScore(v: number): Promise<void> {
  const max = scoreMax.value;
  if (max === null) {
    // 满分还没下发时算不出合法区间，宁可不写也不猜一个上限。
    fail('打分口径还没加载完，稍后再试', 'get_scoring_spec', { label: '重试', run: () => void loadSpecAndConfig() });
    return;
  }
  const next = Math.min(max, Math.max(0, Math.trunc(v)));
  try {
    const reply = await setLimit(null, next);
    minScore.value = reply.min_score;
    filterNote.value = `门槛已设为 ${reply.min_score} 分`;
  } catch (e) {
    fail(`保存门槛失败：${errText(e)}`, 'set_limit', { label: '重试', run: () => void onMinScore(next) });
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
  void refreshLogin();
  void loadExpects();
  // C16：setInterval 在 WebView 挂起时同样不触发；恢复后下一次立即生效。
  // 同一个 tick 兼管进度与事件流：运行中才真的发命令（见 refreshProgress）。
  tickTimer = setInterval(() => {
    void refreshCounts();
    void refreshProgress();
  }, TICK_MS);
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
      :queue="errors"
      @dismiss="onDismissError"
      @dismiss-all="onDismissAllErrors"
      @retry="onRetry"
    />

    <PlatformCard
      :selected="platform"
      :login="login"
      :login-busy="loginBusy"
      @pick="onPickPlatform"
      @login="onLogin"
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
      @update:max-jd="onMaxJd"
      @update:min-score="onMinScore"
    />

    <div v-if="filterOpen" class="card scrape-filter-bar" data-testid="scrape-filter-bar">
      <FilterPanel
        :draft="draft"
        :keywords="keywords"
        :expects="expects"
        :saving="saving"
        :note="filterNote"
        @update:draft="onDraft"
        @update:keywords="keywords = $event"
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
        @empty-filter="filterOpen = true"
        @empty-start="onStart"
      />
    </div>

    <LogCard
      :progress="progress"
      :lines="logLines"
      :feed="feed"
      :score-max="scoreMax"
      :min-score="minScore"
      :live="true"
      @open-decisions="emit('navigate', 'logs')"
    />

    <JobDetailDrawer
      :job="detailJob"
      :min-score="minScore"
      :score-max="scoreMax"
      @close="onCloseDetail"
      @act="onDetailAction"
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
