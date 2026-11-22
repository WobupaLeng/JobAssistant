<script setup lang="ts">
/**
 * 聊天区（对话头 + 消息流 + 简历条 + 打招呼草稿）。
 *
 * 这是 Leptos `features/chat/thread.rs` 的行为等价物，**但 DOM 口径按冻结原型 v3**：
 * 标题/计数在 `.chat-detach-head`（由页面渲染），本组件只出「对话头 + 流 + 输入框之上」。
 *
 * 打招呼草稿：`greet_text` 是后端生成的一整句，用户可以直接发（`greet_one`），
 * 也可以「放到输入框再改」。生成/丢弃都只动本地状态。
 */
import { computed } from 'vue';

import type { ChatMessage, Friend, GreetReply, ResumeItem } from '../../api/types';
import {
  CHAT_DETACH_EMPTY_SUB,
  CHAT_DETACH_EMPTY_TITLE,
  RECALL_TEXT,
  TYPING_TEXT,
  headSub,
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
    resumes?: readonly ResumeItem[];
    resumesOpen?: boolean;
    resumesLoaded?: boolean;
    greet?: GreetReply | null;
    generating?: boolean;
    sendingGreet?: boolean;
    recalling?: boolean;
  }>(),
  {
    loadingMessages: false,
    resumes: () => [],
    resumesOpen: false,
    resumesLoaded: false,
    greet: null,
    generating: false,
    sendingGreet: false,
    recalling: false,
  },
);

const emit = defineEmits<{
  (e: 'generate-greet'): void;
  (e: 'toggle-resumes'): void;
  (e: 'send-resume', resumeId: string): void;
  (e: 'recall'): void;
  (e: 'send-greet'): void;
  (e: 'greet-to-draft'): void;
  (e: 'discard-greet'): void;
}>();

const split = computed(() => splitThread(props.messages));

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

function confirmSendResume(resume: ResumeItem): void {
  const peer = props.friend === null ? '' : props.friend.name || props.friend.company;
  const ok = window.confirm(`确定把简历「${resume.name}」发给「${peer}」？发出去之后无法收回。`);
  if (ok) emit('send-resume', resume.resume_id);
}

function confirmRecall(): void {
  if (window.confirm('确定撤回最近一条自己发的消息？')) emit('recall');
}
</script>

<template>
  <div class="chat-detach-chat">
    <div v-if="props.friend === null" class="rail-body-slot" data-testid="chat-detach-slot">
      <div class="rail-state" data-testid="chat-detach-empty">
        <span class="state-glyph">💬</span>
        <span class="state-title" data-testid="chat-detach-empty-title">{{ CHAT_DETACH_EMPTY_TITLE }}</span>
        <span class="state-sub">{{ CHAT_DETACH_EMPTY_SUB }}</span>
      </div>
    </div>

    <template v-else>
      <div class="chat-head">
        <div class="chat-head-text">
          <span class="chat-head-name" data-testid="chat-head-name">
            {{ props.friend.name || props.friend.uid }}
          </span>
          <span class="chat-head-sub" data-testid="chat-head-sub">{{ headSub(props.friend) }}</span>
        </div>
        <div class="chat-head-actions">
          <button
            class="btn btn-sm btn-ghost"
            type="button"
            data-testid="chat-greet-btn"
            :disabled="props.generating"
            @click="emit('generate-greet')"
          >{{ props.generating ? '生成中…' : '生成打招呼' }}</button>
          <button
            class="btn btn-sm btn-ghost"
            type="button"
            data-testid="chat-resume-btn"
            :aria-expanded="props.resumesOpen ? 'true' : 'false'"
            @click="emit('toggle-resumes')"
          >选简历</button>
          <button
            class="btn btn-sm btn-ghost"
            type="button"
            data-testid="chat-recall-btn"
            :disabled="props.recalling"
            @click="confirmRecall"
          >{{ props.recalling ? '撤回中…' : '撤回上一条' }}</button>
        </div>
      </div>

      <div v-if="props.resumesOpen" class="resume-bar" data-testid="chat-resume-bar">
        <span class="section-label">简历</span>
        <button
          v-for="r in props.resumes"
          :key="r.resume_id"
          class="chip chip-accent as-btn"
          type="button"
          :data-testid="`chat-resume-${r.resume_id}`"
          @click="confirmSendResume(r)"
        >{{ r.name }}</button>
        <span v-if="props.resumesLoaded && props.resumes.length === 0" class="cfg-hint" data-testid="chat-resume-empty">
          没读到简历列表。确认 Boss 账号里已上传附件简历。
        </span>
        <span class="spacer" />
        <button class="btn btn-sm btn-ghost" type="button" data-testid="chat-resume-close" @click="emit('toggle-resumes')">
          收起
        </button>
      </div>

      <div class="rail-body-slot" data-testid="chat-detach-slot">
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
        <span class="sr-only" data-testid="chat-detach-state-announce" aria-live="polite">聊天区</span>
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
