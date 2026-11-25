<script setup lang="ts">
/**
 * 对话框外壳（`.chat-detach`）：只负责骨架、宽度与 Esc/收起。宽度过渡靠 `is-open` 切换时补间
 * `flex-basis`（禁止 translateX）。收起态的 `[hidden]` 在 `styles/vue-chat.css` 里是零宽盒而不是
 * `display: none`，否则同一帧 display 翻转会让过渡永远不跑，语义由 `inert` 承接。
 */
import type { Friend } from '../../api/types';
import { CHAT_DETACH_RAIL_TITLE, CHAT_DETACH_TITLE, headSub } from '../../lib_ui/chat-model';

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
    class="chat-detach hl-surface"
    :class="{ 'is-open': props.open }"
    data-testid="chat-detach"
    role="dialog"
    aria-label="Boss 消息（对话框）"
    :aria-hidden="props.open ? 'false' : 'true'"
    :hidden="!props.open"
    :inert="!props.open"
  >
    <div class="chat-detach-head" data-testid="chat-detach-head">
      <!-- 两行：第一行是标题行（标题 / 计数 / 副行 / 自动投递按钮），
           第二行是自动投递条的计数（待投递 / 已投递，role="head" 插槽）。
           计数原先挤在标题行里，和「开始自动投递」抢同一行的宽度。 -->
      <div class="chat-detach-head-row">
        <span class="card-title" data-testid="chat-detach-title">
          {{ CHAT_DETACH_TITLE }}
        </span>
        <span class="chat-detach-sub" data-testid="chat-detach-sub">{{ headSub(props.friend) }}</span>
        <slot name="head-sub" />
        <slot name="head" />
      </div>
    </div>

    <div class="chat-detach-body" data-testid="chat-detach-body">
      <div class="chat-detach-rail" data-testid="chat-detach-rail">
        <span class="sr-only">{{ CHAT_DETACH_RAIL_TITLE }}</span>
        <slot name="rail" />
        <button
          v-if="props.open"
          class="chat-detach-rail-collapse"
          type="button"
          data-testid="chat-detach-rail-collapse"
          aria-label="收起对话框"
          @click="emit('close')"
        >
          收起<span class="chat-detach-rail-collapse-chev" aria-hidden="true">»</span>
        </button>
      </div>
      <slot />
    </div>

    <!-- 收起按钮常驻在对话框根节点上，**不能放进 body 的裁切盒**：body 为纵向滚动必须
         overflow-y: hidden，而「一轴 visible + 一轴 hidden」会被规范强制算成 auto 仍会裁切，
         悬停浮出的双 chevron 会被切掉一半。按钮盒恒为 22px，浮动的是那枚绝对定位的 SVG。 -->
    <button
      class="chat-detach-collapse"
      type="button"
      data-testid="chat-detach-close-btn"
      title="Esc"
      aria-label="收起对话框"
      @click="emit('close')"
    >
      <!-- 悬停时从左边框长出一枚双 chevron「»」。两笔各 10px 粗、linecap/linejoin 全 round，
           左缘压在边框线上；纯图形不带字，无障碍靠按钮的 aria-label 与 title。 -->
      <svg
        class="chat-detach-chevrons"
        viewBox="0 0 46 52"
        aria-hidden="true"
        focusable="false"
      >
        <defs>
          <linearGradient id="chat-chev-grad" x1="0" y1="0" x2="1" y2="1">
            <stop class="chev-a" offset="0" />
            <stop class="chev-b" offset="1" />
          </linearGradient>
        </defs>
        <path d="M5 5 L23 26 L5 47" stroke="url(#chat-chev-grad)" />
        <path d="M23 5 L41 26 L23 47" stroke="url(#chat-chev-grad)" />
      </svg>
    </button>
  </div>
</template>
