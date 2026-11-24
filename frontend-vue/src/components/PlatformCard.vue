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
import {
  isLoggedIn,
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
  }>(),
  { login: null, loginBusy: false, actionBusy: false },
);

const emit = defineEmits<{
  (e: 'pick', id: string): void;
  (e: 'login', id: string): void;
  (e: 'save'): void;
  (e: 'logout'): void;
}>();

const cards = computed(() => platformCards(props.selected, props.loginBusy, loggedIn.value));
/** 已登录口径统一走 `login-model.isLoggedIn`（状态字符串唯一真源），不在这里重写比较。 */
const loggedIn = computed(() => isLoggedIn(props.login?.state));
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
  <!-- 打分方式分段已整体迁到顶栏 `TopbarTail`（`scrape-mode-segmented` 与
       初筛输入都在那边，紧跟登录按钮右侧）。本组件只剩平台选择与登录动作。 -->
</template>
