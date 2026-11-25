<script setup lang="ts">
/**
 * 自动投递状态条（`auto-apply-bar`）。页面用 `v-if` 在「右栏」与「对话框头」两处二选一渲染，
 * 同一时刻只有一个实例，所以 `auto-greet-toggle-btn` 全站唯一。三态 idle / running（警示色）/
 * paused 全部来自后端，本地只有一个「按钮忙碌」瞬时标志。
 */
import { computed } from 'vue';

import type { AutoStatusReply } from '../../api/types';
import {
  autoPhase,
  barMode,
  stateClass,
  stateLabel,
  statusText,
  toggleClass,
  toggleLabel,
  togglePressed,
} from '../../lib_ui/auto-model';

const props = withDefaults(
  defineProps<{
    status: AutoStatusReply;
    sending?: boolean;
    sendingLabel?: string;
    busy?: boolean;
  }>(),
  { sending: false, sendingLabel: '', busy: false },
);

const emit = defineEmits<{
  (e: 'toggle'): void;
  (e: 'stop'): void;
}>();

const phase = computed(() => autoPhase(props.status, props.sending));
const mode = computed(() => barMode(phase.value));
const label = computed(() => stateLabel(phase.value));
const labelClass = computed(() => stateClass(phase.value));
const mainLabel = computed(() => toggleLabel(phase.value));
const mainClass = computed(() => toggleClass(phase.value));
const pressed = computed(() => togglePressed(phase.value));
const detail = computed(() => statusText(props.status, phase.value, props.sending, props.sendingLabel));
const showStop = computed(() => mode.value !== 'idle');
</script>

<template>
  <div
    class="auto-apply-bar"
    data-testid="auto-apply-bar"
    :data-mode="mode"
    :title="detail"
  >
    <span :class="labelClass" data-testid="auto-apply-state" aria-live="polite">{{ label }}</span>
    <!-- 弹性占位：按钮组因此贴住条的右内边，标签留在左边（原先靠 space-between，
         但 .auto-apply-acts 是 flex 0 0 auto 的内容宽盒，按钮实际跟着标签走）。 -->
    <span class="spacer" />
    <span class="auto-apply-acts">
      <button
        :class="mainClass"
        type="button"
        data-testid="auto-greet-toggle-btn"
        :aria-pressed="pressed ? 'true' : 'false'"
        :disabled="props.busy"
        @click="emit('toggle')"
      >{{ mainLabel }}</button>
      <button
        v-if="showStop"
        class="btn btn-sm btn-danger"
        type="button"
        data-testid="auto-greet-stop-btn"
        :disabled="props.busy"
        @click="emit('stop')"
      >结束</button>
    </span>
  </div>
</template>
