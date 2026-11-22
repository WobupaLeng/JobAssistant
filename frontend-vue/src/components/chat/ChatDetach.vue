<script setup lang="ts">
/**
 * 对话框外壳（`.chat-detach`）—— **只负责骨架、宽度与 Esc/收起**。
 *
 * 结构照 SPEC §2.1：头（标题/计数/副行/收起）→ 体（左段 Boss 列表 + 右段聊天区）。
 *
 * 宽度不做过渡动画：`flex-basis` 从 0 长到 `--detach-w` 只在 `.is-chat-open`
 * 这个类切换时发生，规则在 `styles/vue-chat.css`（禁止 translateX，A15 要求
 * computed `position: static`）。
 *
 * 自动投递条由页面决定放「对话框头」还是「右栏」，本组件留一个 head slot
 * —— 同一时刻只渲染一处，全站仍只有一个 `auto-greet-toggle-btn`。
 */
import type { Friend } from '../../api/types';
import { CHAT_CLOSE_LABEL, CHAT_DETACH_RAIL_TITLE, CHAT_DETACH_TITLE, headSub } from '../../lib_ui/chat-model';

const props = withDefaults(
  defineProps<{
    open: boolean;
    friend: Friend | null;
    count: number;
  }>(),
  { friend: null },
);

const emit = defineEmits<{ (e: 'close'): void }>();
</script>

<template>
  <div
    class="chat-detach"
    :class="{ 'is-open': props.open }"
    data-testid="chat-detach"
    role="dialog"
    aria-label="Boss 消息（对话框）"
    :aria-hidden="props.open ? 'false' : 'true'"
    :hidden="!props.open"
  >
    <div class="chat-detach-head">
      <span class="card-title" data-testid="chat-detach-title">
        {{ CHAT_DETACH_TITLE }}
        <span class="chip" data-testid="chat-detach-count">{{ props.count }}</span>
      </span>
      <span class="chat-detach-sub" data-testid="chat-detach-sub">{{ headSub(props.friend) }}</span>
      <slot name="head" />
      <span class="spacer" />
      <button
        class="btn btn-sm btn-ghost"
        type="button"
        data-testid="chat-detach-close-btn"
        title="Esc"
        @click="emit('close')"
      >{{ CHAT_CLOSE_LABEL }}</button>
    </div>

    <div class="chat-detach-body" data-testid="chat-detach-body">
      <div class="chat-detach-rail" data-testid="chat-detach-rail">
        <span class="sr-only">{{ CHAT_DETACH_RAIL_TITLE }}</span>
        <slot name="rail" />
      </div>
      <slot />
    </div>
  </div>
</template>
