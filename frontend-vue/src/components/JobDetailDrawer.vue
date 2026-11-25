<!--
  岗位详情抽屉（M5 补遗）。

  外壳照冻结原型 job-detail-*：job-detail-drawer / -title / -close-btn /
  -body / -reject-btn / -deliver-btn。
  正文与字段口径照后端口径（原型正文只是占位串）。
  展开与收起统一由 `lib_ui/pan.ts` 驱动（类 `.drawer-anim` / `.is-closing`，
  时长与曲线在 motion.css 第 0、6 段），与其他三个抽屉完全一致。
-->
<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import type { Job } from '../api/types';
import { getJob } from '../api/commands/db';
import { hasHigherOverlay } from '../lib_ui/overlay-stack';
import { emptyJob } from '../lib_ui/job';
import {
  DASH,
  actionFailPrefix,
  closeDetail,
  detailActions,
  detailChips,
  detailCompany,
  detailHeadChips,
  detailJdText,
  detailKv,
  detailLink,
  detailReason,
  detailScoreClass,
  detailScoreMaxText,
  detailScoreText,
  detailTitle,
  detailWhen,
  needsJdFetch,
  openDetail,
  type DetailAct,
  type DetailAction,
  EMPTY_DETAIL,
} from '../lib_ui/job-detail-model';
import { useDrawerPresence } from '../lib_ui/use-drawer';

const props = withDefaults(
  defineProps<{
    /** `null` = 关着。 */
    job?: Job | null;
    /** 门槛，来自 `get_scoring_spec`（C10）；`null` = 还没读到。 */
    minScore?: number | null;
    /** 满分，来自 `get_scoring_spec`（C10）。 */
    scoreMax?: number | null;
    /** 当前列表页签：决定底部动作按钮（`pending` / `delivered` / `rejected`）。 */
    tab?: string;
  }>(),
  { job: null, minScore: null, scoreMax: null, tab: 'pending' },
);

const emit = defineEmits<{
  (e: 'close'): void;
  /** 单条动作：带上这张卡的 job_id，页面据此改库（不回读列表快照）。 */
  (e: 'act', action: DetailAct, jobId: string): void;
  (e: 'chat', jobId: string): void;
}>();

/** 展开 / 收起过渡：展开由 watch 触发，收起点击即刻广播 close。 */
const { panel, animateOpen, animateClose } = useDrawerPresence(() => emit('close'));
/**
 * 点击抽屉以外即关闭。
 *
 * 三条约束，缺一条就会出现「点了没反应」：
 * 1. **挂 `click` 而不是 `pointerdown`**：打开发生在一次 pointerdown → pointerup → click
 *    的末尾，关闭判据若挂在 pointerdown 上，用户「下一次」按住任意位置的第一个事件就把
 *    抽屉关掉了。
 * 2. **`pressOutside` 记的是按下起点**：在面板里按下、拖到外面松开时，浏览器补发的
 *    click 目标是两者的共同祖先，只看 target 会把一次拖选当成点外部。
 * 3. **打开那次 click 本身不算「点外面」**（`openedAt` 冷却）：打开由卡片的 click 发起，
 *    同一次 click 继续冒泡到本监听器时抽屉已经开着、起点又落在卡片（面板外），
 *    于是被同一击关掉——实测控制台里就是 `card click → job prop -> id → doc click` 后
 *    立刻 `job prop -> null`。列表卡片与抽屉是同一棵树的兄弟分支，这一击必然同时命中
 *    打开与关闭两条路径，只能按「刚刚打开」把它滤掉。
 */
let pressOutside = true;
/** 最近一次「由 null 变成有值」的时刻：这之后的第一次 click 只可能是打开它的那一击。 */
let openedAt = 0;

function onDocPointerDown(ev: PointerEvent): void {
  const el = ev.target as HTMLElement | null;
  pressOutside = el === null || el.closest('[data-testid="job-detail-drawer"]') === null;
}

function onDocClick(ev: MouseEvent): void {
  const outside = pressOutside;
  pressOutside = true;
  if (!props.job) return;
  if (hasHigherOverlay('drawer')) return;
  const el = ev.target as HTMLElement | null;
  if (el === null) return;
  if (el.closest('[data-testid="job-detail-drawer"]') !== null) return;
  if (!outside) return;
  // 打开它的那一击与关闭判据是同一个事件：同一轮事件循环内到达的这次 click 一律放过。
  if (performance.now() - openedAt < 120) return;
  close();
}
/** 补拉后的完整岗位（`jd_text` 在列表接口里是空的）。 */
const full = ref<Job | null>(null);
const jdLoading = ref(false);
const jdError = ref('');

let alive = true;
let loadToken = 0;

/** 关闭：先播收起回程，过渡结束再广播——父组件那时才把 `job` 置空。 */
function close(): void {
  animateClose();
}

/** 按当前 job 初始化，并在需要时补拉完整 JD（失败不干掉抽屉）。 */
async function load(job: Job | null): Promise<void> {
  full.value = job;
  jdError.value = '';
  if (!job) {
    jdLoading.value = false;
    return;
  }
  if (!needsJdFetch(job)) {
    jdLoading.value = false;
    return;
  }
  jdLoading.value = true;
  const token = ++loadToken;
  try {
    const fetched = await getJob(job.job_id);
    if (!alive || token !== loadToken) return;
    if (fetched) full.value = fetched;
  } catch (e) {
    if (!alive || token !== loadToken) return;
    jdError.value = e instanceof Error ? e.message : String(e);
  } finally {
    if (alive && token === loadToken) jdLoading.value = false;
  }
}

watch(
  () => props.job,
  (job) => {
    void load(job);
    if (job) {
      openedAt = performance.now();
      // 面板节点在这一帧之后才存在，等 tick 完再挂展开类（pan.ts 内部会强制回流）。
      void nextTick(animateOpen);
    }
  },
  { immediate: true },
);

// Esc 关闭。挂 window 而不是面板：用户焦点可能还停在列表里。
// 分层判据（SPEC §6.1 / A10）：决策记录与配置在抽屉之上，它们开着时这层不出手。
function onKeydown(ev: KeyboardEvent): void {
  if (ev.key !== 'Escape') return;
  if (!props.job) return;
  if (hasHigherOverlay('drawer')) return;
  ev.preventDefault();
  close();
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  document.addEventListener('pointerdown', onDocPointerDown, true);
  document.addEventListener('click', onDocClick);
});
onBeforeUnmount(() => {
  alive = false;
  window.removeEventListener('keydown', onKeydown);
  document.removeEventListener('pointerdown', onDocPointerDown, true);
  document.removeEventListener('click', onDocClick);
});

const j = computed<Job | null>(() => full.value);
const isOpen = computed(() => j.value !== null);
const title = computed(() => (j.value ? detailTitle(j.value) : '岗位详情'));
const company = computed(() => (j.value ? detailCompany(j.value) : DASH));
const headChips = computed(() => (j.value ? detailHeadChips(j.value) : []));
const chips = computed(() => (j.value ? detailChips(j.value, props.minScore, props.scoreMax) : []));
const scoreText = computed(() => (j.value ? detailScoreText(j.value.match_score) : DASH));
const scoreCls = computed(() =>
  j.value ? detailScoreClass(j.value.match_score, props.minScore) : 'score-num drawer-score is-none',
);
const scoreMaxText = computed(() => detailScoreMaxText(props.scoreMax));
const reason = computed(() => (j.value ? detailReason(j.value, jdLoading.value) : ''));
const jdText = computed(() =>
  j.value ? detailJdText(j.value, jdLoading.value, jdError.value) : '',
);
const kv = computed(() => (j.value ? detailKv(j.value) : []));
const link = computed(() => (j.value ? detailLink(j.value) : null));
const when = computed(() => (j.value ? detailWhen(j.value) : DASH));
const rejected = computed(() => (j.value ? j.value.rejected !== 0 : false));

/** 底部动作按钮：由当前页签裁（见 `detailActions` 的注释）；未加载出岗位时按页签给占位。 */
const footActions = computed(() =>
  j.value ? detailActions(j.value, props.tab) : detailActions(emptyJob(), props.tab),
);

function act(a: DetailAct): void {
  if (!j.value) return;
  /*
   * 动作会改岗位状态（不投递 / 已投递 / 待投递），抽屉要跟着收——走同一条收起回程。
   *
   * job_id 随事件一起交出去：页面不能靠 `detailJob` 反查（那是当前列表页的快照，
   * 抽屉里的这条未必还在其中，落空就会静默无动作）。
   */
  animateClose();
  emit('act', a, j.value.job_id);
}

/** 「去跟 Boss 聊」：把当前岗位交给页面写草稿，由聊天层同屏消费（不是写操作，不走 act）。 */
function chatWithBoss(): void {
  if (!j.value) return;
  emit('chat', j.value.job_id);
}

/** 供页面在动作成功后同步本地态（避免等下一次 refresh 才更新）。 */
function patch(patchJob: Job): void {
  full.value = patchJob;
  void openDetail(EMPTY_DETAIL, patchJob.job_id);
}

defineExpose({ patch, closeDetail, jdLoading, jdError });
</script>

<template>
  <!-- 抽屉次序（不许调换）：遮罩在前、面板在后 —— DOM 序决定同层先后，遮罩必须先画。
       遮罩刻意**不写死 z-index**（见 styles/drawer.css 注），只老老实实排在面板后面一层。 -->
  <div
    v-if="isOpen"
    class="drawer-backdrop"
    data-testid="job-detail-backdrop"
    aria-hidden="true"
    @click="close"
  />
  <aside
    ref="panel"
    v-show="isOpen"
    class="drawer hl-surface"
    :class="{ 'is-open': isOpen }"
    data-testid="job-detail-drawer"
    role="dialog"
    aria-label="岗位详情"
    aria-modal="false"
  >
    <div class="drawer-head">
      <span class="card-title" data-testid="job-detail-title">{{ title }}</span>
      <span class="spacer" />
      <button
        class="btn btn-sm btn-ghost"
        aria-label="关闭"
        title="关闭（Esc）"
        data-testid="job-detail-close-btn"
        @click="close"
      >
        ×
      </button>
    </div>

    <div class="drawer-body" data-testid="job-detail-body">
      <div class="detail-head-row">
        <span class="card-sub" data-testid="job-detail-company">{{ company }}</span>
        <span
          v-for="(c, i) in headChips"
          :key="`head-${i}`"
          class="chip"
          :data-testid="i === 0 ? 'job-detail-salary' : 'job-detail-location'"
        >
          {{ c }}
        </span>
        <span v-if="rejected" class="chip chip-bad" data-testid="job-detail-rejected-chip">规则否决</span>
      </div>

      <div class="detail-score-row">
        <span :class="scoreCls" data-testid="job-detail-score">{{ scoreText }}</span>
        <span class="card-sub" data-testid="job-detail-score-max">{{ scoreMaxText }}</span>
      </div>

      <div class="detail-chips" data-testid="job-detail-chips">
        <span
          v-for="(c, i) in chips"
          :key="`chip-${i}`"
          class="chip"
          :class="c.tone === 'primary' ? 'chip-primary' : c.tone === 'bad' ? 'chip-bad' : c.tone === 'accent' ? 'chip-accent' : ''"
          :title="c.title ?? undefined"
        >
          {{ c.text }}
        </span>
      </div>

      <section>
        <div class="section-label-static">打分依据</div>
        <div class="jd-text drawer-reason" data-testid="job-detail-reason">{{ reason }}</div>
      </section>

      <section>
        <div class="section-label-static">岗位信息</div>
        <dl class="kv" data-testid="job-detail-kv">
          <template v-for="(row, i) in kv" :key="`kv-${i}`">
            <dt>{{ row.label }}</dt>
            <dd>
              <template v-if="row.label === '链接'">
                <a
                  v-if="link"
                  :href="link"
                  target="_blank"
                  rel="noreferrer"
                  data-testid="job-detail-link"
                >
                  在浏览器打开
                </a>
                <span v-else class="dim" data-testid="job-detail-link-empty">{{ DASH }}</span>
              </template>
              <template v-else>
                {{ row.value }}
                <span
                  v-if="row.chip"
                  class="chip"
                  :class="row.chip.tone === 'bad' ? 'chip-bad' : ''"
                  :data-testid="`job-detail-kv-${row.label}`"
                >
                  {{ row.chip.text }}
                </span>
              </template>
            </dd>
          </template>
        </dl>
        <div class="detail-when" data-testid="job-detail-when">入库 {{ when }}</div>
      </section>

      <section class="detail-jd-sec">
        <div class="section-label-static">职位描述</div>
        <pre class="jd-text" data-testid="job-detail-jd">{{ jdText }}</pre>
      </section>
    </div>

    <div class="drawer-foot" data-testid="job-detail-foot">
      <button
        v-for="meta in footActions"
        :key="meta.action"
        :class="meta.btnClass"
        :data-testid="meta.testId"
        :disabled="meta.disabled"
        @click="act(meta.action)"
      >
        {{ meta.label }}
      </button>
      <button class="btn btn-ghost" data-testid="job-detail-chat" @click="chatWithBoss">去跟 Boss 聊</button>
      <span class="spacer" />
      <span class="detail-foot-note" data-testid="job-detail-foot-note">
        {{ rejected ? '该岗位已被规则否决' : '单条操作，多选用列表批量条' }}
      </span>
    </div>
  </aside>
</template>
