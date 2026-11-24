<!--
  岗位详情抽屉（M5 补遗）。

  外壳照冻结原型 job-detail-*：job-detail-drawer / -title / -close-btn /
  -body / -reject-btn / -deliver-btn。
  正文与字段口径照 Leptos features/jobs/drawer.rs（原型正文只是占位串）。
  滑入动画用 C17 的 drawer-in-play，退场不加类（原型用 .is-open 过渡）。
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
import { CLASS, playNextFrame } from '../lib_ui/motion';
import GlassClone from './GlassClone.vue';

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
  (e: 'act', action: DetailAct): void;
  (e: 'chat', jobId: string): void;
}>();

const panel = ref<HTMLElement | null>(null);
/** 补拉后的完整岗位（`jd_text` 在列表接口里是空的）。 */
const full = ref<Job | null>(null);
const jdLoading = ref(false);
const jdError = ref('');

let alive = true;
let loadToken = 0;

/** 关闭：先广播再清本地态——父组件据此把 `job` 置空。 */
function close(): void {
  emit('close');
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
      // 面板挂载后补一帧再打动画类：初始态必须先被浏览器算一次，
      // 否则初始态与动画同帧生效，进场会被整段跳过（`drawer.rs:90-97`）。
      void nextTick(() => {
        if (panel.value) playNextFrame(panel.value, CLASS.DRAWER_IN);
      });
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
});
onBeforeUnmount(() => {
  alive = false;
  window.removeEventListener('keydown', onKeydown);
  document.removeEventListener('pointerdown', onDocPointerDown, true);
});

/** 点击抽屉以外的任意位置关闭（捕获阶段）：遮罩只覆盖右侧一列，
 *  页面其余区域的点击不会冒泡到遮罩上，必须挂在 document 层判定。
 *  规则同 Esc：上层（决策记录 / 配置）开着时这层不出手。 */
function onDocPointerDown(ev: PointerEvent): void {
  if (!props.job) return;
  if (hasHigherOverlay('drawer')) return;
  const el = ev.target as HTMLElement | null;
  if (el === null) return;
  if (el.closest('[data-testid="job-detail-drawer"]') !== null) return;
  // 抽屉的触发点（岗位卡）不参与关闭，否则「点开」会立刻被同一次点击关掉。
  if (el.closest('[data-testid="job-detail-close-btn"]') !== null) return;
  close();
}

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
  emit('act', a);
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
    class="drawer glass-l2 hl-surface"
    :class="{ 'is-open': isOpen }"
    data-testid="job-detail-drawer"
    role="dialog"
    aria-label="岗位详情"
    aria-modal="false"
  >
    <!-- L2-2 P0：面板背后窗内内容（.dashboard）的非交互折射副本（裁定 6），开合时建 / 拆。 -->
    <GlassClone mode="backdrop" :active="isOpen" />
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
