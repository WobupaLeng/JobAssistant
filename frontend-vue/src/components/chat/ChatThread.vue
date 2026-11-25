<script setup lang="ts">
/**
 * 聊天区（对话头 + 消息流 + 简历条 + 打招呼草稿）。行为口径见 `lib_ui/chat-model`；
 * 标题 / 计数在 `.chat-detach-head`（由页面渲染），本组件只出对话头、流与输入框之上。
 * 打招呼草稿是后端生成的一整句，可直接发（`greet_one`）或放进输入框再改。
 */
import { computed, onBeforeUpdate, ref } from 'vue';

import type { ChatMessage, Friend, GreetReply } from '../../api/types';
import {
  CHAT_DETACH_EMPTY_SUB,
  CHAT_DETACH_EMPTY_TITLE,
  CHAT_STATE_ANNOUNCE_TEXT,
  RECALL_TEXT,
  TYPING_TEXT,
  msgKey,
  msgTestIdOrLocal,
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
    /** 当前会话存在未撤回的 HR 消息：「生成回复」按钮据此显隐。 */
    hasHrReply?: boolean;
  }>(),
  {
    loadingMessages: false,
    greet: null,
    generating: false,
    sendingGreet: false,
    recalling: false,
    hasHrReply: false,
  },
);

const emit = defineEmits<{
  (e: 'generate-greet'): void;
  (e: 'generate-reply'): void;
  (e: 'recall'): void;
  (e: 'send-greet'): void;
  (e: 'greet-to-draft'): void;
  (e: 'discard-greet'): void;
}>();

// `props.messages` 可能缺失（调用方没传 / 测试直挂组件）：默认成空数组，
// 否则 `splitThread(undefined)` 抛错被 Vue 记为渲染期未捕获错误。
const split = computed(() => splitThread(props.messages ?? []));

/* 消息流重建时只播一次入场动画：气泡恒带 `animation` 的话，远程结果整体替换或 key 错位
 * 会让重建的气泡全部重播，造成可感知的卡顿。只在节点新建那一帧挂 `is-entering`，下一帧
 * 摘掉；class 直接改 DOM，不进响应式状态（每帧变更会触发重算）。 */
const rootEl = ref<HTMLElement | null>(null);
let enteredOnce = false;

onBeforeUpdate(() => {
  if (enteredOnce) return;
  enteredOnce = true;
  const el = rootEl.value;
  if (el === null) return;
  el.classList.add('is-entering');
  requestAnimationFrame(() => el.classList.remove('is-entering'));
});

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
  <div ref="rootEl" class="chat-detach-chat">
    <div v-if="props.friend === null" class="rail-body-slot" data-testid="chat-detach-slot">
      <div class="rail-state" data-testid="chat-detach-empty">
        <!-- 40×40 圆形玻璃盒 + 一枚气泡 SVG（不是 emoji）。 -->
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
              :key="msgKey(m, i)"
              :class="bubbleClass(m.sender)"
              :data-testid="msgTestIdOrLocal(m, i)"
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
          <!-- 消息流末尾的说明条：住在滚动区里、紧接最后一条气泡之后。放在 v-else 模板**之外**，
               否则空会话里「已是最新消息」会整条消失。样式随语气走，见 .detach-hint。 -->
          <slot name="footer" />
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
          <!-- 重新生成：同一份入参再跑一次（模型给不同措辞）。 -->
          <button
            class="btn btn-sm btn-ghost"
            type="button"
            data-testid="chat-greet-regen"
            :disabled="props.generating"
            @click="emit('generate-greet')"
          >{{ props.generating ? '生成中…' : '重新生成' }}</button>
          <!-- 生成回复：只有对方说过话才出现（没有可接的话就没意义）。 -->
          <button
            v-if="props.hasHrReply"
            class="btn btn-sm btn-ghost"
            type="button"
            data-testid="chat-greet-reply"
            :disabled="props.generating"
            @click="emit('generate-reply')"
          >生成回复</button>
          <button class="btn btn-sm btn-ghost" type="button" data-testid="chat-greet-discard" @click="emit('discard-greet')">
            丢弃
          </button>
        </div>
        <div class="greet-text" data-testid="chat-greet-text">{{ props.greet.text }}</div>
        <div v-if="props.greet.note !== ''" class="cfg-hint">{{ props.greet.note }}</div>
        <div class="greet-hint" data-testid="chat-greet-source">
          {{ props.greet.reply_used ? '依据：HR 回复 + 我的技能' : '依据：岗位信息 + 我的技能' }}<template
            v-if="props.greet.jd_used"
          >（已用完整 JD）</template><template v-else>（未拿到 JD，只用公司名与岗位名）</template>
        </div>
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
