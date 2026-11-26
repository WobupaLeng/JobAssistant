<script setup lang="ts">
/**
 * 概览页：抓取条 + 筛选面板 + 岗位列表 + 最近事件。本页只做编排——进页拉一次 `list_jobs` /
 * `job_tab_counts` / `scrape_progress` / `get_scoring_spec` / `get_app_config`，之后
 * `job_tab_counts` 每 5 秒轮询（**只为页签数字**）。写：筛选只发 diff，批量标记走 `set_job_flag`，
 * 抓取走 `scrape_start` / `scrape_stop`。 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
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
  unmarkDelivered,
} from '../api/commands/db';
import { listLogs } from '../api/commands/logs';
import { getScoringSpec } from '../api/commands/shell';
import { scrapeDrain, scrapeProgress, scrapeStart, scrapeStop } from '../api/commands/scrape';
import { getAppConfig, setAppConfig, setLimit, setScrapeDefaults } from '../api/commands/config';
import { sensitiveList, sensitiveRemove } from '../api/commands/sensitive';
import { loginStart, loginStatus } from '../api/commands/scrape';
import { loginAck, logout } from '../api/commands/scrape';
import { listenLoginState, type UnlistenFn } from '../api/events';
import { listExpectations } from '../api/commands/chat';
import { setWindowTheme } from '../api/commands/window';
import type {
  AppConfigReply,
  Job,
  JobTabCounts,
  LoginStatus,
  Expectation,
  SensitivePage,
} from '../api/types';
import { setGreetDraft } from '../features/chat/greet-draft';
import { tokenSpentTick } from '../lib_ui/token-ledger';
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
  type BatchOp,
  type ListTab,
  type SortField,
} from '../lib_ui/job-list-model';
import {
  clearLivePatches,
  livePatches,
  pushFreshJobs,
  resetCountDelta,
} from '../lib_ui/job-live';
import {
  clickBlank,
  clickCard,
  enterSelecting,
  setChecked,
  type SelectionState,
} from '../lib_ui/longpress';
import {
  LLM_PREFILTER_DEFAULT,
  MAX_JD_CEILING,
  buttonState,
  clampPrefilter,
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
import { loadDraft, loadKeywords, saveDraft, saveKeywords } from '../features/scrape/filter';
import { parseLogLevel } from '../lib_ui/logfmt';
import { readInitialTheme, applyTheme } from '../lib_ui/theme';
import {
  FONT_SCALES,
  PALETTES,
  applyFontScale,
  applyPalette,
  readFontScale,
  readPalette,
  type FontScaleId,
  type PaletteId,
} from '../lib_ui/palette';
import {
  EMPTY_DETAIL,
  actionFailPrefix,
  actionNote,
  closeDetail,
  detailTitle,
  openDetail,
  type DetailAct,
  type DetailState,
} from '../lib_ui/job-detail-model';
import { PRIMARY_PLATFORM, pickPlatform } from '../lib_ui/platform-model';
import { PLATFORMS } from '../lib_ui/options';
import { mergeLoginState } from '../lib_ui/login-model';
import { CHAT_ACTIVE_COMPANY_EVENT, CHAT_ACTIVE_COMPANY_KEY } from '../lib_ui/chat-model';
import { closePanel, PAN_CLASS, playOpen, resetPanel } from '../lib_ui/pan';
import {
  EMPTY_QUEUE,
  clearErrors,
  dismissError,
  pushError,
  type ErrorItem,
  type ErrorQueue,
} from '../lib_ui/error-queue';

/** 跳页交给父级：页面直接 import 路由会形成回环，父级监听 `navigate` 再切换。 */
const props = defineProps<{ activeNav?: string }>();
const emit = defineEmits<{
  /** 浮层窗开关。只有 `config` / `logs` 两种：**聊天不是页**，常驻右栏。 */
  (e: 'navigate', nav: 'config' | 'logs'): void;
  /** 卡片「打招呼」⇒ 请父级分发槽就地展开对话框并消费草稿。 */
  (e: 'greet'): void;
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
/** 筛选抽屉的节点：展开类挂在它身上，收起时用它等回程跑完再摘（与另两个抽屉同一套）。 */
const filterBar = ref<HTMLElement | null>(null);

/** 打开筛选抽屉：等节点挂上再补一帧挂展开类，关键帧才从起点播。 */
function openFilter(): void {
  if (filterOpen.value) return;
  filterOpen.value = true;
  void nextTick(() => playOpen(filterBar.value, PAN_CLASS.DRAWER));
}

/**
 * 关闭筛选抽屉：**当帧**先摘掉展开态让 `--drawer-dur` 的位移回程真的播出来，过渡结束再摘
 * 节点。反过来（先挂 `.is-closing` 播回程、过渡完才卸载）会让点击到卸载之间出现「点了没反应」
 * 的空窗，因为 transform 推回起点的那一帧眼睛看着是不动的。
 */
function closeFilter(): void {
  if (!filterOpen.value) return;
  filterOpen.value = false;
  const node = filterBar.value;
  void closePanel(node).then(() => {
    resetPanel(node);
    resetPanel(filterBar.value);
  });
}

const scoreMax = ref<number | null>(null);
const minScore = ref<number | null>(null);
const maxJd = ref(0);
const config = shallowRef<AppConfigReply | null>(null);
const tokensTotal = computed(() => config.value?.tokens_total ?? 0);
/* 分项读数：只读既有 config 字段，零新增 invoke 形状。累加由后端 ADR-005 的 `bump_tokens`
 * 落库，前端只负责重读（见下方 `refreshTokens`）。 */
const tokensGreet = computed(() => config.value?.tokens_greet ?? 0);
const tokensScore = computed(() => config.value?.tokens_score ?? 0);

const draft = ref<FilterDraft>(createDraft());
const savedKnown = ref<Record<string, unknown>>({});
const saving = ref(false);
const filterNote = ref('');
/**
 * 搜索关键词：进 `scrape_start.keywords`，**不进** `set_scrape_defaults`。
 * 后端不存这份，只能落本地，否则 F5 一次就清空。
 */
const keywords = ref<string[]>(loadKeywords());
const expects = shallowRef<readonly Expectation[]>([]);

/** 抓取事件流（`scrape_drain` 增量），入库行首 `✔`（C9）。 */
const feed = shallowRef<FeedLine[]>([]);
/** 事件行的全局递增序号：混排后给每条一个稳定 key。 */
let feedN = 0;

const logLines = shallowRef<LogLine[]>([]);
const logSince = ref<number>(0);
/** 首屏历史日志是否已补过一次（只补一次，避免每次挂载都重读整环）。 */
const logPrimed = ref(false);

const sensitiveOpen = ref(false);
/**
 * 敏感库是否挂载：打开即挂，**关掉就立刻摘**。面板 teleport 在 `.app-shell` 之后、
 * `z-index: 61`，常驻 DOM 会让一块关掉的透明面板压住顶栏主题菜单 / 城市选择器；它自己排的
 * 收起回调又是个自由计时器，晚到一步就是「关了又弹回来」。
 */
const sensitiveMounted = ref(false);
watch(sensitiveOpen, (open) => {
  if (open) sensitiveMounted.value = true;
  else sensitiveMounted.value = false;
});
const sensitivePage = shallowRef<SensitivePage>({ items: [], total: 0 });
const sensitiveLoading = ref(false);
const sensitiveError = ref('');

// 错误是**队列**：并发失败时一条都不丢（见 lib_ui/error-queue.ts）。
const errors = ref<ErrorQueue>(EMPTY_QUEUE);

// 岗位详情抽屉：打开的那一条按 job_id 单独缓存，不跟着列表分页窗口走——翻页 / 排序 / 过滤
// 之后 `jobs` 里已经找不到它，直接查列表会得到「点了卡片什么都不出现」。
const detail = ref<DetailState>(EMPTY_DETAIL);
const detailJob = ref<Job | null>(null);

// 平台卡：本轮只有 Boss 接了登录链路，选中的平台随抓取一起发给 scrape_start。
const platform = ref(PRIMARY_PLATFORM);
const login = shallowRef<LoginStatus | null>(null);
const loginBusy = ref(false);
/** 全窗浮层的挂载根（`index.html` 里那个空 div）；独立挂载页面时为空，就地渲染。 */
const overlayRoot = ref<HTMLElement | null>(null);

/** 最近一次「退出登录」还没回来（防连点）。 */
const loginActionBusy = ref(false);
/** `login-state` 的退订函数；未订阅/已退订时为 `null`。 */
let unlistenLogin: UnlistenFn | null = null;
/** 组件已卸载：挡住「订阅还没 await 回来就卸载」的悬挂监听。 */
let disposed = false;

const themeLabel = ref(readInitialTheme() === 'dark' ? '深色' : '浅色');
const paletteId = ref<PaletteId>(readPalette());
/** 当前主题（深/浅），供调色板按主题取色。 */
function currentTheme(): 'dark' | 'light' {
  return themeLabel.value === '深色' ? 'dark' : 'light';
}
applyPalette(paletteId.value, currentTheme());
const fontScaleId = ref<FontScaleId>(readFontScale());
applyFontScale(fontScaleId.value);

/**
 * 主题下拉面板的开合（主题 / 主题色 / 字号收成一枚按钮）。开关住在 `TopbarTail.vue`，
 * 这里只做转发：面板点外部关闭时它会 emit 一次 false，页面状态跟着回写。
 */
const themePanelOpen = ref(false);
function onToggleThemePanelHandler(next: boolean): void {
  themePanelOpen.value = next;
}
/**
 * 面板里点「深色 / 浅色」：**直接用点中的那一档**，不能再反一次（按钮语义是「选哪个就是
 * 哪个」）。`flipTheme` 只服务于需要取反的场合，本页已无调用方。
 */
function onPickThemeMode(mode: 'dark' | 'light'): void {
  applyTheme(mode);
  themeLabel.value = mode === 'dark' ? '深色' : '浅色';
  applyPalette(paletteId.value, mode);
  // 窗口模糊底色分深浅两档：深色档是深蓝黑，浅色必须换成近白，否则整窗发灰。
  // 非 Tauri 环境（单测 / 浏览器预览）里这条命令必然失败：纯外观增强，吞掉即可。
  void setWindowTheme(mode).catch(() => undefined);
}

let tickTimer: ReturnType<typeof setInterval> | null = null;
let logTimer: ReturnType<typeof setTimeout> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | null = null;
/** 上一次看到的「已入库条数」；`null` = 还没基线（首帧只记基线，不补行）。 */
let insertedSeen: number | null = null;

/**
 * 打分方式与初筛线都落 `app_config`（`score_mode` / `llm_prefilter`，两者都在
 * 后端 `WRITABLE_KEYS` 里，走 `set_app_config` 即可，不走 `set_limit`）。
 */
const scoreMode = ref<'rule' | 'llm'>('rule');
const llmPrefilter = ref(LLM_PREFILTER_DEFAULT);
const segs = computed(() => scoreModeSegs(scoreMode.value));

/** 切打分方式：先乐观切，写库失败再回滚（否则界面与配置两张皮）。 */
async function onPickMode(key: 'rule' | 'llm'): Promise<void> {
  if (scoreMode.value === key) return;
  const prev = scoreMode.value;
  scoreMode.value = key;
  try {
    await setAppConfig({ score_mode: key });
  } catch (e) {
    scoreMode.value = prev;
    fail(`打分方式保存失败：${errText(e)}`, setAppConfig.name);
  }
}

/** 初筛线：夹到 0-100 再写库；失败回滚，避免显示值与库值不一致。 */
async function onPrefilter(v: number): Promise<void> {
  const next = clampPrefilter(v);
  if (next === llmPrefilter.value) return;
  const prev = llmPrefilter.value;
  llmPrefilter.value = next;
  try {
    await setAppConfig({ llm_prefilter: next });
  } catch (e) {
    llmPrefilter.value = prev;
    fail(`初筛分数线保存失败：${errText(e)}`, setAppConfig.name);
  }
}
const state = computed(() => buttonState(progress.value.running, pausedByUser.value));

// ---------------------------------------------------------------- 错误

/**
 * 记一条错误进队列。`retry` 给出时，错误条上多一个「重试」按钮。
 * 队列上限与去重都在 `pushError` 里（纯函数），这里不做判断。
 */
function fail(message: string, code: string, retry: { label: string; run: () => void } | null = null): void {
  // 同一来源（同 code）只保留一条：先清掉旧的同源卡片，再入队，避免错误堆叠。
  const prior = errors.value.items.filter((it) => it.code === code);
  for (const it of prior) retryActions.delete(it.id);
  if (prior.length > 0) errors.value = prior.reduce((q, it) => dismissError(q, it.id), errors.value);
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

/** 每页条数；后端 `list_jobs` 按 `tab` 过滤后分页（`offset`）。 */
const PAGE_SIZE = 50;
const hasMore = ref(true);
const pageLoading = ref(false);

/**
 * 列表实际渲染的行 = 服务端这一页 + 抓取中就地补的几条 − 刚自动投出去的那几条。
 * 分页 `offset` 仍以 `jobs` 为准，补进来的行不参与，避免翻页错位。
 */
const shownJobs = computed<Job[]>(() => {
  const { sentIds, freshJobs } = livePatches();
  const hidden = new Set(sentIds);
  const rows = jobs.value.filter((j) => !hidden.has(j.job_id));
  if (freshJobs.length === 0) return rows;
  const have = new Set(rows.map((j) => j.job_id));
  const extra = freshJobs.filter((j) => !hidden.has(j.job_id) && !have.has(j.job_id));
  return extra.length === 0 ? rows : [...extra, ...rows];
});

/** 页签数字带同一份补丁；后端一给权威计数，补丁就归零（见 `resetCountDelta`）。 */
const shownCounts = computed<JobTabCounts>(() => {
  const { pendingDelta, deliveredDelta } = livePatches();
  if (pendingDelta === 0 && deliveredDelta === 0) return counts.value;
  const c = counts.value;
  return {
    pending: Math.max(0, c.pending + pendingDelta),
    delivered: Math.max(0, c.delivered + deliveredDelta),
    rejected: c.rejected,
  };
});

function listArgs(offset: number) {
  return {
    include_rejected: showAll.value || tabIncludeRejected(tab.value),
    rejected_only: false,
    ascending: sortAscending(sortDescending.value),
    limit: PAGE_SIZE,
    offset,
    sort: sortField.value,
    tab: tabQuery(tab.value, showAll.value),
    order: sortOrder(sortDescending.value),
  };
}

/** 滚到底部时追加下一页；不足一页即视为已拉完。 */
async function loadMore(): Promise<void> {
  if (!hasMore.value || pageLoading.value || listLoading.value) return;
  pageLoading.value = true;
  try {
        const items = await listJobs(listArgs(jobs.value.length));
        jobs.value = [...jobs.value, ...items];
        hasMore.value = items.length === PAGE_SIZE;
  } catch (e) {
    fail(`加载更多失败：${errText(e)}`, 'list_jobs', { label: '重试', run: () => void loadMore() });
  } finally {
    pageLoading.value = false;
  }
}

async function refreshList(): Promise<void> {
  listLoading.value = true;
  listError.value = '';
  try {
    const [items, c] = await Promise.all([listJobs(listArgs(0)), jobTabCounts()]);
    jobs.value = items;
    hasMore.value = items.length === PAGE_SIZE;
    counts.value = c;
    clearLivePatches();
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
    resetCountDelta();
  } catch {
    /* 轮询失败不打断用户：下一轮自会重试 */
  }
}

/**
 * 抓取刚入库几条：拉最新一页里对应的行，先补进列表（`freshJobs`），不等整份重拉。
 * 只补「未投递」口径——其它页签不该出现刚抓来的岗位；失败就等用户主动刷新。
 */
async function pullFreshJobs(count: number): Promise<void> {
  if (count <= 0) return;
  if (!showAll.value && tab.value !== 'pending') return;
  try {
    const rows = await listJobs({
      include_rejected: showAll.value,
      rejected_only: false,
      ascending: false,
      limit: Math.min(count, PAGE_SIZE),
      offset: 0,
      sort: 'created',
      tab: showAll.value ? undefined : 'pending',
      order: 'desc',
    });
    pushFreshJobs(rows);
  } catch {
    /* 增量显示是锦上添花：失败等下一次进度或手动刷新 */
  }
}

async function refreshProgress(): Promise<void> {
  try {
    progress.value = fromProgress(await scrapeProgress());
  } catch {
    /* 同上：进度是辅助信息 */
  }
  const inserted = progress.value.inserted;
  if (insertedSeen === null) {
    // 首帧只记基线：挂载时库里已有多少条不该被当成「刚抓到」。
    insertedSeen = inserted;
  } else if (inserted > insertedSeen) {
    const delta = inserted - insertedSeen;
    insertedSeen = inserted;
    void pullFreshJobs(delta);
  } else if (inserted < insertedSeen) {
    // 新一轮抓取从 0 起算。
    insertedSeen = inserted;
  }
  // 运行中才拉事件流；停了立刻不再拉（与 drain 定时器同口径）。
  if (progress.value.running) await drainFeed();
  // 进度是「要不要轮询」的唯一判据，这里统一收口：抓取一开就挂上 tick，
  // 一停就卸掉。调用方不必各自记得补一句 syncTick（挂载、启动、停止、
  // 可见性恢复都会经过这里）。
  syncTick();
}

/**
 * 读日志。失败即**停止轮询**（见 `pollLogs`），不静默重试、不新增错误卡片——
 * 日志是诊断通道，它自己失败时不能再制造错误堆叠。
 */
async function loadLogs(): Promise<boolean> {
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
    return true;
  } catch {
    /* 日志失败不提示：它本身就是诊断通道 */
    return false;
  }
}

/**
 * 日志卡首屏补一次历史日志。日志卡原先只在「已登录 + 抓取运行中」才轮询，空闲态永远是空框。
 * 这里只读 Rust 内存环（`LogRing`，不碰 sidecar），补完交给 `startLogPolling` 的闸门追新增。
 * 失败不重试（`loadLogs` 自己吞错）。
 */
async function primeLogs(): Promise<void> {
  if (logPrimed.value || !isLoggedIn()) return;
  logPrimed.value = true;
  await loadLogs();
}

// 登录态一到位就补一次历史日志：`refreshLogin` / `login-state` 都不经过 `syncTick`，
// 只挂在 `syncTick` 里会漏掉「空闲态下登录态才回来」的常见路径。
watch(
  () => login.value?.state,
  (state) => {
    // 只补首屏快照；轮询闸门由 `syncTick` 按「已登录 + 运行中」自己开关。
    if (state === 'logged_in') void primeLogs();
    else stopLogPolling();
  },
  { immediate: true },
);

/**
 * 日志轮询的唯一闸门：**已登录 + 抓取运行中**才允许继续轮询。
 * 空闲态一条定时 IPC 都不发（首屏那一次快照由 `primeLogs` 补，不算轮询）；
 * 读失败（sidecar 不可用 / 管道关闭）即停，不循环重试。
 */
function canPollLogs(): boolean {
  return isLoggedIn() && progress.value.running;
}

let logPollStopped = false;

function startLogPolling(): void {
  if (logTimer !== null || disposed || logPollStopped || !canPollLogs()) return;
  logTimer = setTimeout(async () => {
    logTimer = null;
    // 定时器触发时再查一次闸门：停止后即便有残留回调也不发请求。
    if (disposed || logPollStopped || !canPollLogs()) return;
    const ok = await loadLogs();
    if (!ok) {
      // 失败：立即关闭闸门并清掉任何已排队的定时器，不再循环。
      logPollStopped = true;
      stopLogPolling();
      return;
    }
    startLogPolling();
  }, LOG_POLL_MS);
}

function stopLogPolling(): void {
  if (logTimer !== null) clearTimeout(logTimer);
  logTimer = null;
}

/**
 * 事件流：`scrape_drain(50)` 取增量转成行追加。只在**运行中**拉（空闲时白烧 CPU），
 * 失败静默——辅助信息不该盖住真正的错误。
 */
async function drainFeed(): Promise<void> {
  try {
    const updates = await scrapeDrain(50);
    if (updates.length === 0) return;
    const lines: FeedLine[] = [];
    for (const u of updates) {
      // 风控停止：不进事件流（那里只放岗位行），而是进错误队列，并挂「恢复」——
      // 用户过完验证后点它，就是重新 scrape_start。
      if (u.kind === 'risk') {
        fail(`触发风控已停止：${u.reason}。请在浏览器中手动完成验证后，点「恢复」重新抓取`,
          'scrape_risk', { label: '恢复', run: () => void onStart() });
        continue;
      }
      // 事件不带时间戳：用一次 drain 的接收时刻补上，与运行日志同一刻度后混排。
      feedN += 1;
      const line = toFeedLine(u, Date.now(), feedN);
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
    // 门槛以库里存的为准：`score_min_default` 只是「从没设置过」时的兜底。
    // 只在 null 时回填会让用户改过的值在重进页面后被默认值盖掉（输入框回显 70，
    // 而打分日志按库里的 60 生效）。
    if (Number.isFinite(cfg.min_score)) minScore.value = cfg.min_score;
    maxJd.value = cfg.max_jd;
    scoreMode.value = cfg.score_mode === 'llm' ? 'llm' : 'rule';
    llmPrefilter.value = clampPrefilter(cfg.llm_prefilter);
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

/**
 * 只重读 token 三键。后端在打分 / 打招呼 / 自动投递的回执里累加（ADR-005），读数变了顶栏就得
 * 当场跳一下（`TopbarTail` 的 `token-bump` 挂在 props 变化上）。失败静默：读数是辅助信息，
 * 不值得一张错误卡片，下一轮自会重读。
 */
async function refreshTokens(): Promise<void> {
  try {
    config.value = await getAppConfig();
  } catch {
    /* 见上：不弹错误卡片、不进重试队列 */
  }
}

// 有人刚花掉 token（聊天里生成话术、自动投递发出一条）：重读一次权威累计。
watch(tokenSpentTick, () => void refreshTokens());

// ---------------------------------------------------------------- 写：筛选

function onDraft(next: FilterDraft): void {
  draft.value = next;
  saveDraft(next); // 本地兜底立刻写，不等防抖
  if (saveTimer !== null) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => void persistDraft(), SAVE_DEBOUNCE_MS);
}

/** 关键词只落本地：改一次写一次，F5 之后原样回来。 */
function onKeywords(next: readonly string[]): void {
  keywords.value = [...next];
  saveKeywords(next);
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

/**
 * 筛选抽屉的「应用」= **只保存条件**，不连锁登录把关与抓取。
 * 顺序不能反：先落库（`set_scrape_defaults`）再关抽屉，否则用户看不到保存结果；
 * 保存失败也不拦关闭。
 */
async function applyNow(): Promise<void> {
  if (saveTimer !== null) clearTimeout(saveTimer);
  await persistDraft();
  filterNote.value = '筛选条件已保存';
  closeFilter();
}

/** 「应用」入口把关闭抽屉的时机交给这里：先关，用户才看得到抓取条上的进度。 */
function onApplyFilter(): void {
  void applyNow();
}

/**
 * 抓取前的登录把关，返回 true 表示可以继续。已登录直接放行；未登录且用户确认则拉起扫码
 * （`loginStart`）但**不**继续抓取——扫码是异步的，此刻 profile 里还没有 cookie；用户取消则
 * 返回 false 且不起任何进程。用 `window.confirm` 是不想为一个是/否询问多一层遮罩状态。
 */
async function ensureLogin(): Promise<boolean> {
  await refreshLogin();
  if (isLoggedIn()) return true;

  const label = PLATFORMS.find((p) => p.id === platform.value)?.name ?? platform.value;
  const go = window.confirm(`${label} 尚未登录。\n\n抓取需要先登录：点「确定」打开浏览器扫码，登录完成后再回来点「开始抓取」。`);
  if (!go) {
    filterNote.value = '未登录：已取消，未开始抓取';
    return false;
  }
  await onLogin(platform.value);
  filterNote.value = '请在弹出的浏览器里扫码登录，完成后回到这里点「开始抓取」';
  return false;
}

/** 「恢复默认」只重置并保存，不连锁抓取（用户点的是重置，不是开始）。 */
function resetFilter(): void {
  onDraft(createDraft());
  if (saveTimer !== null) clearTimeout(saveTimer);
  void persistDraft();
}

/** 用户主动点「读取期望」入口。未登录只给提示，不发起请求（求职期望是被动能力）。 */
function onReadExpect(): void {
  if (!isLoggedIn()) {
    filterNote.value = '请先登录后再读取求职期望';
    return;
  }
  void loadExpects();
}

/** 已登录守卫：只认 `login_status` / `login-state` 给出的 `logged_in`。 */
function isLoggedIn(): boolean {
  return login.value?.state === 'logged_in';
}

/**
 * 读求职期望（`list_expectations`）。仅在用户点击且已登录时调用（见 `onReadExpect`）。
 * 失败进错误队列但**不挡操作**，且**不做自动重试**——「重试」按钮只在用户点击时生效。
 */
async function loadExpects(): Promise<void> {
  try {
    const list = await listExpectations();
    expects.value = list;
    filterNote.value = `已读取 ${list.length} 条求职期望`;
  } catch (e) {
    fail(`读取期望职位失败：${errText(e)}`, 'list_expectations', {
      label: '重试',
      run: () => onReadExpect(),
    });
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

/** 三类批量动作的落地：不投递（否决）/ 已投递 / 待投递（撤销否决或撤销投递）。 */
async function applyBatchOp(op: BatchOp, ids: readonly string[]): Promise<number> {
  if (op === 'reject') return await rejectJobs(ids);
  if (op === 'deliver') return await markDelivered(ids);
  // 「待投递」是撤回：不投递来的撤销否决，已投递来的撤销投递标记，两边都回到未投递。
  const pendingIds = jobs.value.filter((j) => ids.includes(j.job_id) && j.status !== 'delivered').map((j) => j.job_id);
  const deliveredIds = ids.filter((id) => !pendingIds.includes(id));
  let hit = 0;
  if (pendingIds.length > 0) hit += await allowJobs(pendingIds);
  if (deliveredIds.length > 0) hit += await unmarkDelivered(deliveredIds);
  return hit;
}

async function batch(op: BatchOp): Promise<void> {
  const ids = selection.value.checked.slice();
  if (!showBatchActions(ids.length)) return;
  try {
    const hit = await applyBatchOp(op, ids);
    const name = op === 'reject' ? '不投递' : op === 'deliver' ? '已投递' : '待投递';
    filterNote.value = `${name}：影响 ${hit} 条（勾选 ${ids.length} 条）`;
    selection.value = { selecting: false, checked: [] };
    await refreshList();
  } catch (e) {
    const code = op === 'reject' ? 'reject_jobs' : op === 'deliver' ? 'mark_delivered' : 'allow_jobs';
    fail(`批量操作失败：${errText(e)}`, code, { label: '重试', run: () => void batch(op) });
  }
}

/** 卡片上的「撤销否决」（只在「不投递」页签出现）：恢复后该岗位回到待投递列表。 */
async function onAllowJob(jobId: string): Promise<void> {
  try {
    const hit = await allowJobs([jobId]);
    filterNote.value = actionNote('allow', hit);
    // 多选态只剩一个入口（勾选框 / 全选本页），单条动作后没有理由停在多选态。
    selection.value = { selecting: false, checked: selection.value.checked.filter((id) => id !== jobId) };
    await refreshList();
  } catch (e) {
    fail(`${actionFailPrefix('allow')}${errText(e)}`, 'allow_jobs', {
      label: '重试',
      run: () => void onAllowJob(jobId),
    });
  }
}

/** 卡片上的「待投递」（只在「已投递」页签出现）：取消投递标记后回到未投递列表。 */
async function onUndeliverJob(jobId: string): Promise<void> {
  try {
    const hit = await unmarkDelivered([jobId]);
    filterNote.value = `待投递：影响 ${hit} 条`;
    selection.value = { selecting: false, checked: selection.value.checked.filter((id) => id !== jobId) };
    await refreshList();
  } catch (e) {
    fail(`取消投递失败：${errText(e)}`, 'unmark_delivered', {
      label: '重试',
      run: () => void onUndeliverJob(jobId),
    });
  }
}

// ---------------------------------------------------------------- 详情抽屉

/** 卡片单击 = 打开详情。卡片自己把整条岗位带回来，抽屉不依赖列表快照。 */
function onOpenDetail(job: Job): void {
  detailJob.value = job;
  detail.value = openDetail(detail.value, job.job_id);
}

function onCloseDetail(): void {
  detail.value = closeDetail();
  detailJob.value = null;
}

/**
 * 抽屉底部三个单条动作。与批量条共用同一批命令（reject_jobs / allow_jobs /
 * mark_delivered，载荷都是 `{args:{jobIds}}`），差别只在文案。成功后关抽屉并刷新，失败进
 * 错误队列——**不让抽屉停在半更新状态**。
 */
async function onDetailAction(action: DetailAct, jobId: string): Promise<void> {
  /*
   * `jobId` 必须由抽屉自己带过来，**不能**回头读 `detailJob`：列表是分页窗口，翻页 / 排序后
   * 那条岗位可能已不在其中，反查落空就表现为「按钮点了没反应」。
   */
  if (!jobId) return;
  const ids = [jobId];
  const code =
    action === 'reject'
      ? 'reject_jobs'
      : action === 'allow'
        ? 'allow_jobs'
        : action === 'undo-deliver'
          ? 'unmark_delivered'
          : 'mark_delivered';
  try {
    const hit =
      action === 'reject'
        ? await rejectJobs(ids)
        : action === 'allow'
          ? await allowJobs(ids)
          : action === 'undo-deliver'
            ? await unmarkDelivered(ids)
            : ((await markDelivered(ids)), ids.length);
    filterNote.value = actionNote(action, hit);
    detail.value = closeDetail();
    await refreshList();
  } catch (e) {
    fail(`${actionFailPrefix(action)}${errText(e)}`, code, {
      label: '重试',
      run: () => void onDetailAction(action, jobId),
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
    const next = await loginStatus(platform.value);
    /*
     * `unknown` 是后端的**降级值**（sidecar 不在线或 `session.open` 回执失败），不是「未登录」的
     * 结论。降级值不得抹掉已知结论（同 `mergeLoginState`）：上一次的 `logged_in` / `no_login`
     * 继续留着；只有 `saved === true` 时才把它升格成 `no_login`。
     */
    if (next.state === 'unknown' && login.value !== null) {
      const known = login.value.state !== 'unknown';
      if (known) {
        // 保留旧 state，只更新 running / message / saved 这些与本次查询相关的字段。
        login.value = { ...login.value, running: next.running, message: next.message, saved: next.saved };
        return;
      }
    }
    login.value = next;
  } catch {
    // 命令本身失败（IPC 异常）：同样不能把已知结论清成「未知」，留旧值。
    if (login.value === null) login.value = null;
  }
}

/**
 * 订阅 `login-state`（后端旁路推送，500ms 粒度）。事件只带 `(platform, state, logged_in)`，
 * 缺 `running` / `saved` / `message`，所以走 `mergeLoginState` 合并而不是整份替换，否则
 * 「已登录」会被一条迟到的推送降级成「未知」。非 Tauri 环境 `listenLoginState` 返回 no-op。
 */
async function subscribeLoginState(): Promise<void> {
  if (unlistenLogin !== null) return;
  try {
    const off = await listenLoginState(
      (payload) => {
        login.value = mergeLoginState(login.value, payload);
        // 消费完就回执：两个页面都确认后后端立刻停掉 5 秒补发。
        void loginAck(payload.platform, payload.state);
        // 登录成功那一拍补一次权威查询：`saved` / `message` 只在查询里有。
        if (payload.state === 'logged_in') void refreshLogin();
      },
      () => undefined,
    );
    // 订阅是 await 的，期间组件可能已卸载：那就立刻退订，别留悬挂监听。
    if (disposed) {
      off();
      return;
    }
    unlistenLogin = off;
  } catch {
    // 订阅失败只影响实时性，兜底查询与 5s tick（`refreshLogin`）仍在。
    unlistenLogin = null;
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

/**
 * 「退出登录」：清标记，运行中的会话会让 sidecar 一起关掉
 * （`LogoutReply.closed`）。退出后立刻把徽标拉回权威状态。
 */
async function onLogout(): Promise<void> {
  if (loginActionBusy.value) return;
  loginActionBusy.value = true;
  try {
    await logout(platform.value);
    filterNote.value = '已退出登录';
  } catch (e) {
    fail(`退出登录失败：${errText(e)}`, 'logout', { label: '重试', run: () => void onLogout() });
  } finally {
    loginActionBusy.value = false;
    await refreshLogin();
  }
}

/**
 * 抓取进行中的进度轮询。**只在运行中挂载**，空闲时完全不发 IPC（登录态查询会触碰浏览器）。
 * 进度与日志仍靠它推进：`scrape_drain` 没有独立推送通道，运行中必须保留轮询。
 */
function syncTick(): void {
  const need = progress.value.running;
  if (need && tickTimer === null) {
    tickTimer = setInterval(() => {
      void refreshProgress();
      if (canPollLogs()) startLogPolling();
      else stopLogPolling();
      // 抓取已结束：立刻卸掉轮询，不让空闲态继续发请求。
      if (!progress.value.running) {
        void refreshCounts();
        syncTick();
      }
    }, TICK_MS);
    return;
  }
  if (!need && tickTimer !== null) {
    clearInterval(tickTimer);
    tickTimer = null;
    void refreshCounts();
  }
}

/** WebView 从挂起恢复：`setTimeout` / 事件都可能断过，补一次权威查询。 */
function onVisibilityChange(): void {
  if (document.visibilityState === 'visible') {
    void refreshLogin();
    syncTick();
  }
}

async function onGreet(jobId: string): Promise<void> {
  const job = jobs.value.find((j) => j.job_id === jobId);
  setGreetDraft(job?.company ?? '', job?.title ?? '', undefined, jobId);
  // 不切页：草稿留给同屏的聊天层就地消费（匹配好友 ⇒ 选中 ⇒ 预填 + focus ⇒ 不发送）。
  emit('greet');
}

/** 抽屉「去跟 Boss 聊」：与卡片打招呼同一条草稿路径，并收起抽屉让聊天层接管。 */
function onDetailChat(jobId: string): void {
  detail.value = closeDetail();
  void onGreet(jobId);
}

// ---------------------------------------------------------------- 卡片单击 / 长按

/** 多选期间单击只切勾选，其余情况打开详情。 */
function onCardOpen(job: Job): void {
  if (selection.value.selecting) {
    selection.value = clickCard(selection.value, job.job_id);
    return;
  }
  onOpenDetail(job);
}

/** 长按到点：进入多选，并勾上按下的这张。 */
function onCardLongPress(jobId: string): void {
  selection.value = enterSelecting(jobId);
}

/** 多选态下点列表空白处：退出多选。 */
function onCheckBlank(): void {
  selection.value = clickBlank(selection.value);
}

// ---------------------------------------------------------------- 抓取

/**
 * 「开始抓取」：**先登录把关，再抓取**。抽屉不由这里打开，唯一例外是关键词为空——
 * 抓取根本没法发起，弹抽屉让用户就地补条件。
 */
async function onStart(): Promise<void> {
  if (startDisabled(starting.value, progress.value.running)) return;
  // 搜索词只认 `keywords`。为空就拦下来，交给后端回落默认词表会
  // 让用户以为"我填的排除词生效了"——其实一个搜索词都没发出去。
  const kws = searchKeywords(keywords.value);
  if (kws.length === 0) {
    filterNote.value = KW_REQUIRED_NOTE;
    openFilter();
    return;
  }
  // 登录把关放在发请求之前：未登录时 scrape_start 必然失败，先问再走。
  if (!(await ensureLogin())) return;
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
 * 两处限额输入框的保存动作（`on_max_jd` / `on_min_score` 同口径）。只走 `set_limit`：
 * `min_score` 与 `max_jd` 都不在 `set_app_config` 的 `WRITABLE_KEYS` 里，塞进 patch 会被
 * **静默跳过**。
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

/**
 * 概览业务尾段（平台卡 + `.tb-tail`）有没有落位进壳的那一行。`AppShell` 在 header 里留了
 * 一个 `display: contents` 的 `.tb-embed`：数据归本页，DOM 归顶栏那一行。单测直接
 * `mount(DashboardPage)` 时壳不在场 ⇒ Teleport `disabled`，就地渲染。
 */
const tailInTopbar = ref(false);

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

// ---------------------------------------------------------------- 生命周期

/* 跨页联动：聊天页展开某 Boss 时，概览页高亮同公司的岗位（键由 chat-model 统一定义）。 */
const highlightCompany = ref('');
function readHighlightCompany(): string {
  try {
    return localStorage.getItem(CHAT_ACTIVE_COMPANY_KEY) ?? '';
  } catch {
    return '';
  }
}
function onStorageHighlight(ev: StorageEvent): void {
  if (ev.key === CHAT_ACTIVE_COMPANY_KEY || ev.key === null) highlightCompany.value = readHighlightCompany();
}
/** 同页事件（写入方是本标签页的聊天层）：直接重读，不依赖 storage 事件。 */
function onSamePageHighlight(): void {
  highlightCompany.value = readHighlightCompany();
}

/** 捕获阶段判定点击是否落在筛选抽屉内，不落在就收起。只关自己的抽屉（JD 抽屉由
 *  JobDetailDrawer 的遮罩自理）。城市选择器被 Teleport 到 body，DOM 上已不在抽屉里，
 *  所以按 `data-overlay="popover"` 再放行一次；它自己负责点外面关闭，这里不能越权。 */
function onGlobalPointerDown(ev: PointerEvent): void {
  if (!filterOpen.value) return;
  const target = ev.target as HTMLElement | null;
  if (target === null) return;
  if (target.closest('[data-testid="scrape-filter-bar"]') !== null) return;
  if (target.closest('[data-overlay="popover"]') !== null) return;
  closeFilter();
}

onMounted(() => {
  overlayRoot.value = document.querySelector('#app-overlay-root');
  // 首帧主题来自 index.html 脚本，Rust 不知道；挂载时补一次，让窗口底色与界面一致。
  // 非 Tauri 环境里 `set_window_theme` 不可用，失败不影响界面（纯外观增强）。
  void setWindowTheme(readInitialTheme()).catch(() => undefined);
  tailInTopbar.value = document.querySelector('.tb-embed') !== null;
  highlightCompany.value = readHighlightCompany();
  window.addEventListener('storage', onStorageHighlight);
  window.addEventListener(CHAT_ACTIVE_COMPANY_EVENT, onSamePageHighlight);
  document.addEventListener('pointerdown', onGlobalPointerDown, true);
  void loadSpecAndConfig();
  void refreshList();
  void refreshProgress();
  // 日志与求职期望都是被动能力：挂载时不拉取、不无条件轮询（见 `startLogPolling` 的闸门）。
  void refreshLogin();
  void subscribeLoginState();
  document.addEventListener('visibilitychange', onVisibilityChange);
  // **不再有 5 秒常驻 tick**：空闲时既不该查登录态也不该查进度。
  // 原来那个 `setInterval` 每 5 秒调一次 `loginStatus`，而它会经 Rust
  // `login_status` → sidecar `session.open` 把浏览器导航一遍——用户看到的是
  // 浏览器窗口/标签页被反复打开。登录态改由 `login-state` 事件驱动
  // （`subscribeLoginState`），断流时只在下述运行态或用户主动操作时补查。
  syncTick();
});

onBeforeUnmount(() => {
  disposed = true;
  document.removeEventListener('visibilitychange', onVisibilityChange);
  document.removeEventListener('pointerdown', onGlobalPointerDown, true);
  window.removeEventListener('storage', onStorageHighlight);
  window.removeEventListener(CHAT_ACTIVE_COMPANY_EVENT, onSamePageHighlight);
  if (unlistenLogin !== null) {
    unlistenLogin();
    unlistenLogin = null;
  }
  if (tickTimer !== null) clearInterval(tickTimer);
  stopLogPolling();
  if (saveTimer !== null) clearTimeout(saveTimer);
});

watch(
  () => tab.value,
  () => {
    selection.value = { selecting: false, checked: [] };
  },
);
</script>

<template>
  <!-- 容器锚点 shell-dashboard。 -->
  <section class="page page-dashboard" data-testid="shell-dashboard">
    <AppErrorBar
      :queue="errors"
      @dismiss="onDismissError"
      @dismiss-all="onDismissAllErrors"
      @retry="onRetry"
    />

    <!-- 顶栏单行落位（SPEC-001 决策 7）：平台卡与尾段的数据在本页，DOM 属于壳的那一行，
         所以整段 teleport 进 AppShell 的 `.tb-embed`。落位点不存在（单测独立挂载）时
         disabled ⇒ 就地渲染，不产生 Teleport 警告。 -->
    <Teleport to=".tb-embed" :disabled="!tailInTopbar" defer>
      <PlatformCard
        :selected="platform"
        :login="login"
        :login-busy="loginBusy"
        :action-busy="loginActionBusy"
        @pick="onPickPlatform"
        @login="onLogin"
        @logout="onLogout"
      />

      <TopbarTail
        :segs="segs"
        :llm-prefilter="llmPrefilter"
        :tokens-total="tokensTotal"
        :tokens-greet="tokensGreet"
        :tokens-score="tokensScore"
        :sensitive-open="sensitiveOpen"
        :theme-label="themeLabel"
        :palette-id="paletteId"
        :palettes="PALETTES"
        :font-scale-id="fontScaleId"
        :font-scales="FONT_SCALES"
        @pick-font-scale="
          (id: FontScaleId) => {
            fontScaleId = id;
            applyFontScale(id);
          }
        "
        @pick-palette="
          (id: PaletteId) => {
            paletteId = id;
            applyPalette(id, currentTheme());
          }
        "
        @toggle-sensitive="
          () => {
            sensitiveOpen = !sensitiveOpen;
          }
        "
        @pick-mode="onPickMode"
        @update:prefilter="onPrefilter"
        @open-config="emit('navigate', 'config')"
        @pick-theme-mode="onPickThemeMode"
        @toggle-theme-panel="onToggleThemePanelHandler"
      />
    </Teleport>

    <ScrapeBar
      :progress="progress"
      :state="state"
      :starting="starting"
      :max-jd="maxJd"
      :min-score="minScore ?? 0"
      :score-max="scoreMax"
      :scraped-total="progress.inserted"
      :filter-open="filterOpen"
      @start="onStart"
      @stop="onStop"
      @update:max-jd="onMaxJd"
      @update:min-score="onMinScore"
      @toggle-filter="filterOpen ? closeFilter() : openFilter()"
    />

    <div class="job-fill">
      <JobListView
        :jobs="shownJobs"
        :counts="shownCounts"
        :tab="tab"
        :show-all="showAll"
        :sort-field="sortField"
        :sort-descending="sortDescending"
        :selection="selection"
        :score-max="scoreMax"
        :highlight-company="highlightCompany"
        :loading="listLoading"
        :error="listError"
        :has-more="hasMore"
        :page-loading="pageLoading"
        @load-more="loadMore"
        @update:tab="onTab"
        @update:show-all="onShowAll"
        @sort="onSort"
        @open="onCardOpen"
        @longpress="onCardLongPress"
        @check-blank="onCheckBlank"
        @refresh="refreshList()"
        @toggle="selection = { ...selection, checked: toggleChecked(selection.checked, $event) }"
        @check-all="(checked: boolean) => (selection = setChecked(checked ? shownJobs.map((j) => j.job_id) : []))"
        @batch="batch"
        @greet="onGreet"
        @allow="onAllowJob"
        @undeliver="onUndeliverJob"
        @empty-filter="openFilter()"
        @empty-start="onStart"
      />
      <!-- 筛选抽屉：浮在窗口左侧、盖住岗位列表原位置，列表与日志卡不被挤动。
           面板节点常驻：开合只切 `.is-open`，由 src/app.css 补间位移与透明度；
           收起态 visibility: hidden + pointer-events: none，`inert` 兜住焦点与读屏。 -->
      <div
        v-show="filterOpen"
        class="filter-backdrop"
        data-testid="scrape-filter-backdrop"
        @click="closeFilter"
      ></div>
      <div
        id="scrape-filter-bar"
        ref="filterBar"
        class="drawer scrape-filter-bar is-floating"
        :class="{ 'is-open': filterOpen }"
        data-testid="scrape-filter-bar"
        :inert="!filterOpen"
        @click.stop
      >
        <FilterPanel
          :draft="draft"
          :keywords="keywords"
          :expects="expects"
          :saving="saving"
          :note="filterNote"
          @update:draft="onDraft"
          @update:keywords="onKeywords"
          @apply="onApplyFilter"
          @cancel="closeFilter"
          @reset="resetFilter"
          @read-expect="onReadExpect"
        />
      </div>
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

  </section>

  <!-- 抽屉与敏感库是全窗浮层，必须挂在没有 overflow / transform / 滤镜的浮层根上：
       `.page-dashboard` 是 `overflow: hidden` 的滚动容器，作为它后代的 `position: fixed`
       元素会被裁进那一列，表现为「点了卡片右侧不展开」。 -->
  <Teleport to="#app-overlay-root" :disabled="overlayRoot === null">
    <JobDetailDrawer
      :job="detailJob"
      :open="detail.jobId !== null"
      :min-score="minScore"
      :score-max="scoreMax"
      :tab="tab"
      @close="onCloseDetail"
      @act="onDetailAction"
      @chat="onDetailChat"
    />

    <SensitivePanel
      v-if="sensitiveMounted"
      :open="sensitiveOpen"
      :page="sensitivePage"
      :loading="sensitiveLoading"
      :error="sensitiveError"
      @close="sensitiveOpen = false"
      @reload="loadSensitive"
      @remove="removeSensitive"
    />
  </Teleport>
</template>
