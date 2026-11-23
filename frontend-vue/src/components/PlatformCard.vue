<!--
  顶栏三平台卡（M5 补遗）。

  骨架逐字照冻结原型 platform-*：
  platform-<id>-card / -check / -status / -login-btn，未接入的两个 is-off + 「未接入」。
  平台常量只从 lib_ui/options.ts 的 PLATFORMS 来（C15），显示口径在
  lib_ui/platform-model.ts 与 lib_ui/login-model.ts（纯函数、有单测）。
  点亮「登录」后按后端 login_status 追加：platform-login-save-btn / platform-logout-btn，
  以及 resolve-scan-flag（二维码，仅 waiting 态显示）。
-->
<script setup lang="ts">
import { computed } from 'vue';

import { platformCards, type LoginStatusLike } from '../lib_ui/platform-model';
import { clampPrefilter, LLM_PREFILTER_DEFAULT, LLM_PREFILTER_MAX } from '../lib_ui/scrape-panel-model';
import {
  loginBadge,
  logoutAction,
  saveLoginAction,
  scanFlagVisible,
} from '../lib_ui/login-model';
import type { LoginStatus } from '../api/types';

const props = withDefaults(
  defineProps<{
    selected: string;
    login?: LoginStatus | null;
    loginBusy?: boolean;
    /** 最近一次「保存登录 / 退出」还没回来 */
    actionBusy?: boolean;
    /** 打分方式分段（规则 / 大模型），紧贴 Boss 登录按钮之后渲染。 */
    segs?: ReadonlyArray<{ key: 'llm' | 'rule'; label: string; active: boolean; disabled: boolean; testId: string }>;
    /** 初筛线（规则分低于该值不调大模型）。 */
    llmPrefilter?: number;
  }>(),
  { login: null, loginBusy: false, actionBusy: false, segs: () => [], llmPrefilter: LLM_PREFILTER_DEFAULT },
);

const emit = defineEmits<{
  (e: 'pick', id: string): void;
  (e: 'login', id: string): void;
  (e: 'save'): void;
  (e: 'logout'): void;
  (e: 'pick-mode', key: 'llm' | 'rule'): void;
  (e: 'update:prefilter', value: number): void;
}>();

/** 大模型打分选中时才露初筛输入框（规则打分下它没有意义）。 */
const llmActive = computed(() => props.segs.some((s) => s.key === 'llm' && s.active));

function onPrefilterInput(ev: Event): void {
  const el = ev.target as HTMLInputElement | null;
  if (!el) return;
  const next = clampPrefilter(el.value);
  // 被夹过的值要写回输入框，否则用户看到「200」但库里是 100
  el.value = String(next);
  if (next !== props.llmPrefilter) emit('update:prefilter', next);
}

const cards = computed(() => platformCards(props.selected, props.loginBusy));
const badge = computed(() => loginBadge(props.login as LoginStatus | null));
const save = computed(() => saveLoginAction(props.login as LoginStatus | null, props.actionBusy));
const out = computed(() => logoutAction(props.login as LoginStatus | null, props.actionBusy));
const scanning = computed(() => scanFlagVisible(props.login?.state));
/** 二维码载荷：后端把 data URI 放在 `login_status.message` 里，没有就不画。 */
const scanSrc = computed(() => {
  const msg = (props.login?.message ?? '').trim();
  return props.login !== null && msg.startsWith('data:image/') ? msg : '';
});
</script>

<template>
  <span
    v-for="p in cards"
    :key="p.id"
    :class="p.cardClass"
    :data-testid="p.testId"
    :title="p.mode === 'off' ? p.title : undefined"
    :data-mode="p.mode"
  >
    <input
      class="check"
      type="checkbox"
      :checked="p.checked"
      :disabled="p.mode === 'off'"
      :aria-label="p.name"
      :data-testid="p.checkTestId"
      @change="emit('pick', p.id)"
    />
    <span class="platform-name">{{ p.name }}</span>
    <template v-if="p.loginVisible">
      <i :class="badge.dotClass" :data-testid="p.statusTestId" :aria-label="badge.dotLabel" :title="badge.title"></i>
      <button
        class="btn btn-sm btn-primary platform-login"
        :data-testid="p.loginTestId"
        :disabled="p.loginDisabled"
        :title="p.loginTitle"
        @click="emit('login', p.id)"
      >
        {{ p.loginLabel }}
      </button>
      <button
        v-if="save.visible"
        :class="save.btnClass"
        :data-testid="save.testId"
        :disabled="save.disabled"
        :title="save.title"
        @click="emit('save')"
      >
        {{ save.label }}
      </button>
      <button
        v-if="out.visible"
        :class="out.btnClass"
        :data-testid="out.testId"
        :disabled="out.disabled"
        :title="out.title"
        @click="emit('logout')"
      >
        {{ out.label }}
      </button>
      <img
        v-if="scanning && scanSrc !== ''"
        class="platform-scan"
        :src="scanSrc"
        alt="登录二维码"
        data-testid="resolve-scan-flag"
      />
    </template>
    <span v-else class="platform-suffix">{{ p.mode === 'off' ? '未接入' : '未选中' }}</span>
  </span>
  <div v-if="segs.length > 0" class="segmented scoring-after-login" role="radiogroup" aria-label="打分方式" data-testid="scrape-mode-segmented">
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
</template>
