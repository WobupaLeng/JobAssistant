<!--
  按公司搜岗位的浮层（用户 m01529）。

  两条路，互不影响别的会话/岗位：
  - 「查看 JD」走 `company_job_preview`，**只读**：不入库、不出现在任何列表里。
  - 「加入待投递」走 `job_import`，写库后立刻出现在「未投递」页签（父级刷新列表）。

  浮层自己只管展示与两条动作的进行态；搜索请求与结果状态由页面持有
  （`DashboardPage.vue` 的 `companySearch`），因为「加入以后刷新哪个列表」是页面的事。
-->
<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import type { CompanyJobItem, Job } from '../api/types';
import { companyJobPreview, jobImport } from '../api/commands/company';
import {
  emptyNote,
  headline,
  importNote,
  jobMeta,
  type SearchState,
} from '../lib_ui/company-search';

const props = withDefaults(
  defineProps<{
    open?: boolean;
    state: SearchState;
  }>(),
  { open: false },
);

const emit = defineEmits<{
  (e: 'close'): void;
  /** 已成功写库：页面据此刷新列表与计数（不做局部插入，免得两处口径打架）。 */
  (e: 'imported', job: Job): void;
}>();

/** 详情态：正在看的那条岗位（空 = 列表态）。 */
const detail = ref<Job | null>(null);
/** 正在读 JD 的那条（骨架/禁用「查看 JD」用）。 */
const loadingId = ref('');
const detailError = ref('');
/** 正在加入的那条（防重复点击）。 */
const busyId = ref('');
const okNote = ref('');
const failNote = ref('');

const rows = computed(() => props.state.jobs);
const showDetail = computed(() => detail.value !== null);

function errText(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.replace(/^Error:\s*/u, '').trim() || '操作失败';
}

function resetTransient(): void {
  detail.value = null;
  loadingId.value = '';
  detailError.value = '';
  busyId.value = '';
  okNote.value = '';
  failNote.value = '';
}

/* 每次开合都从干净状态开始：上一次的 JD 预览不该留在下一次搜索里。 */
watch(() => props.open, (open) => {
  if (!open) resetTransient();
});
/* 搜了新公司：列表整批换了，详情态也要退回列表。 */
watch(() => props.state.company, () => {
  resetTransient();
});

async function onPreview(job: CompanyJobItem): Promise<void> {
  if (loadingId.value !== '') return;
  detailError.value = '';
  okNote.value = '';
  failNote.value = '';
  loadingId.value = job.job_id;
  try {
    detail.value = await companyJobPreview(job.job_id);
  } catch (e) {
    detailError.value = `读取 JD 失败：${errText(e)}`;
  } finally {
    loadingId.value = '';
  }
}

async function onImport(job: CompanyJobItem): Promise<void> {
  if (busyId.value !== '') return;
  okNote.value = '';
  failNote.value = '';
  busyId.value = job.job_id;
  try {
    // 已经预览过就把 JD 一起带上：省一次详情页请求，也让打分有正文可读。
    const jd = detail.value !== null && detail.value.job_id === `boss:${job.job_id}`
      ? (detail.value.jd_text ?? undefined)
      : undefined;
    const saved = await jobImport({
      job_id: job.job_id,
      title: job.title,
      company: job.company,
      salary: job.salary,
      location: job.location,
      experience: job.experience,
      degree: job.degree,
      job_url: job.url,
      jd,
    });
    okNote.value = importNote(saved);
    emit('imported', saved);
  } catch (e) {
    failNote.value = `加入失败：${errText(e)}`;
  } finally {
    busyId.value = '';
  }
}

function backToList(): void {
  detail.value = null;
  detailError.value = '';
}
</script>

<template>
  <div v-if="open" class="cs-backdrop" data-testid="company-search-backdrop" aria-hidden="true" @click="emit('close')" />
  <section
    v-show="open"
    class="cs-panel card"
    data-testid="company-search-panel"
    role="dialog"
    aria-modal="false"
    aria-label="按公司搜岗位"
    @click.stop
  >
    <header class="cs-head">
      <span class="card-title" data-testid="company-search-title">
        {{ showDetail ? '岗位详情' : '按公司搜岗位' }}
      </span>
      <span class="spacer" />
      <button
        v-if="showDetail"
        class="btn btn-sm btn-ghost"
        type="button"
        data-testid="company-search-back-btn"
        @click="backToList"
      >
        返回列表
      </button>
      <button
        class="btn btn-sm btn-ghost"
        type="button"
        aria-label="关闭"
        data-testid="company-search-close-btn"
        @click="emit('close')"
      >
        关闭
      </button>
    </header>

    <div class="cs-body" data-testid="company-search-body">
      <!-- 详情态：这条岗位的 JD 正文（只读预览，不入库） -->
      <template v-if="showDetail">
        <div class="cs-detail-head">
          <span class="cs-row-title" data-testid="company-search-detail-title">{{ detail?.title || '（无标题）' }}</span>
          <span class="cs-row-salary" data-testid="company-search-detail-salary">{{ detail?.salary || '薪资未标' }}</span>
        </div>
        <p class="cs-row-meta" data-testid="company-search-detail-company">
          {{ detail?.company || '公司未知' }}
        </p>
        <pre class="cs-jd" data-testid="company-search-detail-jd">{{ detail?.jd_text || '（没有读到 JD 正文）' }}</pre>
      </template>

      <!-- 列表态 -->
      <template v-else>
        <p class="cs-headline" data-testid="company-search-headline">{{ headline(state) }}</p>
        <p v-if="state.phase === 'error'" class="cs-note is-error" data-testid="company-search-error">
          {{ state.error }}
        </p>
        <p v-else-if="emptyNote(state) !== ''" class="cs-note" data-testid="company-search-empty">
          {{ emptyNote(state) }}
        </p>

        <ul v-if="rows.length > 0" class="cs-list" data-testid="company-search-list">
          <li v-for="job in rows" :key="job.job_id" class="cs-row" :data-testid="`company-search-row-${job.job_id}`">
            <div class="cs-row-main">
              <span class="cs-row-title">{{ job.title || '（无标题）' }}</span>
              <span class="cs-row-salary">{{ job.salary || '薪资未标' }}</span>
              <span class="cs-row-company">{{ job.company || '公司未知' }}</span>
              <span class="cs-row-meta">{{ jobMeta(job) }}</span>
            </div>
            <div class="cs-row-acts">
              <button
                class="btn btn-sm btn-ghost"
                type="button"
                :disabled="loadingId === job.job_id"
                :data-testid="`company-search-jd-btn-${job.job_id}`"
                @click="onPreview(job)"
              >
                {{ loadingId === job.job_id ? '读取中…' : '查看 JD' }}
              </button>
              <button
                class="btn btn-sm btn-primary"
                type="button"
                :disabled="busyId === job.job_id"
                :data-testid="`company-search-import-btn-${job.job_id}`"
                @click="onImport(job)"
              >
                {{ busyId === job.job_id ? '加入中…' : '加入待投递' }}
              </button>
            </div>
          </li>
        </ul>
      </template>

      <p v-if="detailError !== ''" class="cs-note is-error" data-testid="company-search-detail-error">{{ detailError }}</p>
      <p v-if="okNote !== ''" class="cs-note is-ok" data-testid="company-search-ok">{{ okNote }}</p>
      <p v-if="failNote !== ''" class="cs-note is-error" data-testid="company-search-fail">{{ failNote }}</p>
    </div>
  </section>
</template>