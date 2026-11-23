<script setup lang="ts">
/**
 * 敏感库抽屉：已入库 / 已排除的明细，支持搜索、多选、批量删除。
 *
 * 视觉与类名沿用冻结的 `styles/sensitive.css`（`.sensitive-backdrop` /
 * `.sensitive-panel.is-open` 已经在里面），本组件不新增布局规则。
 *
 * 抽屉不是路由页：C13 的页面分发是插槽，这里的开合由顶栏按钮控制，
 * 因此用 `position: fixed` 覆盖层而不是切页（原型同款）。
 */
import { computed, ref, watch } from 'vue';
import type { SensitivePage, SensitiveRecord } from '../api/types';
import {
  SENSITIVE_TABS,
  allChecked,
  countLabel,
  deleteLabel,
  removeIds,
  visibleRecords,
  type SensitiveTab,
} from '../lib_ui/sensitive-model';
import GlassClone from './GlassClone.vue';

const props = withDefaults(
  defineProps<{
    open?: boolean;
    page?: SensitivePage;
    loading?: boolean;
    error?: string;
  }>(),
  { open: false, page: () => ({ items: [], total: 0 }), loading: false, error: '' },
);

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'reload', kind: 'all' | 'stored' | 'rejected'): void;
  (e: 'remove', ids: number[]): void;
}>();

const tab = ref<SensitiveTab>('all');
const keyword = ref('');
const checked = ref<number[]>([]);
const appliedKeyword = ref('');

const visible = computed(() => visibleRecords(props.page.items, tab.value, appliedKeyword.value));
const allOn = computed(() => allChecked(checked.value, visible.value));
const delLabel = computed(() => deleteLabel(checked.value.length));
const total = computed(() => countLabel(props.page.total));

function kindOf(t: SensitiveTab): 'all' | 'stored' | 'rejected' {
  return SENSITIVE_TABS.find((x) => x.key === t)?.kind ?? 'all';
}

watch(tab, (t) => {
  checked.value = [];
  emit('reload', kindOf(t));
});

watch(
  () => props.open,
  (open) => {
    if (!open) {
      checked.value = [];
      return;
    }
    checked.value = [];
    emit('reload', kindOf(tab.value));
  },
);

function search(): void {
  appliedKeyword.value = keyword.value;
  checked.value = [];
}

function reset(): void {
  keyword.value = '';
  appliedKeyword.value = '';
  tab.value = 'all';
  checked.value = [];
  emit('reload', 'all');
}

function toggle(id: number): void {
  checked.value = checked.value.includes(id) ? checked.value.filter((x) => x !== id) : [...checked.value, id];
}

function toggleAll(): void {
  checked.value = allOn.value ? [] : visible.value.map((r) => r.id);
}

function removeOne(record: SensitiveRecord): void {
  emit('remove', [record.id]);
  checked.value = checked.value.filter((x) => x !== record.id);
}

function removeChecked(): void {
  const ids = checked.value.slice();
  if (ids.length === 0) return;
  emit('remove', ids);
  checked.value = [];
}

defineExpose({
  /** 供父级在删除成功后同步本地列表（省一次全量重拉，保住滚动位置）。 */
  applyRemoved(ids: number[]): void {
    props.page.items = removeIds(props.page.items, ids);
  },
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="sensitive-backdrop" data-testid="sensitive-backdrop" @click="emit('close')"></div>
    <aside
      class="sensitive-panel glass-l2"
      :class="{ 'is-open': open }"
      data-testid="sensitive-panel"
      :aria-hidden="!open"
      aria-label="敏感库"
    >
      <!-- L2-2 P0：面板背后窗内内容（.dashboard）的非交互折射副本（裁定 6），开合时建 / 拆。 -->
      <GlassClone mode="backdrop" :active="open" />
      <div class="sensitive-head">
        <div class="sensitive-head-text">
          <span class="sensitive-title">敏感库</span>
          <span class="sensitive-sub" data-testid="sensitive-count">{{ total }}</span>
        </div>
        <span class="spacer"></span>
        <button class="btn btn-sm btn-ghost" type="button" data-testid="sensitive-close-btn" @click="emit('close')">
          关闭
        </button>
      </div>

      <div class="sensitive-tools">
        <div class="sensitive-search">
          <input
            v-model="keyword"
            class="input"
            type="search"
            placeholder="公司 / 岗位 / 城市 / 原因"
            data-testid="sensitive-search-input"
            aria-label="搜索敏感记录"
            @keydown.enter.prevent="search"
          />
          <button class="btn btn-sm btn-ghost" type="button" data-testid="sensitive-search-btn" @click="search">
            搜索
          </button>
          <button class="btn btn-sm btn-ghost" type="button" data-testid="sensitive-reset-btn" @click="reset">
            重置
          </button>
        </div>
        <div class="sensitive-filter">
          <button
            v-for="t in SENSITIVE_TABS"
            :key="t.key"
            class="chip as-btn"
            :class="{ 'is-on': tab === t.key }"
            type="button"
            :aria-pressed="tab === t.key"
            :data-testid="`sensitive-tab-${t.key}-tab`"
            @click="tab = t.key"
          >
            {{ t.label }}
          </button>
        </div>
        <div class="sensitive-tools-actions">
          <button class="btn btn-sm btn-ghost" type="button" data-testid="sensitive-selectall-btn" @click="toggleAll">
            {{ allOn ? '取消全选' : '全选' }}
          </button>
          <button
            class="btn btn-sm btn-danger"
            type="button"
            :disabled="checked.length === 0"
            data-testid="sensitive-delete-btn"
            @click="removeChecked"
          >
            {{ delLabel }}
          </button>
        </div>
      </div>

      <p v-if="error" class="sensitive-error" data-testid="sensitive-error">{{ error }}</p>

      <div class="sensitive-body">
        <p v-if="loading" class="sensitive-loading" data-testid="sensitive-loading">正在加载…</p>
        <p v-else-if="visible.length === 0" class="sensitive-loading" data-testid="sensitive-empty">没有匹配的记录。</p>
        <div v-else class="sensitive-grid" data-testid="sensitive-grid">
          <article
            v-for="r in visible"
            :key="r.id"
            class="sensitive-card"
            :data-testid="`sensitive-card-${r.id}`"
          >
            <input
              class="sensitive-check"
              type="checkbox"
              :checked="checked.includes(r.id)"
              :data-testid="`sensitive-check-${r.id}`"
              :aria-label="`勾选 ${r.company ?? r.title ?? r.id}`"
              @change="toggle(r.id)"
            />
            <div class="sensitive-card-row">
              <span class="sensitive-company">{{ r.company ?? '—' }}</span>
              <span class="sensitive-chip-status" :class="{ 'is-rejected': r.kind === 'rejected' }">
                {{ r.kind === 'rejected' ? '已排除' : '已入库' }}
              </span>
            </div>
            <div class="sensitive-card-row">
              <span class="sensitive-job" :class="{ 'is-rejected': r.kind === 'rejected' }">{{ r.title ?? '—' }}</span>
              <span class="sensitive-salary">{{ r.salary ?? '面议' }}</span>
            </div>
            <p class="sensitive-note" :class="{ 'is-rejected': r.kind === 'rejected' }">{{ r.reason ?? '—' }}</p>
            <div class="sensitive-card-foot">
              <span class="sensitive-time">{{ r.location ?? '—' }}</span>
              <span class="spacer"></span>
              <button
                class="btn btn-sm btn-ghost"
                type="button"
                :data-testid="`sensitive-remove-${r.id}`"
                @click="removeOne(r)"
              >
                删除
              </button>
            </div>
          </article>
        </div>
      </div>
    </aside>
  </Teleport>
</template>
