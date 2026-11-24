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
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
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
import { PAN_CLASS, playOpen, resetPanel } from '../lib_ui/pan';
import { hasHigherOverlay } from '../lib_ui/overlay-stack';

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
/** 面板节点：展开关键帧挂在它身上（与另三个抽屉同一套，见 lib_ui/pan.ts）。 */
const panel = ref<HTMLElement | null>(null);
/** 首次挂载那次加载是否已经发过：避免 onMounted 与 watch 各发一次。 */
let booted = false;

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
    // **加载必须落在挂载之后**：父级用 v-if 控制面板块，打开那一帧本组件还没挂，
    // 这里的 watch 回调拿不到节点、更拿不到数据；真正的首次加载由 onMounted 负责
    // （见下方 mounted 那次 emit）。这里只管「重新打开同一实例」时的重播与刷新。
    if (panel.value !== null) {
      resetPanel(panel.value);
      playOpen(panel.value, PAN_CLASS.DRAWER);
    }
    if (booted) emit('reload', kindOf(tab.value));
  },
);

/**
 * 关闭：**立刻**通知父级卸载面板，不排任何计时器。
 *
 * 用户口径（本轮）：「关闭有延时」——之前把 `emit('close')` 排在 320ms 收起回程之后，
 * 点下去到面板消失之间就多出一段空窗，期间遮罩还在、点击还被吃。
 * 遮罩点击 / 关闭按钮 / Esc 三条路走同一个函数，也不再需要防重入标记：
 * 卸载是父级一次同步改 state，重复调用无副作用。
 */
function close(): void {
  if (!props.open) return;
  emit('close');
}

/**
 * Esc 只在这一层最上面时生效（与岗位详情 / 配置 / 决策记录同一张分层表）。
 * 挂 window 冒泡阶段：排在那几个 `document` 捕获监听之后，一次按键只关一层。
 */
function onKeydown(ev: KeyboardEvent): void {
  if (ev.key !== 'Escape' || !props.open) return;
  if (hasHigherOverlay('sensitive')) return;
  ev.preventDefault();
  close();
}

onMounted(() => {
  window.addEventListener('keydown', onKeydown);
  // 面板区块由父级 v-if 控制：能挂载就一定是要打开的那一次。
  // 首次数据加载发生在这里（watch 在挂载前不会触发，见上面那段注释）。
  booted = true;
  resetPanel(panel.value);
  playOpen(panel.value, PAN_CLASS.DRAWER);
  emit('reload', kindOf(tab.value));
});
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));

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
    <div v-if="props.open" class="sensitive-backdrop" data-testid="sensitive-backdrop" @click="close"></div>
    <aside
      ref="panel"
      class="sensitive-panel glass-l2 hl-surface"
      :class="{ 'is-open': props.open }"
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
        <button class="btn btn-sm btn-ghost" type="button" data-testid="sensitive-close-btn" @click="close">
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
            class="sensitive-card hl-surface"
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
