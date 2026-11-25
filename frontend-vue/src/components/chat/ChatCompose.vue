<script setup lang="ts">
/**
 * 唯一输入框（`.chat-detach-compose`），全站只有这一处能打字。Ctrl/⌘ + Enter 发送，
 * 回车换行。「AI 生成」按钮在发送按钮正上方：由后端拼提示词（Boss 回复上下文 + JD +
 * 我的技能）写一句落进输入框，用户还能改。组件只管发事件与禁用态。
 */
import { computed, ref } from 'vue';

import {
  CHAT_AI_BUSY_LABEL,
  CHAT_AI_LABEL,
  CHAT_COMPOSE_PLACEHOLDER,
  CHAT_SEND_LABEL,
  canSend,
  isSendHotkey,
} from '../../lib_ui/chat-model';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    sending?: boolean;
    /** 「AI 生成」进行中：按钮转「生成中…」并禁用，防连点叠加请求。 */
    generating?: boolean;
  }>(),
  { sending: false, generating: false },
);

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'send'): void;
  (e: 'generate'): void;
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

function generate(): void {
  if (props.generating) return;
  emit('generate');
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
    <!--
      「AI 生成」与「发送」同属右列、列内留一道细缝，整列与左侧输入框等高（各自 flex: 1 1 0
      平分）。生成按钮在发送按钮正上方，胶囊保留炫彩描边。
    -->
    <div class="chat-detach-actions">
      <button
        class="btn btn-ghost chat-detach-ai"
        type="button"
        data-testid="chat-detach-ai-btn"
        :disabled="props.generating"
        @click="generate"
      >{{ props.generating ? CHAT_AI_BUSY_LABEL : CHAT_AI_LABEL }}</button>
      <button
        class="btn btn-primary chat-detach-send"
        type="button"
        data-testid="chat-detach-send-btn"
        :disabled="!canSubmit"
        @click="submit"
      >{{ props.sending ? '发送中…' : CHAT_SEND_LABEL }}</button>
    </div>
  </div>
</template>
