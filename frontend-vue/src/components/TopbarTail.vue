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
import { computed } from 'vue';
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
  }>(),
  {
    llmPrefilter: LLM_PREFILTER_DEFAULT,
    tokensTotal: 0,
    tokensGreet: 0,
    tokensScore: 0,
    sensitiveOpen: false,
    themeLabel: '深色',
  },
);

const emit = defineEmits<{
  (e: 'pick-mode', key: 'rule' | 'llm'): void;
  (e: 'update:prefilter', value: number): void;
  (e: 'toggle-sensitive'): void;
  (e: 'open-config'): void;
  (e: 'toggle-theme'): void;
}>();

/**
 * 大模型打分被选中时才露初筛输入框——规则打分下它没有意义，
 * 常驻会把顶栏撑乱（SPEC §11.3 的落位顺序不变）。
 */
const llmActive = computed(() => props.segs.some((s) => s.key === 'llm' && s.active));

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
    <!-- 打分方式分段已移到 PlatformCard（紧贴 Boss 登录按钮之后）。 -->
    <!-- 初筛线已移到 PlatformCard（紧贴大模型打分之后）。 -->
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      :aria-pressed="sensitiveOpen"
      data-testid="sensitive-open"
      @click="emit('toggle-sensitive')"
    >
      敏感库
    </button>
    <!-- 原型 L1400-1406：同一枚读数两种 DOM，CSS 在 1360 断点处互换显隐（A32 各唯一 1 份）。
         分项（模型打分/打招呼）与分隔线住 .tb-tokens 内，1360–1599 随原型 L1335-1337 隐藏；
         折叠 chip 的 title 给全量三值（原型 L1406 逐字格式）。数据来自既有 config DTO，
         Rust 核心暂无累计写入路径，读数为旧库遗留值，前端不造数（见 HANDOFF 偏差②更正）。 -->
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
    <button
      class="btn btn-sm btn-ghost"
      type="button"
      data-testid="theme-toggle"
      :title="`切换明暗主题（当前${themeLabel}）`"
      @click="emit('toggle-theme')"
    >
      {{ themeLabel === '深色' ? '☾ 深色' : '☀ 浅色' }}
    </button>
  </div>
</template>
