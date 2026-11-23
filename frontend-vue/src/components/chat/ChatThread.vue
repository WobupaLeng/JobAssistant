<script setup lang="ts">
/**
 * 聊天区（对话头 + 消息流 + 简历条 + 打招呼草稿）。
 *
 * 这是 Leptos 版聊天线程的行为等价物（Leptos 版已退役，见 git 历史，提交 53d05cb 之前），**但 DOM 口径按冻结原型 v3**：
 * 标题/计数在 `.chat-detach-head`（由页面渲染），本组件只出「对话头 + 流 + 输入框之上」。
 *
 * 打招呼草稿：`greet_text` 是后端生成的一整句，用户可以直接发（`greet_one`），
 * 也可以「放到输入框再改」。生成/丢弃都只动本地状态。
 */
import { computed } from 'vue';

import type { ChatMessage, Friend, GreetReply } from '../../api/types';
import {
  CHAT_DETACH_EMPTY_SUB,
  CHAT_DETACH_EMPTY_TITLE,
  CHAT_STATE_ANNOUNCE_TEXT,
  RECALL_TEXT,
  TYPING_TEXT,
  msgTestId,
  senderSlug,
  splitThread,
  type SenderSlug,
} from '../../lib_ui/chat-model';

const props = withDefaults(
  defineProps<{
    friend: Friend | null;
    messages: readonly ChatMessage[];
    loadingMessages?: boolean;
    greet?: GreetReply | null;
    generating?: boolean;
    sendingGreet?: boolean;
    recalling?: boolean;
  }>(),
  {
    loadingMessages: false,
    greet: null,
    generating: false,
    sendingGreet: false,
    recalling: false,
  },
);

const emit = defineEmits<{
  (e: 'generate-greet'): void;
  (e: 'recall'): void;
  (e: 'send-greet'): void;
  (e: 'greet-to-draft'): void;
  (e: 'discard-greet'): void;
}>();

const split = computed(() => splitThread(props.messages));

/* `peerName` / `subText` 随 `chat-head` 整行一起移除：这条头行已不再渲染，
   留在脚本里的计算属性只会变成没人读的死代码。Boss 列表用
   `chat-model.friendName` / `friendLine` 取名与副行。 */

function bubbleClass(sender: string): string {
  const slug: SenderSlug | string = senderSlug(sender);
  return `bubble-row is-${slug}`;
}

function greetTone(greet: GreetReply): string {
  const src = String(greet.source ?? '').trim();
  return src === 'llm' || src === 'model' ? 'chip chip-primary' : 'chip chip-warn';
}

function greetChip(greet: GreetReply): string {
  const src = String(greet.source ?? '').trim();
  const isModel = src === 'llm' || src === 'model';
  if (!isModel) return '规则话术';
  return greet.tokens > 0 ? `大模型 · ${greet.tokens} tokens` : '大模型';
}


</script>

<template>
  <div class="chat-detach-chat">
    <div v-if="props.friend === null" class="rail-body-slot" data-testid="chat-detach-slot">
      <div class="rail-state" data-testid="chat-detach-empty">
        <!-- 原型 L2422：40×40 圆形玻璃盒 + 同一枚气泡 SVG（不是 emoji）。 -->
        <span class="state-glyph is-round" aria-hidden="true"><svg width="20" height="20" viewBox="0 0 16 16" fill="none"><path d="M2.6 4.2h10.8v6.4H7.4L4.2 13v-2.4H2.6z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/></svg></span>
        <span class="state-title" data-testid="chat-detach-empty-title">{{ CHAT_DETACH_EMPTY_TITLE }}</span>
        <span class="state-sub">{{ CHAT_DETACH_EMPTY_SUB }}</span>
      </div>
    </div>

    <template v-else>
      <!-- 对话头整行已移除（用户裁决）：HR 名与「生成打招呼 / 选简历」原本挤在
           这一条里，公司/岗位字段一缺就只剩两个按钮悬在中间。名字已在展开态的
           左上标题行（`.chat-detach-head`）显示，两个按钮下沉到 Boss 列表的
           每一行（`FriendList.vue`），那里才有「对谁打招呼、给谁发简历」的上下文。 -->

      <div class="rail-body-slot" data-testid="chat-detach-slot">
        <!-- 顶部提示条（发送失败 / 未登录 / 未连接等）：由页面经 notices 插槽提供，贴在消息流最上方。 -->
        <slot name="notices" />
        <div class="chat-feed rail-body" data-testid="chat-detach-thread">
          <div v-if="props.loadingMessages && split.bubbles.length === 0" class="rail-state">
            <span class="skel-line" />
            <span class="skel-line" />
            <span class="skel-line" />
          </div>
          <div v-else-if="split.bubbles.length === 0 && !split.typing" class="empty">
            还没有消息记录
          </div>
          <template v-else>
            <div
              v-for="(m, i) in split.bubbles"
              :key="msgTestId(m) ?? `local-${i}`"
              :class="bubbleClass(m.sender)"
              :data-testid="msgTestId(m) ?? `chat-msg-local-${i}`"
            >
              <div class="bubble">
                <span v-if="m.recalled" class="bubble-recall">{{ RECALL_TEXT }}</span>
                <span v-else class="bubble-text">{{ m.content }}</span>
                <span class="bubble-time">{{ m.time || '—' }}</span>
              </div>
            </div>
            <div v-if="split.typing" class="bubble-row is-system" data-testid="chat-typing">
              <div class="bubble"><span class="bubble-typing">{{ TYPING_TEXT }}</span></div>
            </div>
          </template>
        </div>
        <span class="sr-only" data-testid="chat-detach-state-announce" aria-live="polite">{{
          CHAT_STATE_ANNOUNCE_TEXT
        }}</span>
      </div>

      <div v-if="props.greet !== null" class="greet-box" data-testid="chat-greet-box">
        <div class="greet-head">
          <span class="section-label">打招呼草稿</span>
          <span :class="greetTone(props.greet)">{{ greetChip(props.greet) }}</span>
          <span class="spacer" />
          <button class="btn btn-sm btn-ghost" type="button" data-testid="chat-greet-discard" @click="emit('discard-greet')">
            丢弃
          </button>
        </div>
        <div class="greet-text" data-testid="chat-greet-text">{{ props.greet.text }}</div>
        <div v-if="props.greet.note !== ''" class="cfg-hint">{{ props.greet.note }}</div>
        <div class="greet-actions">
          <button
            class="btn btn-sm btn-primary"
            type="button"
            data-testid="chat-greet-send"
            :disabled="props.sendingGreet"
            @click="emit('send-greet')"
          >{{ props.sendingGreet ? '发送中…' : '发送这句' }}</button>
          <button class="btn btn-sm btn-ghost" type="button" data-testid="chat-greet-todraft" @click="emit('greet-to-draft')">
            放到输入框再改
          </button>
        </div>
      </div>

      <!-- 唯一输入框：由页面用 ChatCompose 插到这里（slot），
           这样「输入框」与「消息流」共用同一列的 flex 高度链。 -->
      <slot name="compose" />
    </template>
  </div>
</template>
