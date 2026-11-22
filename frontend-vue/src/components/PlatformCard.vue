<!--
  顶栏三平台卡（M5 补遗）。

  骨架逐字照冻结原型 platform-*：
  platform-<id>-card / -check / -status / -login-btn，未接入的两个 is-off + 「未接入」。
  平台常量只从 lib_ui/options.ts 的 PLATFORMS 来（C15），显示口径在
  lib_ui/platform-model.ts（纯函数、有单测）。
-->
<script setup lang="ts">
import { computed } from 'vue';

import {
  loginDot,
  loginDotClass,
  loginDotLabel,
  platformCards,
  type LoginStatusLike,
} from '../lib_ui/platform-model';

const props = withDefaults(
  defineProps<{
    selected: string;
    login?: LoginStatusLike | null;
    loginBusy?: boolean;
  }>(),
  { login: null, loginBusy: false },
);

const emit = defineEmits<{
  (e: 'pick', id: string): void;
  (e: 'login', id: string): void;
}>();

const cards = computed(() => platformCards(props.selected, props.loginBusy));
const dot = computed(() => loginDot(props.login));
const dotClass = computed(() => loginDotClass(dot.value));
const dotLabel = computed(() => loginDotLabel(dot.value));
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
      <i :class="dotClass" :data-testid="p.statusTestId" :aria-label="dotLabel"></i>
      <button
        class="btn btn-sm btn-primary platform-login"
        :data-testid="p.loginTestId"
        :disabled="p.loginDisabled"
        :title="p.loginTitle"
        @click="emit('login', p.id)"
      >
        {{ p.loginLabel }}
      </button>
    </template>
    <span v-else class="platform-suffix">{{ p.mode === 'off' ? '未接入' : '未选中' }}</span>
  </span>
</template>
