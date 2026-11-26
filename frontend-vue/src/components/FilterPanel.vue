<script setup lang="ts">
/**
 * 完整筛选面板。每次改动立刻写本地草稿，400ms 防抖后只把 diff 发给 `set_scrape_defaults`
 * （diff 与「上次保存的 known」都在父级手里）。**搜索关键词与排除关键词是两块**：搜索词进
 * `scrape_start.keywords`，排除词进草稿的 `exclude`，共用输入框会把排除词当搜索词发出去。
 */
import { computed, ref } from 'vue';
import CityPicker from './CityPicker.vue';
import {
  DEGREE_OPTIONS,
  EXPERIENCE_OPTIONS,
  HR_ACTIVE_OPTIONS,
  INDUSTRY_OPTIONS,
  JOB_TYPE_OPTIONS,
  MAX_PAGES_MAX,
  MAX_PAGES_MIN,
  RECOMMEND_KEYWORDS,
  SALARY_OPEN_HIGH,
  SALARY_OPTIONS,
  SCALE_OPTIONS,
  STAGE_OPTIONS,
  WEEKEND_OPTIONS,
  isCustomSalary,
  salaryLabel,
  tidSafe,
} from '../lib_ui/options';
import {
  KW_DUPLICATE_NOTE,
  KW_PLACEHOLDER,
  activeCount,
  customSalaryText,
  mergeKeywordList,
  normalizeKeywords,
  toggleCode,
  type FilterDraft,
} from '../lib_ui/scrape-filter-model';

const props = withDefaults(
  defineProps<{
    draft: FilterDraft;
    /** 搜索关键词（页面持有，直接进 `scrape_start.keywords`）。 */
    keywords?: readonly string[];
    /** 求职期望 chips（`list_expectations` 的结果，页面读取后传进来）。 */
    expects?: readonly { name: string; city: string; salary: string }[];
    /** 摘要里显示的生效项数（父级算好传进来，避免两处各算一遍）。 */
    saving?: boolean;
    note?: string;
  }>(),
  { keywords: () => [], expects: () => [], saving: false, note: '' },
);

const emit = defineEmits<{
  (e: 'update:draft', draft: FilterDraft): void;
  (e: 'update:keywords', keywords: string[]): void;
  (e: 'apply'): void;
  (e: 'cancel'): void;
  (e: 'reset'): void;
  (e: 'read-expect'): void;
}>();

const kwInput = ref('');
/** 关键词那一排的本地提示（重复词），与保存态的 `note` 分开。 */
const kwNote = ref('');

const count = computed(() => activeCount(props.draft));
const salaryText = computed(() => customSalaryText(props.draft.salary_low, props.draft.salary_high));

/** 草稿是父级的 prop，改动一律克隆后 emit（不就地改 prop）。 */
function patch(part: Partial<FilterDraft>): void {
  emit('update:draft', { ...props.draft, ...part });
}

function toggle(field: 'experiences' | 'degrees' | 'scales' | 'stages' | 'industries', code: string): void {
  const table =
    field === 'experiences'
      ? EXPERIENCE_OPTIONS
      : field === 'degrees'
        ? DEGREE_OPTIONS
        : field === 'scales'
          ? SCALE_OPTIONS
          : field === 'stages'
            ? STAGE_OPTIONS
            : INDUSTRY_OPTIONS;
  patch({ [field]: toggleCode(props.draft[field], code, table) } as Partial<FilterDraft>);
}

function toggleSalary(code: string): void {
  patch({ salaries: toggleCode(props.draft.salaries, code, SALARY_OPTIONS) });
}

/** 关键词入口：写 `keywords`，**绝不写 `draft.exclude`**。 */
function addKeywords(raw: string): void {
  if (raw.trim() === '') return;
  const { list, added } = mergeKeywordList(props.keywords, raw);
  if (added.length === 0) {
    kwNote.value = KW_DUPLICATE_NOTE;
    return;
  }
  kwNote.value = '';
  emit('update:keywords', list);
  kwInput.value = '';
}

function addKeyword(): void {
  addKeywords(kwInput.value);
}

function addRecommend(kw: string): void {
  addKeywords(kw);
}

function removeKeyword(kw: string): void {
  kwNote.value = '';
  emit(
    'update:keywords',
    props.keywords.filter((x) => x !== kw),
  );
}

/** 期望职位 chip：并入搜索关键词（去重，重复不报错——它只是推荐入口）。 */
function addExpect(name: string): void {
  const { list } = mergeKeywordList(props.keywords, name);
  kwNote.value = '';
  emit('update:keywords', normalizeKeywords(list));
}

function addSalaryRange(): void {
  const low = props.draft.salary_low.trim();
  const high = props.draft.salary_high.trim();
  if (low === '' && high === '') return;
  const lo = Number.parseInt(low === '' ? '0' : low, 10);
  const hi = Number.parseInt(high === '' ? String(SALARY_OPEN_HIGH) : high, 10);
  const code = `sal:${Number.isFinite(lo) ? lo : 0}-${Number.isFinite(hi) ? hi : SALARY_OPEN_HIGH}`;
  const next = props.draft.salaries.includes(code) ? props.draft.salaries : [...props.draft.salaries, code];
  patch({ salaries: next, salary_low: '', salary_high: '' });
}

function onMaxPages(e: Event): void {
  const raw = (e.target as HTMLInputElement).value;
  const n = Number.parseInt(raw, 10);
  const v = Number.isFinite(n) ? Math.min(MAX_PAGES_MAX, Math.max(MAX_PAGES_MIN, n)) : props.draft.max_pages;
  patch({ max_pages: v });
}

function salaryCodeLabel(code: string): string {
  return isCustomSalary(code) || SALARY_OPTIONS.some(([c]) => c === code) ? salaryLabel(code) : code;
}
</script>

<template>
  <section class="filter-panel f-full" data-testid="filter-panel" aria-label="抓取筛选">
    <div class="f-head">
      <span class="f-title">筛选条件</span>
      <span class="f-count" data-testid="filter-summary">{{ count }} 项生效</span>
      <span class="f-title2">{{ saving ? '正在保存…' : '改动自动保存' }}</span>
      <span class="spacer"></span>
      <button class="btn btn-sm btn-ghost" type="button" data-testid="filter-reset-btn" @click="emit('reset')">
        重置
      </button>
      <!-- 右上角 × 是唯一出口，不要再加「收起」按钮。 -->
      <button
        class="f-x"
        type="button"
        data-testid="filter-close-btn"
        aria-label="关闭筛选条件"
        title="关闭"
        @click="emit('cancel')"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
          <path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" />
        </svg>
      </button>
    </div>

    <div class="f-body">
      <!-- 搜索关键词（进 scrape_start.keywords，不进 set_scrape_defaults） -->
      <div class="f-group f-group-kw">
        <span class="f-glabel">关键词（可多个，逐个搜索）</span>
        <span class="f-glabel-sub">{{ kwNote || '回车添加；可空格或逗号一次填多个' }}</span>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-pos-chips">
            <!-- 关键词不再挂 × 按钮：点 chip 本身即移除（仍留可访问名与 testid，
                 测试锚点 scrape-filter-kw-del-* 原样保留在 chip 上）。 -->
            <button
              v-for="kw in keywords"
              :key="kw"
              class="f-chip is-on"
              data-testid="scrape-filter-kw-chip"
              :data-del-testid="`scrape-filter-kw-del-${tidSafe(kw)}`"
              type="button"
              :title="kw"
              :aria-label="`移除关键词 ${kw}`"
              :aria-pressed="true"
              @click="removeKeyword(kw)"
            >
              {{ kw }}
            </button>
            <span v-if="keywords.length === 0" class="f-emptychip" data-testid="filter-kw-empty">
              还没有关键词，回车添加或点下面的推荐
            </span>
          </div>
          <div class="f-line">
            <input
              v-model="kwInput"
              class="input f-kw-input"
              type="text"
              :placeholder="KW_PLACEHOLDER"
              aria-label="添加搜索关键词"
              data-testid="scrape-keywords"
              data-alias="scrape-filter-kw-input"
              @keydown.enter.prevent="addKeyword"
            />
            <button class="btn btn-sm btn-ghost" type="button" data-testid="scrape-filter-kw-add-btn" @click="addKeyword">
              添加
            </button>
            <button
              class="btn btn-sm btn-ghost"
              type="button"
              data-testid="scrape-filter-expect-btn"
              @click="emit('read-expect')"
            >
              读取期望职位
            </button>
          </div>
          <div v-if="expects.length > 0" class="f-chips" data-testid="scrape-filter-expect-list">
            <button
              v-for="(e, i) in expects"
              :key="`${e.name}-${i}`"
              class="f-chip"
              type="button"
              :title="`${e.city} · ${e.salary}`"
              :data-testid="`scrape-filter-expect-${i}`"
              @click="addExpect(e.name)"
            >
              {{ e.name }}
            </button>
          </div>
          <div class="f-chips">
            <button
              v-for="(kw, i) in RECOMMEND_KEYWORDS"
              :key="kw"
              class="f-chip"
              type="button"
              :data-kw="kw"
              :data-testid="`scrape-filter-recommend-${i}`"
              @click="addRecommend(kw)"
            >
              {{ kw }}
            </button>
          </div>
        </div>
      </div>

      <!-- 排除关键词 -->
      <div class="f-group">
        <span class="f-glabel">排除关键词</span>
        <span class="f-glabel-sub">每行或逗号分隔；命中即否决</span>
        <div class="f-gbody">
          <textarea
            class="input f-textarea"
            :value="draft.exclude"
            rows="3"
            data-testid="filter-exclude-input"
            aria-label="排除关键词"
            @input="patch({ exclude: ($event.target as HTMLTextAreaElement).value })"
          ></textarea>
        </div>
      </div>

      <!-- 地点与翻页 -->
      <div class="f-group">
        <span class="f-glabel">地点与翻页</span>
        <span class="f-glabel-sub">城市决定 BOSS 的搜索范围</span>
        <div class="f-gbody">
          <div class="f-tri">
            <label class="f-cell">
              <span class="f-cell-label">城市</span>
              <CityPicker :model-value="draft.city" @update:model-value="patch({ city: $event })" />
            </label>
            <label class="f-cell">
              <span class="f-cell-label">工作类型</span>
              <span class="f-select-wrap">
                <select
                  class="input f-select"
                  :value="draft.job_type"
                  data-testid="filter-job-type-select"
                  @change="patch({ job_type: ($event.target as HTMLSelectElement).value })"
                >
                  <option value="">不限</option>
                  <option v-for="[code, name] in JOB_TYPE_OPTIONS" :key="code" :value="code">{{ name }}</option>
                </select>
                <svg class="f-select-caret" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                  <path d="M4 6.4 8 10.4l4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </span>
            </label>
            <label class="f-cell">
              <span class="f-cell-label">最多翻页</span>
              <input
                class="input input-num f-num"
                type="number"
                :min="MAX_PAGES_MIN"
                :max="MAX_PAGES_MAX"
                :value="draft.max_pages"
                data-testid="filter-max-pages-input"
                aria-label="最多翻页"
                @change="onMaxPages"
              />
            </label>
          </div>
        </div>
      </div>

      <!-- 薪资 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">薪资</span>
          <span class="f-count" data-testid="filter-salary-count">已选 {{ draft.salaries.length }}</span>
          <span class="f-glabel-sub">多选，命中任一档即可</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-salary">
            <button
              v-for="([code, name], i) in SALARY_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.salaries.includes(code) }"
              type="button"
              :aria-pressed="draft.salaries.includes(code)"
              :data-testid="`filter-salary-preset-${i}`"
              @click="toggleSalary(code)"
            >
              {{ name }}
            </button>
            <button
              class="f-clear"
              type="button"
              data-testid="filter-salary-clear-btn"
              @click="patch({ salaries: [] })"
            >
              清除
            </button>
          </div>
          <div class="f-line">
            <input
              class="input input-num"
              type="number"
              placeholder="下限 K"
              :value="draft.salary_low"
              data-testid="filter-salary-from-input"
              aria-label="薪资下限"
              @input="patch({ salary_low: ($event.target as HTMLInputElement).value })"
            />
            <span class="f-dash">—</span>
            <input
              class="input input-num"
              type="number"
              placeholder="上限 K"
              :value="draft.salary_high"
              data-testid="filter-salary-to-input"
              aria-label="薪资上限"
              @input="patch({ salary_high: ($event.target as HTMLInputElement).value })"
            />
            <button class="btn btn-sm btn-ghost" type="button" data-testid="filter-salary-add-btn" @click="addSalaryRange">
              添加
            </button>
            <span v-if="salaryText" class="f-hint">{{ salaryText }}</span>
          </div>
          <div v-if="draft.salaries.length > 0" class="f-chips" data-testid="filter-salary-chips">
            <span v-for="code in draft.salaries" :key="code" class="f-chip is-on">{{ salaryCodeLabel(code) }}</span>
          </div>
        </div>
      </div>

      <!-- 工作经验 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">工作经验</span>
          <span class="f-count" data-testid="filter-exp-count">已选 {{ draft.experiences.length }}</span>
          <span class="f-glabel-sub">多选</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-exp">
            <button
              v-for="([code, name], i) in EXPERIENCE_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.experiences.includes(code) }"
              type="button"
              :aria-pressed="draft.experiences.includes(code)"
              :data-testid="`filter-exp-${i}`"
              @click="toggle('experiences', code)"
            >
              {{ name }}
            </button>
            <button class="f-clear" type="button" data-testid="filter-exp-clear-btn" @click="patch({ experiences: [] })">
              清除
            </button>
          </div>
        </div>
      </div>

      <!-- 学历 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">学历</span>
          <span class="f-count" data-testid="filter-edu-count">已选 {{ draft.degrees.length }}</span>
          <span class="f-glabel-sub">多选</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-edu">
            <button
              v-for="([code, name], i) in DEGREE_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.degrees.includes(code) }"
              type="button"
              :aria-pressed="draft.degrees.includes(code)"
              :data-testid="`filter-edu-${i}`"
              @click="toggle('degrees', code)"
            >
              {{ name }}
            </button>
          </div>
        </div>
      </div>

      <!-- 公司规模 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">公司规模</span>
          <span class="f-count" data-testid="filter-scale-count">已选 {{ draft.scales.length }}</span>
          <span class="f-glabel-sub">多选</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-scale">
            <button
              v-for="([code, name], i) in SCALE_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.scales.includes(code) }"
              type="button"
              :aria-pressed="draft.scales.includes(code)"
              :data-testid="`filter-scale-${i}`"
              @click="toggle('scales', code)"
            >
              {{ name }}
            </button>
            <button class="f-clear" type="button" data-testid="filter-scale-clear-btn" @click="patch({ scales: [] })">
              清除
            </button>
          </div>
        </div>
      </div>

      <!-- 融资阶段 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">融资阶段</span>
          <span class="f-count" data-testid="filter-stage-count">已选 {{ draft.stages.length }}</span>
          <span class="f-glabel-sub">多选</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-finance">
            <button
              v-for="([code, name], i) in STAGE_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.stages.includes(code) }"
              type="button"
              :aria-pressed="draft.stages.includes(code)"
              :data-testid="`filter-finance-${i}`"
              @click="toggle('stages', code)"
            >
              {{ name }}
            </button>
          </div>
        </div>
      </div>

      <!-- 行业 -->
      <div class="f-group">
        <div class="f-ghead">
          <span class="f-glabel">行业</span>
          <span class="f-count" data-testid="filter-industry-count">已选 {{ draft.industries.length }}</span>
          <span class="f-glabel-sub">多选；也可直接填行业代码</span>
        </div>
        <div class="f-gbody">
          <div class="f-chips" data-testid="filter-group-industry">
            <button
              v-for="([code, name], i) in INDUSTRY_OPTIONS"
              :key="code"
              class="f-chip"
              :class="{ 'is-on': draft.industries.includes(code) }"
              type="button"
              :aria-pressed="draft.industries.includes(code)"
              :data-testid="`filter-industry-${i}`"
              @click="toggle('industries', code)"
            >
              {{ name }}
            </button>
          </div>
          <div class="f-line">
            <input
              class="input"
              type="text"
              placeholder="行业代码，逗号分隔"
              :value="draft.industries.join(',')"
              data-testid="filter-industry-code-input"
              aria-label="行业代码"
              @change="
                patch({
                  industries: ($event.target as HTMLInputElement).value
                    .split(',')
                    .map((s) => s.trim())
                    .filter((s) => s !== ''),
                })
              "
            />
          </div>
        </div>
      </div>

      <!-- 投递前再过滤 -->
      <div class="f-group f-subblock">
        <span class="f-glabel">投递前再过滤</span>
        <span class="f-glabel-sub">这些条件只影响入库，不缩小搜索范围</span>
        <div class="f-gbody f-grid2">
          <label class="f-cell">
            <span class="f-cell-label">HR 活跃度</span>
            <span class="f-select-wrap">
              <select
                class="input f-select"
                :value="draft.hr_active"
                data-testid="filter-hr-active-select"
                @change="patch({ hr_active: ($event.target as HTMLSelectElement).value })"
              >
                <option v-for="o in HR_ACTIVE_OPTIONS" :key="o.value" :value="o.value" :title="o.hint">{{ o.name }}</option>
              </select>
              <svg class="f-select-caret" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                <path d="M4 6.4 8 10.4l4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
          </label>
          <label class="f-cell">
            <span class="f-cell-label">休息制度</span>
            <span class="f-select-wrap">
              <select
                class="input f-select"
                :value="draft.weekend"
                data-testid="filter-schedule-select"
                @change="patch({ weekend: ($event.target as HTMLSelectElement).value })"
              >
                <option v-for="o in WEEKEND_OPTIONS" :key="o.value" :value="o.value" :title="o.hint">{{ o.name }}</option>
              </select>
              <svg class="f-select-caret" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                <path d="M4 6.4 8 10.4l4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
          </label>
          <label class="f-cell">
            <span class="f-cell-label">公司成立年限（起）</span>
            <input
              class="input input-num"
              type="number"
              min="0"
              :value="draft.founded"
              data-testid="filter-founded-input"
              aria-label="公司成立年限"
              @input="patch({ founded: ($event.target as HTMLInputElement).value })"
            />
          </label>
          <label class="f-cell">
            <span class="f-cell-label">短期项目 / 项目制</span>
            <span class="f-select-wrap">
              <select
                class="input f-select"
                :value="draft.short_term"
                data-testid="filter-short-term-select"
                @change="patch({ short_term: ($event.target as HTMLSelectElement).value })"
              >
                <option value="on">排除</option>
                <option value="off">不排除</option>
              </select>
              <svg class="f-select-caret" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
                <path d="M4 6.4 8 10.4l4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
          </label>
        </div>
      </div>
    </div>

    <div class="f-foot">
      <span class="f-note" data-testid="filter-note">{{ note || '改动会自动保存；也可点「应用」立即生效' }}</span>
      <span class="spacer"></span>
      <button class="btn btn-sm btn-ghost" type="button" data-testid="filter-cancel-btn" @click="emit('cancel')">
        收起面板
      </button>
      <button class="btn btn-sm btn-primary" type="button" data-testid="filter-apply-btn" @click="emit('apply')">
        应用
      </button>
    </div>
  </section>
</template>
