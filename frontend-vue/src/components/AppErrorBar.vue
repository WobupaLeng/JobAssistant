<script setup lang="ts">
/**
 * 页面级错误条（原型 `.app-error`）。
 *
 * 为什么不是 `styles/controls.css` 里那个 `.error-bar`：这一条横在整页最上面，
 * 带错误码与重试按钮，视觉层级更高。10 秒自动消失（Leptos `shell.rs` 的既有行为，
 * 见 ARCH-003 §1.3），悬停暂停计时——否则用户刚看清就没了。
 */
import { onBeforeUnmount, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    /** 空串表示不显示。 */
    message?: string;
    /** 可选的错误码/命令名，便于对日志。 */
    code?: string;
    /** 自动消失毫秒数；`0` 表示常驻。 */
    autoDismissMs?: number;
  }>(),
  { message: '', code: '', autoDismissMs: 10_000 },
);

const emit = defineEmits<{
  (e: 'retry'): void;
  (e: 'close'): void;
}>();

let timer: ReturnType<typeof setTimeout> | null = null;

function clear(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
}

function arm(): void {
  clear();
  if (props.autoDismissMs <= 0 || props.message === '') return;
  timer = setTimeout(() => emit('close'), props.autoDismissMs);
}

watch(() => props.message, arm, { immediate: true });

onBeforeUnmount(clear);
</script>

<template>
  <div
    v-if="message"
    class="app-error"
    role="alert"
    data-testid="app-error-bar"
    @mouseenter="clear"
    @mouseleave="arm"
  >
    <span class="app-error-ico" aria-hidden="true">!</span>
    <span class="app-error-text">
      <code v-if="code" data-testid="app-error-code">{{ code }}</code>
      <span>{{ message }}</span>
    </span>
    <span class="spacer"></span>
    <button class="btn btn-sm btn-ghost" type="button" data-testid="app-error-retry-btn" @click="emit('retry')">
      重试
    </button>
    <button class="btn btn-sm btn-ghost" type="button" data-testid="app-error-close-btn" @click="emit('close')">
      忽略
    </button>
  </div>
</template>
