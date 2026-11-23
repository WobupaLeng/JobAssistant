<script setup lang="ts">
/**
 * 唯一输入框（`.chat-detach-compose`）。全站只有这一处能打字 ——
 * Boss 列表底部不给发送框（SPEC §4.5 / A14）。
 *
 * Ctrl/⌘ + Enter 发送；回车换行（多行草稿是常态）。
 */
import { computed, ref } from 'vue';

import { CHAT_COMPOSE_PLACEHOLDER, CHAT_SEND_LABEL, canSend, isSendHotkey } from '../../lib_ui/chat-model';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    sending?: boolean;
  }>(),
  { sending: false },
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'send'): void;
}>();

const box = ref<HTMLTextAreaElement | null>(null);

const canSubmit = computed(() => canSend(props.modelValue, props.sending));

function onInput(ev: Event): void {
  emit('update:modelValue', (ev.target as HTMLTextAreaElement).value);
}

function onKeydown(ev: KeyboardEvent): void {
  if (!isSendHotkey(ev)) return;
  ev.preventDefault();
  submit();
}

function submit(): void {
  if (!canSubmit.value) return;
  emit('send');
}

function focus(): void {
  box.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div class="chat-detach-compose" data-testid="chat-detach-compose">
    <textarea
      ref="box"
      class="textarea"
      rows="1"
      :placeholder="CHAT_COMPOSE_PLACEHOLDER"
      :aria-label="CHAT_COMPOSE_PLACEHOLDER"
      data-testid="chat-detach-compose-input"
      :value="props.modelValue"
      @input="onInput"
      @keydown="onKeydown"
    />
    <button
      class="btn btn-sm btn-primary"
      type="button"
      data-testid="chat-detach-send-btn"
      :disabled="!canSubmit"
      @click="submit"
    >{{ props.sending ? '发送中…' : CHAT_SEND_LABEL }}</button>
  </div>
</template>
