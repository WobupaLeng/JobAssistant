<script setup lang="ts">
/**
 * 顶栏尾段：计分模式分段 / 敏感库入口 / token 读数 / 配置 / 主题。单独成组件是为了让
 * `AppShell` 不含业务，数据由 `DashboardPage` 经 props/emit 喂进来。满分 / 门槛 chip 在日志卡头
 * 而不在顶栏；token 读数有两份 DOM（≥1360 走 `.tb-tokens`，窄屏折叠成一枚 chip）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import {
  FONT_SCALES,
  PALETTES,
  type FontScaleId,
  type PaletteDef,
  type PaletteId,
} from '../lib_ui/palette';
import {
  clampPrefilter,
  LLM_PREFILTER_DEFAULT,
  LLM_PREFILTER_MAX,
  tokenText,
  type ScoreModeSeg,
} from '../lib_ui/scrape-panel-model';

const props = withDefaults(
  defineProps<{
    segs: readonly ScoreModeSeg[];
    llmPrefilter?: number;
    tokensTotal?: number;
    tokensGreet?: number;
    tokensScore?: number;
    sensitiveOpen?: boolean;
    themeLabel?: string;
    paletteId?: PaletteId;
    palettes?: readonly PaletteDef[];
    fontScaleId?: FontScaleId;
    fontScales?: readonly { id: FontScaleId; label: string }[];
    themePanelOpen?: boolean;
  }>(),
  {
    llmPrefilter: LLM_PREFILTER_DEFAULT,
    tokensTotal: 0,
    tokensGreet: 0,
    tokensScore: 0,
    sensitiveOpen: false,
    themeLabel: '深色',
    paletteId: 'cobalt',
    palettes: () => PALETTES,
    fontScaleId: 'm',
    fontScales: () => FONT_SCALES,
    themePanelOpen: false,
  },
);

const emit = defineEmits<{
  (e: 'pick-palette', id: PaletteId): void;
  (e: 'pick-font-scale', id: FontScaleId): void;
  (e: 'pick-mode', key: 'rule' | 'llm'): void;
  (e: 'update:prefilter', value: number): void;
  (e: 'toggle-sensitive'): void;
  (e: 'open-config'): void;
  (e: 'pick-theme-mode', mode: 'dark' | 'light'): void;
  (e: 'toggle-theme-panel', next: boolean): void;
}>();

/**
 * 大模型打分被选中时才露初筛输入框——规则打分下它没有意义，
 * 常驻会把顶栏撑乱（落位顺序不变）。
 */
const llmActive = computed(() => props.segs.some((s) => s.key === 'llm' && s.active));

/* ---------------- 主题面板（调色板 + 字号）
 * 面板 teleport 到 body：顶栏只有 46px 高，留在原地会被这条带子裁掉。
 * 开合状态归本组件自己（面板挂在 body 上，跟按钮不在同一棵子树）。根节点带
 * `data-overlay="popover"` 供全局 pointerdown 放行。 */
interface PanelBox {
  top: number;
  left: number;
  width: number;
}

const PANEL_MIN_WIDTH = 232;
const VIEW_GAP = 8;

/** 明暗两档（顺序即渲染顺序）。 */
const MODES: readonly { id: 'dark' | 'light'; label: string }[] = [
  { id: 'dark', label: '☾ 深色' },
  { id: 'light', label: '☀ 浅色' },
];

/**
 * 面板开合：本组件唯一持有（不再经父组件往返）。
 * 按钮在顶栏、面板 teleport 到 `<body>`，两者虽不同子树，但开关是纯 UI 状态，
 * 放在同组件里最不容易走形：点开 / 点外部 / Esc 三条路都只改这一处。
 */
const themeOpen = ref(props.themePanelOpen);
const triggerEl = ref<HTMLButtonElement | null>(null);
const panelEl = ref<HTMLElement | null>(null);
const box = ref<PanelBox | null>(null);

/** 量触发按钮，把面板钉在它右下方（右对齐，避免顶到窗口右缘）。 */
function placePanel(): void {
  const anchor = triggerEl.value;
  if (!anchor) return;
  const r = anchor.getBoundingClientRect();
  const viewW = window.innerWidth;
  const viewH = window.innerHeight;
  const width = Math.min(PANEL_MIN_WIDTH, Math.max(160, viewW - VIEW_GAP * 2));
  // 右缘对齐按钮右缘，再夹进视口
  const left = Math.min(Math.max(VIEW_GAP, r.right - width), Math.max(VIEW_GAP, viewW - width - VIEW_GAP));
  const below = viewH - r.bottom - VIEW_GAP * 2;
  box.value = { top: below < 140 ? r.top - VIEW_GAP : r.bottom + VIEW_GAP, left, width };
}

function toggleThemePanel(): void {
  const next = !themeOpen.value;
  themeOpen.value = next;
  emit('toggle-theme-panel', next);
  if (!next) return;
  // 先量再渲染：box 为空时面板不会渲染（模板里 `v-if="themeOpen && box"`），
  // 所以 placePanel 必须早于下一次渲染，否则首帧无处定位。
  placePanel();
  void nextTick(() => {
    if (box.value === null) placePanel();
    panelEl.value?.querySelector<HTMLElement>('button')?.focus();
  });
}

function closeThemePanel(): void {
  if (!themeOpen.value) return;
  themeOpen.value = false;
  emit('toggle-theme-panel', false);
  triggerEl.value?.focus();
}

function onOutsidePointer(ev: PointerEvent): void {
  if (!themeOpen.value) return;
  const target = ev.target;
  if (!(target instanceof Node)) return;
  if (triggerEl.value?.contains(target)) return;
  if (panelEl.value?.contains(target)) return;
  closeThemePanel();
}

function onDocumentKeydown(ev: KeyboardEvent): void {
  if (!themeOpen.value) return;
  if (ev.key !== 'Escape') return;
  // 只关这一层：模态 / 抽屉的 Esc 由各自组件负责
  ev.stopPropagation();
  closeThemePanel();
}

watch(themeOpen, (open) => {
  if (!open) return;
  placePanel();
});

// 父组件若清了 `themePanelOpen`（例如切页），本地跟着收，避免两处不一致。
watch(
  () => props.themePanelOpen,
  (next) => {
    if (!next && themeOpen.value) themeOpen.value = false;
  },
);

if (typeof document !== 'undefined') {
  document.addEventListener('pointerdown', onOutsidePointer, true);
  document.addEventListener('keydown', onDocumentKeydown, true);
  window.addEventListener('resize', placePanel);
  window.addEventListener('scroll', placePanel, true);
}

onBeforeUnmount(() => {
  if (typeof document === 'undefined') return;
  document.removeEventListener('pointerdown', onOutsidePointer, true);
  document.removeEventListener('keydown', onDocumentKeydown, true);
  window.removeEventListener('resize', placePanel);
  window.removeEventListener('scroll', placePanel, true);
});

function onPrefilterInput(ev: Event): void {
  const el = ev.target as HTMLInputElement | null;
  if (!el) return;
  const next = clampPrefilter(el.value);
  // 被夹过的值要写回输入框，否则用户看到「200」但库里是 100
  el.value = String(next);
  if (next !== props.llmPrefilter) emit('update:prefilter', next);
}
</script>

<template>
  <div class="tb-tail">
    <!-- 左半段 .tb-tools：打分分段 + token 读数 + 配置 + 敏感库，紧贴登录按钮。 -->
    <div class="tb-tools">
    <!-- 打分方式分段 + 初筛线：登录动作在 `PlatformCard`，本组件紧随其后。
         初筛线只在选中大模型时出现，规则打分下没有意义。 -->
    <div
      v-if="segs.length > 0"
      class="segmented scoring-after-login"
      role="radiogroup"
      aria-label="打分方式"
      data-testid="scrape-mode-segmented"
    >
      <button
        v-for="s in segs"
        :key="s.key"
        class="seg"
        :class="{ 'is-active': s.active }"
        type="button"
        role="radio"
        :aria-checked="s.active"
        :disabled="s.disabled"
        :aria-disabled="s.disabled"
        :title="s.key === 'llm' ? '用大模型给岗位打分（需先在配置里填 API Key）' : undefined"
        :data-testid="s.testId"
        @click="!s.disabled && emit('pick-mode', s.key)"
      >
        {{ s.label }}
      </button>
      <label
        v-if="llmActive"
        class="seg-prefilter"
        title="规则分低于该值的岗位不调用大模型打分"
        data-testid="scrape-llm-prefilter-wrap"
      >
        <span class="seg-prefilter-text">规则分 ≥</span>
        <input
          class="seg-prefilter-input"
          type="number"
          min="0"
          :max="LLM_PREFILTER_MAX"
          step="1"
          :value="llmPrefilter"
          inputmode="numeric"
          aria-label="初筛分数线：规则分低于该值不调用大模型打分"
          data-testid="scrape-llm-prefilter"
          @change="onPrefilterInput"
        />
        <span class="seg-prefilter-text">才走大模型</span>
      </label>
    </div>
    <!-- 同一枚读数两种 DOM，CSS 在窄窗断点互换显隐，各唯一 1 份。
         数据来自 config DTO 的 tokens_* 三键，由后端累加。 -->
    <span class="tb-tokens" :title="`累计 ${tokenText(tokensTotal)} token`">
      <span>累计 <b class="token-total" data-testid="token-total">{{ tokenText(tokensTotal) }}</b> token</span>
      <i class="tb-sep" aria-hidden="true"></i>
      <span class="rail-stat">模型打分 <b>{{ tokenText(tokensScore) }}</b></span>
      <span class="rail-stat">打招呼 <b>{{ tokenText(tokensGreet) }}</b></span>
    </span>
    <span
      class="chip tb-token-chip"
      tabindex="0"
      data-testid="token-total-chip"
      :title="`累计 ${tokenText(tokensTotal)} token · 模型打分 ${tokenText(tokensScore)} · 打招呼 ${tokenText(tokensGreet)}`"
    >累计 <b>{{ tokenText(tokensTotal) }}</b> token</span>
    <button class="btn btn-sm btn-ghost" type="button" data-testid="shell-config-btn" @click="emit('open-config')">
      配置
    </button>
    <!-- 敏感库紧跟「配置」右侧，同属「贴登录按钮」这一组。 -->
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      :aria-pressed="sensitiveOpen"
      data-testid="sensitive-open"
      @click="emit('toggle-sensitive')"
    >
      敏感库
    </button>
    </div>
    <!-- 主题按钮独占尾段右端 .tb-theme，与左边那组各占一头。 -->
    <div class="tb-theme">
    <button
      ref="triggerEl"
      class="btn btn-sm btn-ghost theme-btn"
      type="button"
      data-testid="theme-toggle"
      aria-haspopup="dialog"
      :aria-expanded="themeOpen ? 'true' : 'false'"
      aria-controls="theme-panel"
      title="主题：明暗 / 主题色 / 字号"
      @click="toggleThemePanel"
    >
      <span aria-hidden="true">{{ themeLabel === '深色' ? '☾' : '☀' }}</span>
      <span class="theme-btn-label">主题</span>
    </button>
    </div>
    <Teleport to="body">
      <div
        v-if="themeOpen && box"
        id="theme-panel"
        ref="panelEl"
        class="theme-panel pan-pop"
        data-testid="theme-panel"
        data-overlay="popover"
        role="group"
        aria-label="主题设置"
        :style="{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px` }"
      >
        <div class="theme-row">
          <span class="theme-row-label">明暗</span>
          <!-- 只有深 / 浅两枚按钮：翻转能力由这两枚并列覆盖，不要再加一枚「翻转」。 -->
          <div class="theme-modes" data-testid="theme-mode-group">
            <button
              v-for="m in MODES"
              :key="m.id"
              type="button"
              class="btn btn-sm btn-ghost theme-mode-btn"
              :class="{ 'is-active': (themeLabel === '深色' ? 'dark' : 'light') === m.id }"
              :data-testid="`theme-mode-${m.id}`"
              :aria-pressed="(themeLabel === '深色' ? 'dark' : 'light') === m.id"
              @click="emit('pick-theme-mode', m.id)"
            >{{ m.label }}</button>
          </div>
        </div>
        <div class="theme-row">
          <span class="theme-row-label">主题色</span>
          <div class="palette-picker" data-testid="palette-picker">
            <button
              v-for="p in palettes"
              :key="p.id"
              type="button"
              class="palette-dot"
              :class="{ 'is-active': p.id === paletteId }"
              :data-testid="`palette-${p.id}`"
              :title="`主题色：${p.label}`"
              :aria-label="`主题色 ${p.label}`"
              :aria-pressed="p.id === paletteId"
              :style="{ '--dot': p.swatch }"
              @click="emit('pick-palette', p.id)"
            />
          </div>
        </div>
        <div class="theme-row">
          <span class="theme-row-label">字号</span>
          <div class="font-scale-picker" data-testid="font-scale-picker">
            <button
              v-for="f in fontScales"
              :key="f.id"
              type="button"
              class="btn btn-sm btn-ghost font-scale-btn"
              :class="{ 'is-active': f.id === fontScaleId }"
              :data-testid="`font-scale-${f.id}`"
              :aria-pressed="f.id === fontScaleId"
              @click="emit('pick-font-scale', f.id)"
            >{{ f.label }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
