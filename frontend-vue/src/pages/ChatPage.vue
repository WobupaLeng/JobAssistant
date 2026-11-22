<script setup lang="ts">
/**
 * Boss 消息页（M2）。
 *
 * ## 口径
 * 结构与 testid 的唯一真源是冻结原型 `prototypes/ui-v1` + `docs/ui/UI-SPEC-chat-detach-v3.md`
 * （v3–v10 全部裁决），行为规则来自 Leptos 版 `features/chat/*.rs`。
 *
 * - 右栏 `aside.chat-rail` 常驻；对话框 `.chat-detach` 默认 `hidden`；
 * - 点任意 Boss ⇒ 展开（宽 `clamp(600px, 50vw, 80vw)`，下限 600），右栏让位；
 * - `chat-detach-close-btn` / `Esc` 收起并**保留** activeFriend，再点同一位重开；
 * - 全站唯一输入框在对话框聊天区底部；Boss 列表底部不设发送框。
 *
 * ## 数据
 * 好友列表 / 消息 / 简历 / 期望职位都走 `src/api/commands/*`，本页不假设任何后端形状。
 * 自动投递条的循环在这里起停（`features/auto/queue.ts`）：每秒问一次后端状态，
 * 到点才发一条；WebView 挂起时循环自然停（C16）。
 *
 * ## 独立 mount
 * 本页不依赖 `App.vue`：`nav` 只是给外壳用的可选项（`page-chat` 锚点由外壳给），
 * 测试可以直接 `mount(ChatPage)`。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { autoGreetPause, autoGreetReset, autoGreetResume } from '../api/commands/auto';
import {
  chatFriends,
  chatMessages,
  chatRecall,
  chatResumes,
  chatSend,
  chatSendResume,
} from '../api/commands/chat';
import { greetDraft, greetOne } from '../api/commands/greet';
import { sessionOpen } from '../api/commands/scrape';
import type { ChatMessage, Expectation, Friend, GreetReply, ResumeItem } from '../api/types';
import type { Nav } from '../lib_ui/nav';
import AutoApplyBar from '../components/chat/AutoApplyBar.vue';
import ChatCompose from '../components/chat/ChatCompose.vue';
import ChatDetach from '../components/chat/ChatDetach.vue';
import ChatThread from '../components/chat/ChatThread.vue';
import ExpectRow from '../components/chat/ExpectRow.vue';
import FriendList from '../components/chat/FriendList.vue';
import {
  emptyStatus,
  toggleAction,
  type AutoPhase,
} from '../lib_ui/auto-model';
import { startAutoQueue, type AutoQueue } from '../features/auto/queue';
import {
  CHAT_RAIL_TITLE,
  FRIENDS_EMPTY_NOTE,
  FRIENDS_FAIL_PREFIX,
  GREET_SENT_NOTE,
  MESSAGES_FAIL_PREFIX,
  RECALL_DONE_NOTE,
  RESUME_SENT_NOTE,
  SEND_FAIL_PREFIX,
  SESSION_BOOTING_NOTE,
  canSend,
  chatAreaWidth,
  closeDetach,
  createChatUi,
  detachWidth,
  filterFriends,
  isSendableFriend,
  matchFriendByCompany,
  nowText,
  pendingFriend,
  pickFriend,
  shouldRetryWithSession,
  withPending,
} from '../lib_ui/chat-model';
import { peekGreetDraft, takeGreetDraft } from '../features/chat/greet-draft';

const props = withDefaults(
  defineProps<{
    /** 外壳当前页（本页只在 `chat` 时被渲染；留作断言/调试用）。 */
    activeNav?: string;
  }>(),
  { activeNav: 'chat' },
);

const emit = defineEmits<{
  (e: 'navigate', nav: Nav): void;
  /** 展开态上抛给 App.vue → AppShell 的 `.is-chat-open`（右栏让位 / 列数 −1 的 CSS 钩子）。 */
  (e: 'chat-open', open: boolean): void;
}>();

/* ---------------------------------- 会话列表 ---------------------------------- */

const friends = ref<Friend[]>([]);
const loadingFriends = ref(true);
const friendsError = ref('');
const search = ref('');

/* ---------------------------------- 展开态 ---------------------------------- */

const ui = ref(createChatUi());

/* ---------------------------------- 线程 ------------------------------------ */

const messages = ref<ChatMessage[]>([]);
const loadingMessages = ref(false);
const note = ref('');

/* ---------------------------------- 简历 ------------------------------------ */

const resumes = ref<ResumeItem[]>([]);
const resumesOpen = ref(false);
const resumesLoaded = ref(false);

/* -------------------------------- 打招呼草稿 --------------------------------- */

const greet = ref<GreetReply | null>(null);
const generating = ref(false);
const sendingGreet = ref(false);
const recalling = ref(false);
const sending = ref(false);
const draft = ref('');

/* --------------------------------- 求职期望 ---------------------------------- */

const expectations = ref<Expectation[]>([]);
const expectLoading = ref(false);
const expectError = ref('');

/* -------------------------------- 自动投递 ----------------------------------- */

const autoStatus = ref(emptyStatus());
const autoSending = ref(false);
const autoBusy = ref(false);
const autoNote = ref('');
let queue: AutoQueue | null = null;

/* ------------------------------ 窗口宽度（宽度公式） --------------------------
 *
 * --detach-w / --rail-w 的写入已经统一收口到 AppShell 的 syncCanvas()（canvas.ts 是唯一
 * 公式来源，并且保证 --ui-scale 先于这两个变量落盘）。所以本页**不再**写行内样式：
 * 同一份样式表里 `.chat-detach.is-open { flex-basis: var(--detach-w) }` 直接吃行内值，
 * 静态兜底 720px 留在 vue-chat.css，首帧不会塌成 0 宽。
 *
 * 这里只保留一份只读的镜像给单测与调试用（defineExpose 出去），数值来自同一个函数，
 * 所以不会和 syncCanvas 打架。 */

const innerWidth = ref(typeof window === 'undefined' ? 1440 : window.innerWidth);
const detachW = computed(() => detachWidth(innerWidth.value));
const chatW = computed(() => chatAreaWidth(innerWidth.value));

/* ---------------------------------- 派生 ------------------------------------ */

const activeFriend = computed<Friend | null>(
  () => friends.value.find((f) => f.uid === ui.value.activeFriendsUid) ?? null,
);
const visibleFriends = computed(() => filterFriends(friends.value, search.value));
const talkableFriend = computed(() => activeFriend.value);
const canTalk = computed(() => talkableFriend.value !== null && isSendableFriend(talkableFriend.value));
const composeRef = ref<InstanceType<typeof ChatCompose> | null>(null);

/* ---------------------------------- 加载 ------------------------------------ */

async function loadFriends(): Promise<void> {
  loadingFriends.value = true;
  friendsError.value = '';
  try {
    let reply;
    try {
      reply = await chatFriends();
    } catch (e) {
      const text = errText(e);
      if (!shouldRetryWithSession(text)) throw e;
      note.value = SESSION_BOOTING_NOTE;
      await sessionOpen('boss', 'chat');
      reply = await chatFriends();
    }
    friends.value = reply.items;
    if (reply.items.length === 0) note.value = FRIENDS_EMPTY_NOTE;
    // 刷新不得改变展开态与选中（SPEC §3）：只把 activeFriend 的字段换成新的。
    const uid = ui.value.activeFriendsUid;
    if (uid !== null && !friends.value.some((f) => f.uid === uid)) {
      ui.value = closeDetach(ui.value);
    }
  } catch (e) {
    friendsError.value = errText(e);
    note.value = FRIENDS_FAIL_PREFIX + errText(e);
  } finally {
    loadingFriends.value = false;
  }
}

async function loadMessages(uid: string): Promise<void> {
  loadingMessages.value = true;
  try {
    const reply = await chatMessages(uid);
    if (ui.value.activeFriendsUid !== uid) return;
    messages.value = reply.items;
  } catch (e) {
    if (ui.value.activeFriendsUid !== uid) return;
    messages.value = [];
    note.value = MESSAGES_FAIL_PREFIX + errText(e);
  } finally {
    loadingMessages.value = false;
  }
}

async function loadResumes(force = false): Promise<void> {
  if (resumesLoaded.value && !force) return;
  try {
    const reply = await chatResumes();
    resumes.value = reply.items;
    resumesLoaded.value = true;
  } catch (e) {
    resumes.value = [];
    resumesLoaded.value = true;
    note.value = `读取简历失败：${errText(e)}`;
  }
}

async function loadExpectations(): Promise<void> {
  expectLoading.value = true;
  expectError.value = '';
  try {
    const { listExpectations } = await import('../api/commands/chat');
    expectations.value = await listExpectations();
  } catch (e) {
    expectError.value = `读取期望职位失败：${errText(e)}`;
  } finally {
    expectLoading.value = false;
  }
}

/* ---------------------------------- 选中 ------------------------------------ */

async function pick(uid: string): Promise<void> {
  const switched = ui.value.activeFriendsUid !== uid;
  ui.value = pickFriend(ui.value, uid);
  const friend = friends.value.find((f) => f.uid === uid) ?? null;
  if (friend !== null && friend.unread > 0) friend.unread = 0;
  if (switched) {
    messages.value = [];
    greet.value = null;
    draft.value = '';
    resumesOpen.value = false;
    note.value = '';
    await Promise.all([loadMessages(uid), loadResumes()]);
  }
}

/** 收起：保留 activeFriend（A7）；再点同一位也能重开（A8）。 */
function close(): void {
  ui.value = closeDetach(ui.value);
}

/* ---------------------------------- 发送 ------------------------------------ */

async function send(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || !canSend(draft.value, sending.value)) return;
  const text = draft.value.trim();
  sending.value = true;
  try {
    const reply = await chatSend(friend.uid, text, null);
    const outgoing = String(reply.text ?? '') !== '' ? String(reply.text) : text;
    messages.value = [
      ...messages.value,
      { sender: 'me', content: outgoing, time: nowText(), msg_id: '', recalled: false },
    ];
    draft.value = '';
    note.value = '消息已发送';
  } catch (e) {
    note.value = SEND_FAIL_PREFIX + errText(e);
  } finally {
    sending.value = false;
  }
}

async function sendGreet(): Promise<void> {
  const friend = talkableFriend.value;
  const g = greet.value;
  if (friend === null || g === null) return;
  const text = g.text.trim();
  if (text === '') return;
  sendingGreet.value = true;
  try {
    await greetOne(friend.uid, text);
    messages.value = [
      ...messages.value,
      { sender: 'me', content: text, time: nowText(), msg_id: '', recalled: false },
    ];
    greet.value = null;
    note.value = GREET_SENT_NOTE;
  } catch (e) {
    note.value = `发送失败：${errText(e)}`;
  } finally {
    sendingGreet.value = false;
  }
}

async function generateGreet(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || generating.value) return;
  generating.value = true;
  try {
    greet.value = await greetDraft({
      jobId: '',
      title: friend.title,
      company: friend.company,
      salary: '',
      experience: '',
      jd: '',
    });
  } catch (e) {
    note.value = `生成打招呼失败：${errText(e)}`;
  } finally {
    generating.value = false;
  }
}

async function recall(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || recalling.value) return;
  recalling.value = true;
  try {
    await chatRecall(friend.uid);
    note.value = RECALL_DONE_NOTE;
    await loadMessages(friend.uid);
  } catch (e) {
    note.value = `撤回失败：${errText(e)}`;
  } finally {
    recalling.value = false;
  }
}

async function sendResume(resumeId: string): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null) return;
  try {
    await chatSendResume(friend.uid, resumeId);
    resumesOpen.value = false;
    note.value = RESUME_SENT_NOTE;
    await loadMessages(friend.uid);
  } catch (e) {
    note.value = `发送简历失败：${errText(e)}`;
  }
}

/* -------------------------------- 打招呼链路 --------------------------------- */

/** 「放到输入框再改」：草稿搬进输入框并聚焦，草稿框收起。 */
function greetToDraft(): void {
  const g = greet.value;
  if (g === null) return;
  draft.value = g.text;
  greet.value = null;
  composeRef.value?.focus();
}

/**
 * 从概览页的「打招呼」方块过来：消费 greet-draft，
 * 按公司前缀找好友；找不到就插一条占位会话；预填话术并聚焦输入框（不自动发送）。
 */
function consumeGreetDraft(): void {
  const intent = takeGreetDraft();
  if (intent === null) return;

  let target = matchFriendByCompany(friends.value, intent.company);
  if (target === null) {
    const placeholder = pendingFriend(intent.company, intent.company);
    friends.value = withPending(friends.value, placeholder);
    target = placeholder;
  }
  const uid = target.uid;
  void (async () => {
    ui.value = pickFriend(ui.value, uid);
    messages.value = [];
    draft.value = intent.text;
    await loadResumes();
    // 聚焦是用户显式意图（SPEC §6.2 只允许这一处抢焦点）。
    composeRef.value?.focus();
  })();
}

/* -------------------------------- 自动投递 ----------------------------------- */

const autoPhase = computed<AutoPhase>(() => {
  if (autoSending.value) return 'running';
  if (autoStatus.value.paused) return 'paused';
  if (autoStatus.value.running) return 'running';
  return String(autoStatus.value.stop_reason ?? '').trim() === '' ? 'idle' : 'stopped';
});

function onAutoNote(text: string): void {
  if (text.trim() === '') return;
  autoNote.value = text;
  note.value = text;
}

function startQueue(): void {
  if (queue !== null) return;
  queue = startAutoQueue({
    onStatus(st) {
      autoStatus.value = st;
    },
    onSending(v) {
      autoSending.value = v;
    },
    onSent() {
      autoNote.value = '自动投递已发出一条';
    },
    onNote: onAutoNote,
  });
}

async function onAutoToggle(): Promise<void> {
  if (autoBusy.value) return;
  autoBusy.value = true;
  autoNote.value = '';
  const action = toggleAction(autoPhase.value);
  try {
    if (action.kind === 'start') {
      // 开始前先清零上一轮计数，否则「已发 N 条」会接着上次累加。
      await autoGreetReset();
      await autoGreetResume();
    } else if (action.kind === 'pause') {
      await autoGreetPause(action.reason);
    } else {
      await autoGreetResume();
    }
    queue?.kick();
  } catch (e) {
    onAutoNote(`自动投递操作失败：${errText(e)}`);
  } finally {
    autoBusy.value = false;
  }
}

async function onAutoStop(): Promise<void> {
  if (autoBusy.value) return;
  autoBusy.value = true;
  try {
    await autoGreetReset();
    queue?.kick();
    autoNote.value = '已结束本轮自动投递';
  } catch (e) {
    onAutoNote(`重置失败：${errText(e)}`);
  } finally {
    autoBusy.value = false;
  }
}

/* --------------------------------- 生命周期 ---------------------------------- */

function onResize(): void {
  innerWidth.value = window.innerWidth;
}

function onKeydown(ev: KeyboardEvent): void {
  if (ev.key !== 'Escape') return;
  // Esc 分层：本页没有更上层的东西，直接收对话框（SPEC §6.1）。
  if (ui.value.chatOpen) {
    ev.stopPropagation();
    close();
  }
}

onMounted(() => {
  window.addEventListener('resize', onResize);
  window.addEventListener('keydown', onKeydown);
  // 首帧先把已存在的草稿渲染出来（可能来自本页内的「生成打招呼」往返）；
  // 从概览页传过来的意图随后消费。
  const pendingDraft = peekGreetDraft();
  void (async () => {
    await loadFriends();
    consumeGreetDraft();
    if (pendingDraft !== null && draft.value === '') draft.value = pendingDraft.text;
  })();
  void loadExpectations();
  startQueue();
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize);
  window.removeEventListener('keydown', onKeydown);
  queue?.stop();
  queue = null;
});

watch(
  () => ui.value.chatOpen,
  (open) => {
    emit('chat-open', open);
    if (!open) return;
    const friend = activeFriend.value;
    if (friend !== null && messages.value.length === 0) void loadMessages(friend.uid);
  },
);

/* ---------------------------------- 工具 ------------------------------------ */

function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** 自动投递条：展开态住对话框头，收起态住右栏最上方 —— 两处二选一，不复制。 */
const autoBarLocation = computed(() => (ui.value.chatOpen ? 'detach' : 'rail'));

defineExpose({
  /** 测试用：当前展开态快照。 */
  state: ui,
  detachW,
  chatW,
  autoBarLocation,
  autoPhase,
  note,
  friends,
  messages,
  draft,
  pick,
  close,
  send,
  consumeGreetDraft,
  loadFriends,
});
</script>

<template>
  <section class="page page-chat" data-testid="page-chat" :data-active-nav="props.activeNav">
    <!-- ===== 右栏：Boss 列表（默认常驻） ===== -->
    <aside class="chat-rail" data-testid="chat-rail">
      <AutoApplyBar
        v-if="autoBarLocation === 'rail'"
        :status="autoStatus"
        :sending="autoSending"
        :busy="autoBusy"
        @toggle="onAutoToggle"
        @stop="onAutoStop"
      />
      <FriendList
        prefix="chat-rail"
        :title="CHAT_RAIL_TITLE"
        :friends="visibleFriends"
        :active-uid="ui.activeFriendsUid"
        :loading="loadingFriends"
        :error="friendsError"
        :search="search"
        @pick="pick"
        @refresh="loadFriends"
        @retry="loadFriends"
        @search="search = $event"
      />
    </aside>

    <!-- ===== 对话框：点 Boss 才展开 ===== -->
    <ChatDetach
      :open="ui.chatOpen"
      :friend="activeFriend"
      :count="friends.length"
      @close="close"
    >
      <template #head>
        <AutoApplyBar
          v-if="autoBarLocation === 'detach'"
          :status="autoStatus"
          :sending="autoSending"
          :busy="autoBusy"
          @toggle="onAutoToggle"
          @stop="onAutoStop"
        />
      </template>

      <template #rail>
        <FriendList
          v-if="ui.chatOpen"
          prefix="chat-detach-rail"
          :title="'Boss 列表'"
          :friends="visibleFriends"
          :active-uid="ui.activeFriendsUid"
          :loading="loadingFriends"
          :error="friendsError"
          :search="search"
          @pick="pick"
          @refresh="loadFriends"
          @retry="loadFriends"
          @search="search = $event"
        />
      </template>

      <ChatThread
        :friend="talkableFriend"
        :messages="messages"
        :loading-messages="loadingMessages"
        :resumes="resumes"
        :resumes-open="resumesOpen"
        :resumes-loaded="resumesLoaded"
        :greet="greet"
        :generating="generating"
        :sending-greet="sendingGreet"
        :recalling="recalling"
        @generate-greet="generateGreet"
        @toggle-resumes="resumesOpen = !resumesOpen"
        @send-resume="sendResume"
        @recall="recall"
        @send-greet="sendGreet"
        @greet-to-draft="greetToDraft"
        @discard-greet="greet = null"
      >
        <template #compose>
          <!-- 唯一输入框：显式绑 modelValue + update（不走 v-model 的模板展开）。 -->
          <ChatCompose
            v-if="canTalk"
            ref="composeRef"
            :model-value="draft"
            :sending="sending"
            @update:model-value="draft = $event"
            @send="send"
          />
        </template>
      </ChatThread>
    </ChatDetach>

    <!-- ===== 求职期望 + 提示行 ===== -->
    <ExpectRow :items="expectations" :loading="expectLoading" :error="expectError" />
    <div v-if="note !== ''" class="cfg-hint chat-note" data-testid="chat-note">{{ note }}</div>
    <div v-if="autoNote !== ''" class="cfg-hint chat-note" data-testid="auto-note">{{ autoNote }}</div>
  </section>
</template>
