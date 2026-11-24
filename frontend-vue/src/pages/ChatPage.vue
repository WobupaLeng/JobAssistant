<script setup lang="ts">
/**
 * 同屏聊天层（原「Boss 消息页」）。
 *
 * **决策溯源见改造入口 `App.vue`（ADR-004 只在那里落一处注释）**：聊天不再是一整页，
 * 本组件随之从 `pages/` 的整页升为壳级常驻层（图纸
 * [`SPEC-001-legacy-chat-merge`](../../docs/specs/SPEC-001-legacy-chat-merge.md) 决策 1）。
 * 文件名保留 `ChatPage.vue`：`tests/chat-contract.test.ts` / `geometry-collect.test.ts` /
 * `qa-testids.test.ts` 都按这个路径读源码取证，改名等于把 QA 锚点地图一起改掉。
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
 * ## DOM 形状（SPEC §2.1 / §2.4 / A15）
 * 本组件是**多根 fragment**：`aside.chat-rail` 与 `.chat-detach` 直接成为外壳
 * `.app-body` 的 flex 子项，中间不再套 `<section class="page-chat">`。套一层就多出一个布局盒，
 * 对话框便不是「flex 压缩主列」而是被套住 —— A15 的 `position: static` 与主列宽度联动
 * 判据都要求它按 `.app-body` 的子项参与布局。两条瞬态提示行归右栏末尾；
 * 求职期望行已按 SPEC-001 Rev3/Rev5 退役（原型 L926-927：期望并入筛选条件面板，右栏没有那行）。
 *
 * ## 数据
 * 好友列表 / 消息 / 简历 / 期望职位都走 `src/api/commands/*`，本层不假设任何后端形状。
 * **层与概览同屏 ⇒ 会话列表在应用启动时就拉一次**（R1 的「不切页就能发现新消息」），
 * 自动投递状态由 `lib_ui/auto-heartbeat.ts` 的心跳驱动：**没有任何轮询**——状态只在
 * 「60~90 秒随机到期」「每次投递完成」「发完消息」「手动刷新」这几个时刻请求一次。
 *
 * ## 独立 mount
 * 本层不依赖 `App.vue`：测试可以直接 `mount(ChatPage)`（SPEC-001 测试口径「保留独立 mount」）。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { autoGreetNext, autoGreetPause, autoGreetReset, autoGreetResume, autoGreetStatus } from '../api/commands/auto';
import {
  chatFriends,
  chatFriendsLocal,
  chatMessages,
  chatMessagesLocal,
  chatRecall,

  chatSend,

} from '../api/commands/chat';
import { greetDraft, greetOne } from '../api/commands/greet';
import { loginStatus, sessionOpen } from '../api/commands/scrape';
import type { ChatMessage, Friend, GreetReply } from '../api/types';
import AutoApplyBar from '../components/chat/AutoApplyBar.vue';
import ChatCompose from '../components/chat/ChatCompose.vue';
import ChatDetach from '../components/chat/ChatDetach.vue';
import ChatThread from '../components/chat/ChatThread.vue';
import FriendList from '../components/chat/FriendList.vue';
import {
  emptyStatus,
  toggleAction,
  type AutoPhase,
} from '../lib_ui/auto-model';
import { startHeartbeat, type HeartbeatTimer } from '../lib_ui/auto-heartbeat';
import {
  CHAT_RAIL_TITLE,
  CHAT_STATE_ANNOUNCE_TEXT,
  FRIENDS_EMPTY_NOTE,
  FRIENDS_FAIL_PREFIX,
  GREET_SENT_NOTE,
  MESSAGES_FAIL_PREFIX,
  RECALL_DONE_NOTE,
  SEND_FAIL_PREFIX,
  SESSION_BOOTING_NOTE,
  canSend,
  chatAreaWidth,
  closeDetach,
  createChatUi,
  detachWidth,
  filterFriends,
  isPendingFriend,
  loadPendingFriends,
  savePendingFriends,
  CHAT_ACTIVE_COMPANY_KEY,
  CHAT_ACTIVE_COMPANY_EVENT,
  isSendableFriend,
  matchFriendByCompany,
  nowText,
  pendingFriend,
  pickFriend,
  shouldRetryWithSession,
  withPending,
} from '../lib_ui/chat-model';
import { needsSession } from '../lib_ui/session';
import { isLoggedIn } from '../lib_ui/login-model';
import { peekGreetDraft, setGreetDraft, takeGreetDraft } from '../features/chat/greet-draft';
import { hasHigherOverlay } from '../lib_ui/overlay-stack';

/**
 * 展开态上抛给 `App.vue` → `AppShell` 的 `.is-chat-open`：
 * 右栏让位与「列数 = 收起态 − 1」（SPEC §16.9 裁决 X）的 CSS 钩子都挂在那一个壳节点上。
 * 本层不再 emit `navigate` —— 聊天不是页，没有可去的页。
 */
const emit = defineEmits<{
  (e: 'chat-open', open: boolean): void;
}>();

/* ---------------------------------- 会话列表 ---------------------------------- */

const friends = ref<Friend[]>([]);
const loadingFriends = ref(true);
const friendsError = ref('');
/** 卡片「打招呼」带来的岗位：只对该 Boss 生效，发送成功时只标记此岗位已投递。 */
const greetJob = ref<{ uid: string; jobId: string } | null>(null);
const search = ref('');

/* ---------------------------------- 展开态 ---------------------------------- */

const ui = ref(createChatUi());

/* ---------------------------------- 线程 ------------------------------------ */

const messages = ref<ChatMessage[]>([]);
const loadingMessages = ref(false);
const note = ref('');

/* -------------------------------- 打招呼草稿 --------------------------------- */

const greet = ref<GreetReply | null>(null);
const generating = ref(false);
const sendingGreet = ref(false);
const recalling = ref(false);
const sending = ref(false);
const draft = ref('');

/* -------------------------------- 自动投递 ----------------------------------- */

const autoStatus = ref(emptyStatus());
const autoSending = ref(false);
const autoBusy = ref(false);
const autoNote = ref('');

/**
 * 状态心跳（`lib_ui/auto-heartbeat.ts`）：60~90 秒随机，只受登录限制。
 * 这是本页**唯一**的 `auto_greet_status` 周期性来源——没有 1 秒轮询。
 * 投递完成、发完消息、手动刷新都会调 `beat.start()` 立刻主动请求一次。
 */
let beat: HeartbeatTimer | null = null;

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

async function loadFriends(allowOpenSession = false): Promise<void> {
  loadingFriends.value = true;
  friendsError.value = '';
  // 先把刷新前持久化的占位放回列表：后端报错（浏览器未连接）时它们也必须可见。
  friends.value = [...loadPendingFriends(), ...friends.value.filter((f) => !isPendingFriend(f))];
  // 先默认展示持久化的会话列表（不触发浏览器），远程成功后再增量替换。
  try {
    const cached = await chatFriendsLocal();
    if (friends.value.length === 0 && cached.items.length > 0) {
      friends.value = cached.items;
    }
  } catch {
    /* 本地读取失败不阻断远程加载 */
  }
  try {
    let reply;
    try {
      reply = await chatFriends();
    } catch (e) {
      const text = errText(e);
      // 被动：挂载时不得拉起 sidecar/浏览器，只有用户主动操作才允许自动开会话。
      if (!allowOpenSession || !shouldRetryWithSession(text)) throw e;
      note.value = SESSION_BOOTING_NOTE;
      await sessionOpen('boss', 'chat');
      reply = await chatFriends();
    }
    // 占位（后端尚无会话）条目：内存里的 + 刷新前持久化的，去重后保留在列表头部。
    const seen = new Set(reply.items.map((r) => r.company));
    const locals = [...friends.value, ...loadPendingFriends()].filter((f, i, all) => {
      if (!isPendingFriend(f) || seen.has(f.company)) return false;
      return all.findIndex((g) => g.uid === f.uid) === i;
    });
    friends.value = [...locals, ...reply.items];
    savePendingFriends(friends.value);
    if (reply.items.length === 0 && locals.length === 0) note.value = FRIENDS_EMPTY_NOTE;
    // 刷新不得改变展开态与选中（SPEC §3）：只把 activeFriend 的字段换成新的。
    const uid = ui.value.activeFriendsUid;
    if (uid !== null && !friends.value.some((f) => f.uid === uid)) {
      ui.value = closeDetach(ui.value);
    }
  } catch (e) {
    friendsError.value = errText(e);
    // 浏览器未连接时 rail 已给出引导提示，不再把原始报错重复写进 note（否则两段文字叠在一起）。
    if (!needsSession(errText(e))) note.value = FRIENDS_FAIL_PREFIX + errText(e);
  } finally {
    loadingFriends.value = false;
  }
}

async function loadMessages(uid: string): Promise<void> {
  loadingMessages.value = true;
  // 先展示本地持久化消息（不触发浏览器），再用远程结果增量覆盖。
  try {
    const local = await chatMessagesLocal(uid);
    if (ui.value.activeFriendsUid === uid && messages.value.length === 0) {
      messages.value = local.items;
    }
  } catch {
    /* 本地读取失败不阻断远程加载 */
  }
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
    note.value = '';
    await loadMessages(uid);
  }
}

/** 收起：保留 activeFriend（A7）；再点同一位也能重开（A8）。 */
function close(): void {
  ui.value = closeDetach(ui.value);
}

/* ------------------------- Boss 列表行内操作（下沉） ------------------------- */

/**
 * 行内「打招呼」：先选中该行，再生成话术。
 *
 * 原先「生成打招呼」在对话区顶部，作用于「当前选中的人」；下沉到列表行后，
 * 点哪一行就是对谁操作，所以必须先 `pick` 把选中态对齐，否则会给上一行的人
 * 生成话术——那是最典型的错位。已选中时 `pick` 不会重载消息（幂等），
 * 所以重复点同一行是安全的。
 */
/** 手动打招呼新增的占位 HR 可移除：从内存与持久化里同时删除；不碰后端真实会话。 */
const removableUids = computed(() => friends.value.filter(isPendingFriend).map((f) => f.uid));

function removePending(uid: string): void {
  if (!removableUids.value.includes(uid)) return;
  friends.value = friends.value.filter((f) => f.uid !== uid);
  savePendingFriends(friends.value);
  if (ui.value.activeFriendsUid === uid) ui.value = closeDetach(ui.value);
  note.value = '已移除该 Boss';
}

async function greetFor(uid: string): Promise<void> {
  if (generating.value) return;
  if (ui.value.activeFriendsUid !== uid) await pick(uid);
  await generateGreet();
}

/* ---------------------------------- 发送 ------------------------------------ */

async function send(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || !canSend(draft.value, sending.value)) return;
  const text = draft.value.trim();
  sending.value = true;
  try {
    // 发送前先验证登录态：只有已登录才真正发出，否则给出对应提示。
    // sidecar 未启动时 loginStatus 本身会抛错：同样视为「浏览器未连接」，不落入「发送失败」。
    const st = await loginStatus('boss').catch(() => ({ state: 'unknown' as const }));
    if (st.state !== 'logged_in') {
      note.value =
        st.state === 'no_login'
          ? '未登录 Boss 直聘：请先点「扫码登录」，登录后再发送消息'
          : '浏览器未连接：请先点「开始抓取」或「扫码登录」，连接后再发送消息';
      return;
    }
    // 只有发给「卡片打招呼的那位 Boss」时才带岗位 id，后端据此只标记这一个岗位已投递。
    const jobId = greetJob.value !== null && greetJob.value.uid === friend.uid ? greetJob.value.jobId : null;
    const reply = await chatSend(friend.uid, text, jobId);
    const outgoing = String(reply.text ?? '') !== '' ? String(reply.text) : text;
    messages.value = [
      ...messages.value,
      { sender: 'me', content: outgoing, time: nowText(), msg_id: '', recalled: false },
    ];
    draft.value = '';
    note.value = '消息已发送';
    // 已用掉这次岗位标记：之后再发给同一 Boss 不得重复标记。
    if (jobId !== null) greetJob.value = null;
    // 发完消息主动请求一次自动投递状态（不计入 60~90 秒的心跳节拍）。
    if (beat !== null) void beat.start();
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
    // 打招呼成功即把对方挂进 Boss 列表：占位会话（无后端会话）此时才真正入列。
    if (isPendingFriend(friend)) {
      friends.value = withPending(
        friends.value.filter((f) => f.uid !== friend.uid),
        friend,
      );
      savePendingFriends(friends.value);
    }
    note.value = GREET_SENT_NOTE;
  } catch (e) {
    note.value = `发送失败：${errText(e)}`;
  } finally {
    sendingGreet.value = false;
  }
}

/**
 * 生成打招呼话术。`reply` 非空时走「根据 HR 回复生成」。
 *
 * 聊天页拿不到 JD（`Friend` 里没有 `jd_text`），所以这里把公司名交给后端
 * `findByCompany`：后端按公司名翻库里的岗位，用**整段 JD** 组提示词。
 * 翻不到时后端会退回按标题/公司写，并在回执 `jd_used=false` 里说清楚——
 * 界面据此提示，而不是让用户以为模型读了 JD。
 */
async function generateGreet(reply = ''): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || generating.value) return;
  generating.value = true;
  try {
    greet.value = await greetDraft({
      jobId: greetJob.value?.uid === friend.uid ? greetJob.value.jobId : '',
      title: friend.title,
      company: friend.company,
      salary: '',
      experience: '',
      jd: '',
      findByCompany: friend.company,
      reply,
    });
  } catch (e) {
    note.value = `生成打招呼失败：${errText(e)}`;
  } finally {
    generating.value = false;
  }
}

/** 「重新生成」：同一份入参再调一次。 */
function regenerateGreet(): void {
  void generateGreet();
}

/** 当前 Boss 最近一条 HR 消息（`sender === 'hr'`）。没有或已撤回返回空串。 */
const lastHrMessage = computed<string>(() => {
  for (let i = messages.value.length - 1; i >= 0; i -= 1) {
    const m = messages.value[i];
    if (m === undefined) continue;
    if (m.sender !== 'hr' || m.recalled) continue;
    const text = m.content.trim();
    if (text !== '') return text;
  }
  return '';
});

/** 「生成回复」：把 HR 的原话一起交给后端，提示词据此接话。 */
function generateReply(): void {
  const reply = lastHrMessage.value;
  if (reply === '') {
    note.value = '还没有 HR 的回复可以接，先等对方说话';
    return;
  }
  void generateGreet(reply);
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

  const target = matchFriendByCompany(friends.value, intent.company);
  // 1）入列：已有好友直接复用，否则插一条占位（手动打招呼新增，可移除）。
  let uid: string;
  if (target !== null) {
    uid = target.uid;
  } else {
    const placeholder = pendingFriend(intent.company, intent.company);
    friends.value = withPending(friends.value, placeholder);
    savePendingFriends(friends.value);
    uid = placeholder.uid;
  }
  // 记住被点击的岗位，仅对该 Boss 生效：发送成功时只标记这一个岗位已投递。
  greetJob.value = { uid, jobId: intent.jobId };
  // 2）展开聊天框 + 3）生成话术：复用 greetFor（它先 pick 展开，再 generateGreet）。
  void greetFor(uid);
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

async function onAutoToggle(): Promise<void> {
  if (autoBusy.value) return;
  autoBusy.value = true;
  autoNote.value = '';
  const action = toggleAction(autoPhase.value);
  try {
    if (action.kind === 'start') {
      // 开始前先确认登录有效：自动投递只往**已有**会话里发消息，未登录时
      // 既发不出去也不该去找浏览器（那正是「窗口无限打开」的来源）。
      // 后端 `auto_greet_next` 也有同一道闸，这里是让用户当场看到原因。
      const st = await loginStatus('boss');
      // `unknown` = 浏览器（sidecar）还没拉起来：单独提示，不与「登录过期」混为一谈。
      if (st.state === 'unknown') {
        autoNote.value = '浏览器未连接：请先点「开始抓取」或「扫码登录」，连接后再开始投递';
        return;
      }
      if (!isLoggedIn(st.state)) {
        autoNote.value = '未登录 Boss 直聘：请先点「扫码登录」，登录后再开始投递';
        return;
      }
      // 开始前先清零上一轮计数，否则「已发 N 条」会接着上次累加。
      await autoGreetReset();
      await autoGreetResume();
    } else if (action.kind === 'pause') {
      await autoGreetPause(action.reason);
    } else {
      await autoGreetResume();
    }
    // 唯一的状态请求来源：心跳（60~90 秒随机）。开始 / 暂停 / 继续都只是
    // 「立刻主动请求一次」，读到的快照里 `next_at_ms` 到期就发那一条。
    await beat?.start();
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
    // 结束后不停表：心跳是登录态心跳，跟投递是否在跑无关，继续刷新状态。
    await beat?.start();
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
  if (!ui.value.chatOpen) return;
  // Esc 分层（SPEC §6.1 / A10）：一次 keydown 只关最上面那一层。
  // 决策记录 / 配置 / 岗位抽屉 / 敏感库开着时，这层不出手（优先级表单源在
  // `lib_ui/overlay-stack.ts`，四个消费方共用，不在各自文件里抄选择器）。
  if (hasHigherOverlay('chat')) return;
  ev.stopPropagation();
  close();
}

onMounted(() => {
  window.addEventListener('resize', onResize);
  window.addEventListener('keydown', onKeydown);
  // 首帧先把已存在的草稿渲染出来（可能来自本页内的「生成打招呼」往返）；
  // 从概览页传过来的意图随后消费。
  const pendingDraft = peekGreetDraft();
  void (async () => {
    // 挂载是被动加载：不允许自动拉起浏览器（allowOpenSession 缺省 false）。
    await loadFriends();
    consumeGreetDraft();
    // 心跳只受登录限制：已登录就开表（不论自动投递是否在跑），
    // 未登录 / 浏览器未连接则不开表，只给一句引导。
    beat = startHeartbeat({
      onStatus(st) {
        autoStatus.value = st;
      },
      onSending(v) {
        autoSending.value = v;
      },
      onNote: onAutoNote,
    });
    await beat.start();
    // 后台已在跑的投递必须当场显示「暂停」：心跳开表时已读过一次，只补读状态。
    if (!autoStatus.value.running) {
      try {
        autoStatus.value = await autoGreetStatus();
      } catch {
        /* 读不到就保持空态，等心跳或用户操作 */
      }
    }
    if (pendingDraft !== null && draft.value === '') draft.value = pendingDraft.text;
  })();
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', onResize);
  window.removeEventListener('keydown', onKeydown);
  beat?.stop();
  beat = null;
});

// 跨页联动：展开聊天时把该 Boss 的公司名写入共享键，概览页据此高亮同公司岗位；收起即清除。
watch(
  () => [ui.value.chatOpen, (activeFriend.value?.company ?? '').trim()] as const,
  ([open, company]) => {
    try {
      if (open && company !== '') localStorage.setItem(CHAT_ACTIVE_COMPANY_KEY, company);
      else localStorage.removeItem(CHAT_ACTIVE_COMPANY_KEY);
      // storage 事件不会在写入方所在标签页触发：补一个同页事件，让已挂载的概览页即时更新。
      window.dispatchEvent(new Event(CHAT_ACTIVE_COMPANY_EVENT));
    } catch {
      /* 存储不可用：静默降级，仅影响跨页高亮 */
    }
  },
  { immediate: true },
);

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
  <!-- ===== 右栏：Boss 列表（默认常驻，`.app-body` 的直接 flex 子项） ===== -->
  <aside class="chat-rail hl-surface" data-testid="chat-rail">
    <AutoApplyBar
      v-if="autoBarLocation === 'rail'"
      :status="autoStatus"
      :sending="autoSending"
      :busy="autoBusy"
      @toggle="onAutoToggle"
      @stop="onAutoStop"
    />
    <!-- 待投递/已投递：紧贴自动投递条下方另起一行（原先挤在列表头里，被「刷新」挤掉）。 -->
    <div v-if="autoBarLocation === 'rail'" class="rail-counts" data-testid="auto-apply-counts">
      <span data-testid="auto-apply-pending">待投递 <b>{{ autoStatus.pending }}</b></span>
      <span data-testid="auto-apply-done">已投递 <b>{{ autoStatus.sent }}</b></span>
    </div>
    <FriendList
      prefix="chat-rail"
      :title="CHAT_RAIL_TITLE"
      :friends="visibleFriends"
      :active-uid="ui.activeFriendsUid"
      :loading="loadingFriends"
      :error="friendsError"
      :search="search"
      @pick="pick"
      @greet="greetFor"
      :removable-uids="removableUids"
      @remove="removePending"
      @refresh="loadFriends"
      @retry="loadFriends"
      @search="search = $event"
    />
    <!-- 右栏 aria-live 汇报区（SPEC §4.2 的 chat-rail-state-announce）：
         与对话框里的 chat-detach-state-announce 同源，文案都由常量给出，不复制字面量。 -->
    <span class="sr-only" data-testid="chat-rail-state-announce" aria-live="polite">{{
      CHAT_STATE_ANNOUNCE_TEXT
    }}</span>

    <!-- 两条瞬态提示行（结论②保留：发送失败/会话启动的反馈依赖它，K9「错误必须可读」）。
         求职期望行已按 SPEC-001 Rev3/Rev5 退役：原型 L926-927 明写「求职期望已并入筛选
         条件面板；打分试算已移除」，四档基线图右栏也没有这一行；期望数据仍由概览筛选面板
         的「读取期望职位」承载（DashboardPage.loadExpects 不动，api 封装与契约面都不动）。 -->
    <div v-if="note !== ''" class="cfg-hint chat-note" data-testid="chat-note">{{ note }}</div>
    <div v-if="autoNote !== ''" class="cfg-hint chat-note" data-testid="auto-note">{{ autoNote }}</div>
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
    <template #head-sub>
      <span v-if="autoBarLocation === 'detach'" class="rail-counts" data-testid="auto-apply-counts">
        <span data-testid="auto-apply-pending">待投递 <b>{{ autoStatus.pending }}</b></span>
        <span data-testid="auto-apply-done">已投递 <b>{{ autoStatus.sent }}</b></span>
      </span>
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
        :greeting-uid="generating ? (ui.activeFriendsUid ?? '') : ''"
        @pick="pick"
        @refresh="loadFriends"
        @retry="loadFriends"
        @search="search = $event"
        @greet="greetFor"
        :removable-uids="removableUids"
        @remove="removePending"
      />
    </template>

    <ChatThread
      :friend="talkableFriend"
      :messages="messages"
      :loading-messages="loadingMessages"
      :greet="greet"
      :generating="generating"
      :sending-greet="sendingGreet"
      :recalling="recalling"
      :has-hr-reply="lastHrMessage !== ''"
      @generate-greet="regenerateGreet"
      @generate-reply="generateReply"
      @recall="recall"
      @send-greet="sendGreet"
      @greet-to-draft="greetToDraft"
      @discard-greet="greet = null"
    >
      <template #notices>
        <!-- 展开态的提示：右栏的 note / autoNote 在对话框展开时看不到，这里同源再渲染一份（置顶）。 -->
        <div v-if="note !== '' || autoNote !== ''" class="detach-notes">
          <div v-if="note !== ''" class="detach-note" data-testid="chat-note-detach">{{ note }}</div>
          <div v-if="autoNote !== ''" class="detach-note" data-testid="auto-note-detach">{{ autoNote }}</div>
        </div>
      </template>
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
</template>
