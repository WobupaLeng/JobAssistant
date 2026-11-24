<script setup lang="ts">
/**
 * 对话框外壳（`.chat-detach`）—— **只负责骨架、宽度与 Esc/收起**。
 *
 * 结构照 SPEC §2.1：头（标题/计数/副行/收起）→ 体（左段 Boss 列表 + 右段聊天区）。
 *
 * 宽度过渡：`flex-basis` 从 0 长到 `--detach-w` 在 `is-open` 切换时补间，规则在
 * `styles/vue-chat.css`（禁止 translateX，A15 要求 computed `position: static`）；
 * 收起态的 `[hidden]` 在 `styles/motion.css` 里被改成零宽盒而不是 `display: none`，
 * 否则同一帧 display 翻转会让那条过渡永远不跑（用户：聊天框展开没有过渡）。
 * 语义随之由 `inert` 承接：收起时整块不可聚焦、不进无障碍树。
 *
 * 自动投递条由页面决定放「对话框头」还是「右栏」，本组件留一个 head slot
 * —— 同一时刻只渲染一处，全站仍只有一个 `auto-greet-toggle-btn`。
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
          <span class="chip" data-testid="chat-detach-count">{{ props.count }}</span>
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
      </div>
      <slot />
    </div>

    <!-- 收起对话框的按钮挂在对话框根节点上，**不在 body 的裁切盒里**（用户 m04611）。
         放在 body 内部时，body 为了纵向滚动必须是 overflow-y: hidden，
         而「一轴 visible + 一轴 hidden」会被规范强制算成 auto，于是仍然裁切，
         悬停浮出的双 chevron 会被切掉一半。挂到根节点后只有根节点的
         overflow: hidden 生效，chevron 全程留在对话框内侧，不必跨出任何盒子。

         按钮盒恒为 22px（悬停也不变），所以 Boss 列表的宽度与位置一格不动；
         浮动的只是里面那枚绝对定位的 SVG。 -->
    <button
      class="chat-detach-collapse"
      type="button"
      data-testid="chat-detach-close-btn"
      title="Esc"
      aria-label="收起对话框"
      @click="emit('close')"
    >
      <!-- 悬停时从左边框长出一枚双 chevron「»」（用户 m04550 给的形状，m04575 选定 C1 渐变）。
           两笔各 10px 粗、linecap/linejoin 全 round，尖端是圆角不是尖角；
           左缘压在边框线上，读起来就是「从线框上分出去的一半」。
           纯图形不带字：无障碍靠按钮的 aria-label 与 title。 -->
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
