<script setup lang="ts">
/**
 * 顶栏尾段：计分模式分段 / 敏感库入口 / token 读数 / 配置 / 主题。
 *
 * 为什么单独一个组件：`AppShell` 必须保持「不 import 页面、不含业务」，
 * 而这一段要读 `get_scoring_spec` 与 token 统计。放在 `components/` 并只
 * 通过 props/emit 通信，数据由持有它的 `DashboardPage` 喂进来（C13 插槽原则）。
 *
 * 落位（SPEC-001 决策 7 / AC10）：
 * 顶栏是一行，本组件由概览页 teleport 进 `AppShell` 的 `.tb-embed`，
 * 顺序按冻结 SPEC §11.3：`scrape-mode-segmented` → `sensitive-open` → token → 配置。
 * 尾段再分两组（用户 m04039）：`.tb-tools`（分段 · token · 配置 · 敏感库，紧贴登录按钮）
 * 与 `.tb-theme`（主题，独占右端）。
 *
 * 两处口径别改回去：
 * - **满分 / 门槛 chip 不在这里**（legacy-baseline §160「满分口径 `.score-spec` 已迁出顶栏」）——
 *   它挂在 `.card.log-standalone` 卡头（`LogCard`），锚点 `score-spec` 全站恰好一处；
 * - **配置按钮是 `shell-config-btn`**（原型 L1407 / SPEC §11.3 / 断言 A31），
 *   二级导航时代那个 `nav-config` 已随页签退役。
 * - **token 读数两份 DOM**（原型 L1400-1406，断言 A32）：≥1360 走 `.tb-tokens`
 *   （锚点 `token-total`），<1360 折叠成一枚 chip（锚点 `token-total-chip`，类名
 *   `.tb-token-chip`——本仓库 `.tb-stats` 已被日志卡的 C9 进度块占用）。两者各唯一一份，
 *   读数文案都是「累计 N token」，折叠 chip 的 title 给全量读数。
 * - **分项读数**（原型 L1402-1403，Chief 派单接线）：模型打分/打招呼两枚 `rail-stat`
 *   与分隔线住在 `.tb-tokens` 内，1360–1599 随原型隐藏；数据来自既有 config DTO
 *   （tokens_greet/tokens_score），零新增 invoke。Rust 核心暂无累计写入路径，
 *   读数为旧库遗留值（见 HANDOFF 偏差②更正），前端不造数。
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
    paletteId: 'emerald',
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
  (e: 'toggle-theme'): void;
  (e: 'pick-theme-mode', mode: 'dark' | 'light'): void;
  (e: 'toggle-theme-panel', next: boolean): void;
}>();

/**
 * 大模型打分被选中时才露初筛输入框——规则打分下它没有意义，
 * 常驻会把顶栏撑乱（SPEC §11.3 的落位顺序不变）。
 */
const llmActive = computed(() => props.segs.some((s) => s.key === 'llm' && s.active));

/* ---------------- 主题面板（调色板 + 字号）
 *
 * 用户裁决：主题 / 调色板 / 字号三组收成一枚「主题」按钮，点开才展开。
 *
 * 面板必须 Teleport 到 body：这一行住在 `header.topbar`，顶栏带 backdrop-filter
 * （会新建包含块），面板若留在原地会卡在顶栏这条 46px 的带子里被裁掉。
 * 定位与 `CityPicker` 同一套口径：量触发按钮矩形，下面放得下就向下展，否则翻上。
 * 面板根节点带 `data-overlay="popover"`，概览页的全局 pointerdown 会据此放行。
 */
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
 * 面板开合：由页面通过 `themePanelOpen` 持有真值，本组件只发意图。
 * 面板挂在 `<body>` 上（Teleport），按钮留在顶栏「配置 · 敏感库」之后，
 * 两者不在同一棵子树上，页面用一份状态统一管住更不容易走形。
 */
const themeOpen = computed(() => props.themePanelOpen);
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
  emit('toggle-theme-panel', next);
  if (!next) return;
  placePanel();
  void nextTick(() => panelEl.value?.querySelector<HTMLElement>('button')?.focus());
}

function closeThemePanel(): void {
  if (!themeOpen.value) return;
  emit('toggle-theme-panel', false);
  triggerEl.value?.focus();
}

function onOutsidePointer(ev: PointerEvent): void {
  if (!themeOpen.value) return;
  const target = ev.target;
  if (!(target instanceof Node)) return;
  if (triggerEl.value?.contains(target)) return;
  if (panelEl.value?.contains(target)) return;
  emit('toggle-theme-panel', false);
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
    <!-- 左半段 .tb-tools：打分分段 + token 读数 + 配置 + 敏感库，紧贴登录按钮。
         用户 m04039：这组「与主题分开，靠近登录按钮」。主题单独落在右端 .tb-theme。 -->
    <div class="tb-tools">
    <!-- 打分方式分段 + 初筛线（用户裁决：移到登录按钮右边）。
         登录动作住在 `PlatformCard`（同一行 `.tb-embed` 内），本组件紧随其后，
         所以 DOM 顺序上恰好就是「登录按钮 → 规则/大模型分段 → 初筛线」。
         初筛线只在选中大模型时出现：规则打分下它没有意义，常驻会把顶栏撑乱。 -->
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
    <!-- 原型 L1400-1406：同一枚读数两种 DOM，CSS 在窄窗断点处互换显隐（A32 各唯一 1 份）。
         分项（模型打分/打招呼）与分隔线住 .tb-tokens 内，窄窗才折叠；
         折叠 chip 的 title 给全量三值（原型 L1406 逐字格式）。数据来自既有 config DTO
         的 tokens_* 三键，由后端 add_tokens 累加。 -->
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
    <!-- 敏感库紧跟在「配置」右侧（用户 m02692）。它原先排在打分分段之后，
         现在与配置同属「贴登录按钮」这一组，顺序：… 分段 · token · 配置 · 敏感库。 -->
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
    <!-- 主题按钮独占尾段右端 .tb-theme（用户 m04039：主题位置保持不变）。
         与左边那组之间留一条 auto 边距，两组各占一头；按钮与面板是同一个
         `<body>` 外的两层宿主，页面用 `themePanelOpen` 统管开合。 -->
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
        class="theme-panel"
        data-testid="theme-panel"
        data-overlay="popover"
        role="group"
        aria-label="主题设置"
        :style="{ top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px` }"
      >
        <div class="theme-row">
          <span class="theme-row-label">明暗</span>
          <button
            class="btn btn-sm btn-ghost theme-mode"
            type="button"
            :title="`切换明暗主题（当前${themeLabel}）`"
            data-testid="theme-mode-btn"
            @click="emit('toggle-theme')"
          >
            {{ themeLabel === '深色' ? '☾ 深色' : '☀ 浅色' }}
          </button>
          <!-- 深/浅两档各自一个按钮：比单个「翻转」按钮少一次歧义（用户可能在面板里连点），
               也让「明暗」这一行与下面的主题色、字号两行视觉对齐。 -->
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
