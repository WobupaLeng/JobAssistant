<script setup lang="ts">
/**
 * 同屏聊天层。文件名保留 `ChatPage.vue`：契约测试按这个路径读源码取证。右栏常驻，对话框默认
 * `hidden`；点任意 Boss 展开且右栏让位；关闭按钮或 Esc 收起并**保留** activeFriend。会话列表
 * 启动时拉一次，自动投递由 `auto-heartbeat.ts` 驱动，**没有任何轮询**。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { autoGreetNext, autoGreetPause, autoGreetReset, autoGreetResume, autoGreetStatus } from '../api/commands/auto';
import {
  chatFriends,
  chatFriendsLocal,
  chatMessages,
  chatMessagesLocal,
  chatOpen,
  chatRecall,

  jobFetchDetail,
  chatSend,

} from '../api/commands/chat';
import { greetDraft, greetOne } from '../api/commands/greet';
import { loginAck, loginStatus, sessionOpen } from '../api/commands/scrape';
import { allowJobs, getJob, markDelivered, rejectJobs, unmarkDelivered } from '../api/commands/db';
import { getScoringSpec } from '../api/commands/shell';
import { listenLoginState, type UnlistenFn } from '../api/events';
import type { ChatMessage, Friend, GreetReply, Job } from '../api/types';
import AutoApplyBar from '../components/chat/AutoApplyBar.vue';
import ChatCompose from '../components/chat/ChatCompose.vue';
import ChatDetach from '../components/chat/ChatDetach.vue';
import ChatThread from '../components/chat/ChatThread.vue';
import FriendList from '../components/chat/FriendList.vue';
import JobDetailDrawer from '../components/JobDetailDrawer.vue';
import {
  emptyStatus,
  toggleAction,
  type AutoPhase,
} from '../lib_ui/auto-model';
import {
  BROWSER_DETACHED_NOTE,
  NOT_LOGGED_IN_NOTE,
  startHeartbeat,
  type HeartbeatTimer,
} from '../lib_ui/auto-heartbeat';
import { markJobSent } from '../lib_ui/job-live';
import { markTokensSpent } from '../lib_ui/token-ledger';
import {
  AI_DRAFT_FAIL_PREFIX,
  CHAT_RAIL_TITLE,
  CHAT_STATE_ANNOUNCE_TEXT,
  FRIENDS_EMPTY_NOTE,
  FRIENDS_FAIL_PREFIX,
  GREET_SENT_NOTE,
  MESSAGES_FAIL_PREFIX,
  MESSAGES_FRESH_NOTE,
  MESSAGES_FRESH_SHORT,
  RECALL_DONE_NOTE,
  SEND_FAIL_PREFIX,
  SESSION_BOOTING_NOTE,
  aiReplyContext,
  canSend,
  chatAreaWidth,
  closeDetach,
  createChatUi,
  detachWidth,
  filterFriends,
  isPendingFriend,
  loadPendingFriends,
  makeMessagesMemo,
  markFriendSent,
  mergeFriends,
  mergeMessages,
  OPENING_SESSION_NOTE,
  OPEN_SESSION_FAIL_PREFIX,
  savePendingFriends,
  seedLocalMsgIds,
  CHAT_ACTIVE_COMPANY_KEY,
  CHAT_ACTIVE_COMPANY_EVENT,
  isSendableFriend,
  matchFriendByCompany,
  nowText,
  pendingFriend,
  pendingJobId,
  pickFriend,
  shouldRetryWithSession,
  sortFriends,
  withPending,
} from '../lib_ui/chat-model';
import { loadReadMarks, saveReadMark, unreadSessions, type ReadMarks } from '../lib_ui/chat-read';
import { needsSession } from '../lib_ui/session';
import { isKnownState, isLoggedIn } from '../lib_ui/login-model';
import { withLoginAck } from '../lib_ui/login-broadcast';
import { peekGreetDraft, setGreetDraft, takeGreetDraft } from '../features/chat/greet-draft';
import { actionFailPrefix, actionNote, type DetailAct } from '../lib_ui/job-detail-model';
import { hasHigherOverlay } from '../lib_ui/overlay-stack';

/**
 * 展开态上抛给 `App.vue` → `AppShell` 的 `.is-chat-open`：
 * 右栏让位与「列数 = 收起态 − 1」的 CSS 钩子都挂在那一个壳节点上。
 * 本层不再 emit `navigate` —— 聊天不是页，没有可去的页。
 */
const emit = defineEmits<{
  (e: 'chat-open', open: boolean): void;
}>();

/* ---------------------------------- 会话列表 ---------------------------------- */

const friends = ref<Friend[]>([]);
/** 已读水位：Boss 的 unreadMsgCount 恒为 0，未读只能按「HR 最后发言 + 我没读过」派生。 */
const readMarks = ref<ReadMarks>(loadReadMarks());
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
/**
 * `note` 的语气：`bad` 红条（失败，可执行），`hint` 灰色说明（不是错误）。
 * 判据只看文案前缀：失败提示都带 `…失败：`，成功提示无前缀。
 */
const noteTone = computed<'bad' | 'hint'>(() => {
  const text = note.value;
  if (text === MESSAGES_FRESH_NOTE) return 'hint';
  // 读取/导航类失败一律按「重试」呈现。
  if (isRetryableNote(text)) return 'bad';
  if (/失败[:：]/.test(text)) return 'bad';
  return 'hint';
});
/** 顶部红条用的那句（只有真错误才占顶部）。 */
const topNote = computed(() => (noteTone.value === 'bad' ? note.value : ''));
/**
 * 消息流末尾灰色说明条用的那句。
 * 「已是最新消息，点「刷新」可重新拉取」在气泡区只用前半句：刷新按钮就在列表头顶，
 * 对话框里重复一遍只会把一句灰字挤成两行。列表头顶不再重复（用户要求撤掉那条灰条）。
 */
const bottomNote = computed(() => {
  if (noteTone.value !== 'hint') return '';
  return note.value === MESSAGES_FRESH_NOTE ? MESSAGES_FRESH_SHORT : note.value;
});
/**
 * 「已是最新消息」只在消息流末尾出现一次（见 `bottomNote`）。
 * 从前它还会在 Boss 列表最上方顶一条灰条，用户要求撤掉；这里保留判据是因为
 * 列表下方那条瞬态提示条要靠它让位（同一句话不必说两遍，见模板 `chat-note` 的 `v-if`）。
 */
const freshNote = computed(() => (note.value === MESSAGES_FRESH_NOTE ? note.value : ''));
/**
 * 第二条提示（`autoNote`，心跳/自动投递写的）：与 `note` 说的是**同一件事**时不再重复。
 *
 * 典型现场（用户 m00837）：sidecar 没起来 → 心跳先写「浏览器未连接：请先登录」，
 * 随后点开一条 Boss，读消息失败又把 `note` 写成「读取消息失败：sidecar 尚未启动。
 * 请先点「开始抓取」或「扫码登录」…」。两句都是「浏览器没连上」，并排就是同一句话
 * 说两遍；这里只留信息更全的那条（`note`），登录成功后两条照旧由 `onLoginState` 清掉。
 */
const extraNote = computed(() => {
  const text = autoNote.value;
  if (text === '') return '';
  if (text === note.value) return '';
  if (isSessionNote(text) && isSessionNote(note.value)) return '';
  return text;
});

/** 心跳写的这条是不是真错误（决定 auto-note 用红条还是中性条）。 */
const autoNoteBad = computed(() => {
  const text = extraNote.value;
  return text !== '' && (isSessionNote(text) || /失败[:：]/.test(text));
});

/**
 * 读取/建会话/发送失败时把 Playwright 原始文案换成一句「能照做」的话，
 * 原文仍进日志。只认三类原因：导航被打断、浏览器断开、会话被报成不存在；
 * 其余错误照原样显示。
 */
const RETRY_READ = '读取消息失败：Boss 页面没打开（网络慢或被风控拦了一次）。再点一次这条会话即可，仍失败就点「刷新」。';
const RETRY_FRIENDS = '读取会话失败：Boss 页面没打开（网络慢或被风控拦了一次）。点「刷新」重试。';
const RETRY_SEND = '发送失败：Boss 页面没打开（网络慢或被风控拦了一次）。再点一次发送，仍失败就点「刷新」。';
function navAbortOf(text: string, fallback: string): string {
  if (/ERR_ABORTED|ERR_CONNECTION|ERR_TIMED_OUT|ERR_NETWORK_CHANGED/.test(text)) return fallback;
  if (/Target page, context or browser has been closed|has been closed|浏览器未连接/.test(text)) {
    return 'Boss 浏览器连接断了。先点右上角「扫码登录」把浏览器拉起来，再点「刷新」。';
  }
  // 拉列表失败被当成「这条会话不存在」报上来时，原文里的「可能未登录」会把人带偏。
  if (/在 Boss 会话列表里找不到这条会话/.test(text)) return fallback;
  return '';
}
/** 这条提示是否属于「重试即可」的技术失败（决定它用什么样式显示）。 */
function isRetryableNote(text: string): boolean {
  return navAbortOf(text, 'x') !== '';
}

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
/** 自动投递成功提示：绿色、2 秒后淡出（失败提示仍然常驻，等用户处理）。 */
const autoNoteOk = ref(false);
let autoNoteTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * 状态心跳（`lib_ui/auto-heartbeat.ts`）：60~90 秒随机，只受登录限制。
 * 这是本页**唯一**的 `auto_greet_status` 周期性来源——没有 1 秒轮询。
 * 投递完成、发完消息、手动刷新都会调 `beat.start()` 立刻主动请求一次。
 */
let beat: HeartbeatTimer | null = null;

/** `login-state` 的退订句柄（`subscribeLoginState`）；null = 没订阅上或已退订。 */
let unlistenLogin: UnlistenFn | null = null;
/** 组件已卸载：`subscribeLoginState` 是 await 的，回来时据此立刻退订。 */
let disposed = false;

/* ------------------------------ 窗口宽度（宽度公式） --------------------------
 * `--detach-w` / `--rail-w` 由 AppShell 的 syncCanvas() 写入（canvas.ts 是唯一公式来源），
 * 本页不写行内样式，`.chat-detach.is-open { flex-basis: var(--detach-w) }` 直接吃变量，
 * 静态兜底留在 vue-chat.css。这里只保留一份只读镜像给单测与调试。 */

const innerWidth = ref(typeof window === 'undefined' ? 1440 : window.innerWidth);
const detachW = computed(() => detachWidth(innerWidth.value));
const chatW = computed(() => chatAreaWidth(innerWidth.value));

/* ---------------------------------- 派生 ------------------------------------ */

const activeFriend = computed<Friend | null>(
  () => friends.value.find((f) => f.uid === ui.value.activeFriendsUid) ?? null,
);
// 排序口径统一在这里：草稿置顶 → 最近聊天在前 → 无时间戳的最后。
const visibleFriends = computed(() => filterFriends(sortFriends(friends.value), search.value));
const talkableFriend = computed(() => activeFriend.value);
const canTalk = computed(() => talkableFriend.value !== null && isSendableFriend(talkableFriend.value));
const composeRef = ref<InstanceType<typeof ChatCompose> | null>(null);

/* ---------------------------------- 加载 ------------------------------------ */

/**
 * 「该不该跳过远端拉取」的账本（模块级，整个页面生命周期有效）。不用 ref：它只影响要不要
 * 发请求，不参与渲染。
 */
const messagesMemo = makeMessagesMemo();

/**
 * 界面这屏消息属于哪条会话（空串 = 还没显示过任何人的会话）。与 `activeFriendsUid` 分开
 * 记：选中先变、消息要等 `loadMessages` 回来才换，重建会话时 uid 还会中途变一次。
 */
const feedUid = ref('');

async function loadFriends(allowOpenSession = false): Promise<void> {
  loadingFriends.value = true;
  friendsError.value = '';
  // 刷新前持久化的占位先并进列表（追加，不重建现有行）：后端报错（浏览器未连接）时它们也必须可见。
  const pending = loadPendingFriends();
  const known = new Set(friends.value.map((f) => f.uid));
  const fresh = pending.filter((f, i) => !known.has(f.uid) && pending.findIndex((g) => g.uid === f.uid) === i);
  if (fresh.length > 0) friends.value = [...fresh, ...friends.value];
  // 本地缓存的会话列表：库里有就直接先渲染，远程拉到之后按 uid 就地合并。
  // 不读它，界面上就只剩「正在加载 + 一句报错」，本地数据等于白存。
  // 判据是「还没有任何**真实**会话」：上面刚并进来的打招呼占位只是本机草稿，
  // 它不算内容——只拿一条占位就跳过缓存，用户看到的就是「本地数据没加载」。
  if (!friends.value.some((f) => !isPendingFriend(f))) {
    try {
      const cached = await chatFriendsLocal();
      if (cached.items.length > 0) {
        friends.value = mergeFriends(friends.value, cached.items);
      }
    } catch {
      /* 本地读失败不阻断远程加载 */
    }
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
    // 刷新不得改变展开态与选中：只把 activeFriend 的字段换成新的。
    // 顺序不变，不整体替换：远程行与现有行**按 uid 就地改写**，未变的行引用保持不变，
    // 只有真正变更的字段才触发那一行的重渲染：整表重建会让刷新卡住。
    friends.value = mergeFriends(friends.value, reply.items);
    savePendingFriends(friends.value);
    if (reply.items.length === 0 && !friends.value.some(isPendingFriend)) note.value = FRIENDS_EMPTY_NOTE;
    const uid = ui.value.activeFriendsUid;
    if (uid !== null && !friends.value.some((f) => f.uid === uid)) {
      ui.value = closeDetach(ui.value);
    }
  } catch (e) {
    friendsError.value = errText(e);
    // 浏览器未连接时 rail 已给出引导提示，不再把原始报错重复写进 note（否则两段文字叠在一起）。
    if (!needsSession(errText(e))) {
      note.value = navAbortOf(errText(e), RETRY_FRIENDS) || FRIENDS_FAIL_PREFIX + errText(e);
    }
  } finally {
    loadingFriends.value = false;
  }
}

async function loadMessages(uid: string): Promise<void> {
  loadingMessages.value = true;
  // 先展示本地持久化消息（不触发浏览器），再用远程结果增量合并。
  try {
    const local = await chatMessagesLocal(uid);
    // feedUid：界面显示的这屏消息属于哪条会话。与选中 uid 分开记，是因为
    // 「重建会话」会中途把 uid 换掉，此时屏上还是旧 uid 的那一屏。
    if (ui.value.activeFriendsUid === uid && feedUid.value !== uid && local.items.length > 0) {
      messages.value = seedLocalMsgIds(local.items, messages.value);
      feedUid.value = uid;
    }
  } catch {
    /* 本地读取失败不阻断远程加载 */
  }
  // 会话列表里的 last_ts 说明服务端认为这条会话没有新消息，且上次已成功拉过同样的值
  // → 直接用本地渲染，不打这一枪，否则点开每位 Boss 都要等 historyMsg。
  // 每次进来都按当前 uid 重取：重建会话会把这个 uid 换掉，闭包里留旧对象会重复重建。
  const friend = friends.value.find((f) => f.uid === uid) ?? null;
  // 判据只看「上次拉取成功时的 last_ts 与此刻是否相等」，与**此刻消息表是否为空无关**：
  // `pick()` 切换会话时会先清空 `messages`，再进来读缓存，那一刻必然是空的 ——
  // 加「length > 0」会让这个跳过分支永远不成立。
  // 空表的正常路径由下面「本地缓存 >0 才渲染」与本函数末尾的远端请求兜住。
  if (messagesMemo.shouldSkip(uid, friend?.last_ts)) {
    note.value = MESSAGES_FRESH_NOTE;
    loadingMessages.value = false;
    return;
  }
  try {
    const reply = await chatMessages(uid);
    if (ui.value.activeFriendsUid !== uid) return;
    // 远程结果**合并**而不是整体替换：按 msg_id 认同一行，乐观气泡原地保留。
    // 浏览器抓取慢是事实，但界面不该为它闪一下。
    messages.value = seedLocalMsgIds(mergeMessages(messages.value, reply.items), messages.value);
    feedUid.value = uid;
    // 只有成功拉取才记账：失败/空结果记账会让下次误判「已最新」而漏消息。
    messagesMemo.noteFetched(uid, friend?.last_ts);
  } catch (e) {
    if (ui.value.activeFriendsUid !== uid) return;
    // 这条会话的 uid 在 Boss 上已经不存在（列表没拉到也算这一类）：按岗位重新建一次会话，
    // 拿到真 uid 再抓，**不去列表里再找一遍**。
    // 重建成功后直接在本函数里按新 uid 再拉一次，**不递归**：新 uid 也落空时会无限重建。
    const rebound = await rebindDeadSession(friend, errText(e));
    if (rebound !== null) {
      ui.value = pickFriend(ui.value, rebound.uid);
      try {
        const again = await chatMessages(rebound.uid);
        if (ui.value.activeFriendsUid !== rebound.uid) return;
        messages.value = seedLocalMsgIds(mergeMessages(messages.value, again.items), messages.value);
        feedUid.value = rebound.uid;
        messagesMemo.noteFetched(rebound.uid, rebound.friend.last_ts);
        return;
      } catch (e2) {
        if (ui.value.activeFriendsUid !== rebound.uid) return;
        note.value = navAbortOf(errText(e2), RETRY_READ) || MESSAGES_FAIL_PREFIX + errText(e2);
        return;
      }
    }
    // 本地已有内容时不清屏：抓取失败不该把已展示的对话抹掉，只补一句提示。
    if (messages.value.length === 0) {
      note.value = navAbortOf(errText(e), RETRY_READ) || MESSAGES_FAIL_PREFIX + errText(e);
    }
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
  // 打开即已读：把水位抬到这条会话的最新时刻，红点与「[未读]」当场消失。
  if (friend !== null) {
    readMarks.value = saveReadMark(readMarks.value, friend.uid, friend.last_ts ?? 0);
  }
  if (switched) {
    messages.value = [];
    feedUid.value = '';
    greet.value = null;
    draft.value = '';
    note.value = '';
    // 占位会话（还没打过招呼的岗位）在 Boss 上**根本不存在**，直接拿它的假 uid
    // 去拉消息只会报「找不到这条会话」。先真去建会话，拿到服务端分配的 uid，
    // 再用真 uid 继续——占位行原地换掉。
    if (friend !== null && isPendingFriend(friend)) {
      // 建会话要真的去 Boss 上点「立即沟通」再等页面自己跳聊天页（实测 5–15 秒，最坏 ~80 秒）：
      // 选中不能卡在这一步。先把占位行留在列表里、挂上「建立中」提示就返回，
      // 用户可以接着点别的会话；真 uid 到了再回填这一行，人还停在这一行上就顺势进入。
      void ensureSession(friend).then((realUid) => {
        // 失败提示已由 `openSessionFor` 写好，占位行留着可以再点一次。
        if (realUid === '') return;
        // 用户已经点到别处：只回填这一行的 uid，不抢选中。
        if (ui.value.activeFriendsUid !== uid) return;
        ui.value = pickFriend(ui.value, realUid);
        // 拿到真 uid 后同样走 `loadMessages`（不递归 `pick`）：
        // uid 没变时递归进去 `switched` 为 false，会连消息都不拉。
        void loadMessages(realUid);
      });
      return;
    }
    await loadMessages(uid);
  }
}

/**
 * 后端报「在 Boss 会话列表里找不到这条会话」时重新拿一次真实 uid（列表没拉到或 uid 已失效
 * 都会这么报，唯一走得通的路是按岗位重建会话）。只认这一句文案，别的失败一律不动。
 * 返回新 uid 与换过 uid 的那一行；不是这种错误 / 没有岗位信息 / 重建失败时返回 `null`。
 */
async function rebindDeadSession(
  friend: Friend | null,
  text: string,
): Promise<{ uid: string; friend: Friend } | null> {
  if (friend === null || !/在 Boss 会话列表里找不到这条会话/.test(text)) return null;
  const jobId = friend.job_id ?? (isPendingFriend(friend) ? pendingJobId(friend) : '');
  if (jobId === '') return null;
  try {
    const reply = await chatOpen(jobId);
    const realUid = (reply.uid ?? '').trim();
    if (realUid === '') return null;
    // uid 是聊天页地址里的加密 id（与好友列表的数字 uid 不是一套），远程列表暂时认不出它，
    // 打上 `local_only` 让刷新不再把这一行吃掉；岗位 id 一并记下，供远程收录后比对。
    const rebound: Friend = { ...friend, uid: realUid, job_id: friend.job_id ?? jobId, local_only: true };
    friends.value = friends.value.map((f) => (f.uid === friend.uid ? rebound : f));
    savePendingFriends(friends.value);
    return { uid: realUid, friend: rebound };
  } catch {
    return null;
  }
}

/**
 * 正在后台建会话的行（key 是**建之前的占位 uid**，建好后这一行的 uid 就变了）。
 * 连点 / 双击同一行只发一次 `chat_open`：建一次会话要开浏览器点按钮，重复发就是重复开页面。
 */
const booting = new Map<string, Promise<string>>();

/** 建会话；同一行在飞时复用同一个 Promise，调用方可以放心 await。 */
function ensureSession(friend: Friend): Promise<string> {
  const running = booting.get(friend.uid);
  if (running !== undefined) return running;
  const task = openSessionFor(friend).finally(() => booting.delete(friend.uid));
  booting.set(friend.uid, task);
  return task;
}

/**
 * 把一条占位会话变成 Boss 上的真实会话，返回真实 uid（失败返回空串并写好提示）。
 * 「立即沟通」后跳转地址里已带着本次会话的 id，后端 `chat_open` 读的就是它，前端不做推断。
 */
async function openSessionFor(friend: Friend): Promise<string> {
  // 真实会话也带着岗位 id（`commands_chat.rs` 从库里落的 `job_id`）：uid 失效时
  // 同一套「立即沟通」同样能把它接回来，所以不只占位行能建会话。
  const jobId = friend.job_id ?? (isPendingFriend(friend) ? pendingJobId(friend) : '');
  if (jobId === '') {
    note.value = '这条会话缺少岗位信息，无法在 Boss 上建立会话';
    return '';
  }
  note.value = OPENING_SESSION_NOTE;
  let realUid = '';
  try {
    const reply = await chatOpen(jobId);
    realUid = (reply.uid ?? '').trim();
  } catch (e) {
    note.value = navAbortOf(errText(e), RETRY_READ) || OPEN_SESSION_FAIL_PREFIX + errText(e);
    return '';
  }
  if (realUid === '') {
    note.value = OPEN_SESSION_FAIL_PREFIX + 'Boss 没有返回会话标识';
    return '';
  }
  // 占位行原地换成真实会话：uid 变了，名字/公司沿用占位行（服务端还没回列表）。
  // `local_only`：`chat_open` 给的是聊天页地址里的加密 uid，与好友列表的数字 uid 不是一套，
  // 不打标记的话下一次 `chat_friends` 合并会把这行整条删掉，刚打过招呼的 Boss 会消失。
  friends.value = friends.value.map((f) =>
    f.uid === friend.uid ? { ...f, uid: realUid, job_id: f.job_id ?? jobId, local_only: true } : f,
  );
  savePendingFriends(friends.value);
  note.value = '';
  return realUid;
}

/** 收起：保留 activeFriend（A7）；再点同一位也能重开（A8）。 */
function close(): void {
  ui.value = closeDetach(ui.value);
}

/* ------------------------- Boss 列表行内操作（下沉） ------------------------- */

/**
 * 行内「打招呼」：先选中该行，再生成话术。必须先把选中态对齐，否则会给上一行的人生成话术；
 * 已选中时 `pick` 幂等（不重载消息），重复点同一行安全。
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
  // 话术要发得出去才算数：占位行没有真 uid，`chat_send` 无从下手，所以这里必须等会话建完。
  // 等待不影响界面操作——`pick` 已经把「建立中」提示挂上去了。
  const friend = friends.value.find((f) => f.uid === uid) ?? null;
  if (friend !== null && isPendingFriend(friend) && (await ensureSession(friend)) === '') return;
  await generateGreet();
}

/* -------------------------------- 岗位详情抽屉 -------------------------------- */

/**
 * 复用概览页那只 `JobDetailDrawer`：本页只负责按这一行的岗位 id 取数，抽屉自己画、自己补拉 JD。
 * 门槛 / 满分仍只来自 `get_scoring_spec`（C10），本页不写任何分数常量。
 */
const detailJob = ref<Job | null>(null);
const detailMinScore = ref<number | null>(null);
const detailScoreMax = ref<number | null>(null);
/** 抽屉是整窗浮层，挂在 `#app-overlay-root` 上（与概览页同一条路线）。 */
const overlayRoot = ref<HTMLElement | null>(null);

/** 这一行对应的岗位 id；占位会话的岗位记在本机 localStorage（`pendingJobId`）。 */
function friendJobId(friend: Friend): string {
  return friend.job_id ?? (isPendingFriend(friend) ? pendingJobId(friend) : '');
}

/**
 * 拿到这条会话的岗位 id；本地没有、但会话行自带 `encryptJobId` 时，去详情页把 JD
 * 补抓一次入库再返回。
 *
 * 只由点击触发（看岗位详情 / 生成话术），且幂等：已经在库的岗位后端直接返回，
 * 不重抓不覆盖。抓回来的 id 就地写回这一行，同一次会话里不会再补第二遍。
 */
async function ensureFriendJob(friend: Friend): Promise<string> {
  const local = friendJobId(friend);
  if (local !== '') return local;
  let enc = (friend.encrypt_job_id ?? '').trim();
  if (enc === '') {
    // 首屏那一份来自本地缓存（`chat_friends` 表存不下加密岗位 id），所以点开时向
    // Boss 现取一次会话列表补齐——等价于用户手点「刷新」，且只在这一行缺岗位时发生。
    const live = await chatFriends();
    const hit = live.items.find((f) => f.uid === friend.uid);
    const known = (hit?.job_id ?? '').trim();
    if (known !== '') {
      friend.job_id = known;
      return known;
    }
    enc = (hit?.encrypt_job_id ?? '').trim();
    if (enc === '') return '';
    friend.encrypt_job_id = enc;
  }
  const job = await jobFetchDetail(enc);
  friend.job_id = job.job_id;
  return job.job_id;
}

async function openFriendDetail(uid: string): Promise<void> {
  const friend = friends.value.find((f) => f.uid === uid) ?? null;
  if (friend === null) return;
  let jobId = friendJobId(friend);
  try {
    if (jobId === '') {
      // 会话行带着加密岗位 id：点开就去详情页补抓一次（有 JD 才能看详情、才能写话术）。
      note.value = '正在读取岗位详情…';
      jobId = await ensureFriendJob(friend);
      if (jobId === '') {
        note.value = '这条会话没有关联岗位，看不到岗位详情';
        return;
      }
      note.value = '';
    }
    if (detailScoreMax.value === null) {
      const spec = await getScoringSpec();
      detailScoreMax.value = spec.score_max;
      detailMinScore.value = spec.score_min_default;
    }
    const job = await getJob(jobId);
    if (job === null) {
      note.value = '这个岗位已不在库里（可能被清理过）';
      return;
    }
    detailJob.value = job;
  } catch (e) {
    note.value = `读取岗位详情失败：${errText(e)}`;
  }
}

function closeFriendDetail(): void {
  detailJob.value = null;
}

/**
 * 抽屉底部单条动作：与概览页共用同一批命令（载荷 `{args:{jobIds}}`）。
 * 本页没有列表快照要刷，动作成功后只关抽屉 + 一句提示。
 */
async function onFriendDetailAct(action: DetailAct, jobId: string): Promise<void> {
  if (jobId === '') return;
  const ids = [jobId];
  try {
    const hit =
      action === 'reject'
        ? await rejectJobs(ids)
        : action === 'allow'
          ? await allowJobs(ids)
          : action === 'undo-deliver'
            ? await unmarkDelivered(ids)
            : ((await markDelivered(ids)), ids.length);
    note.value = actionNote(action, hit);
    closeFriendDetail();
  } catch (e) {
    note.value = `${actionFailPrefix(action)}${errText(e)}`;
  }
}

/** 「去跟 Boss 聊」：抽屉里那一行的岗位 → 列表里同岗位的会话。 */
function onFriendDetailChat(jobId: string): void {
  closeFriendDetail();
  const hit = friends.value.find((f) => friendJobId(f) === jobId) ?? null;
  if (hit !== null) void pick(hit.uid);
}

/* ---------------------------------- 发送 ------------------------------------ */

/** 发出一条消息后，把会话列表里那一行改成「刚发过」（副行文本 + `[送达]`）。 */
function bumpRow(uid: string, text: string): void {
  friends.value = markFriendSent(friends.value, uid, text);
  // 本机新建的会话只在 localStorage 里活着，改了那一行要顺手落盘，否则刷新回到旧文案。
  savePendingFriends(friends.value);
}

async function send(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || !canSend(draft.value, sending.value)) return;
  const text = draft.value.trim();
  sending.value = true;
  // 只有发给「卡片打招呼的那位 Boss」时才带岗位 id，后端据此只标记这一个岗位已投递。
  // 在 try 外算：下面重建会话后的重发要用同一个值。
  const jobId = greetJob.value !== null && greetJob.value.uid === friend.uid ? greetJob.value.jobId : null;
  try {
    // 发送前先验证登录态：只有已登录才真正发出，否则给出对应提示。
    // sidecar 未启动时 loginStatus 本身会抛错：同样视为「浏览器未连接」，不落入「发送失败」。
    const st = await loginStatus('boss').catch(() => ({ state: 'unknown' as const }));
    if (st.state !== 'logged_in') {
      // 口径同 `onAutoToggle`：一律说「浏览器未连接：请先登录」。
      note.value = '浏览器未连接：请先登录';
      return;
    }
    const reply = await chatSend(friend.uid, text, jobId);
    const outgoing = String(reply.text ?? '') !== '' ? String(reply.text) : text;
    messages.value = seedLocalMsgIds([
      ...messages.value,
      { sender: 'me', content: outgoing, time: nowText(), msg_id: '', recalled: false },
    ], messages.value);
    draft.value = '';
    note.value = '消息已发送';
    // 左侧那一行就地改成「我刚发了这句」：副行文字 + `[送达]` 立刻出现，不必等重拉列表。
    bumpRow(friend.uid, outgoing);
    // 已用掉这次岗位标记：之后再发给同一 Boss 不得重复标记。
    if (jobId !== null) greetJob.value = null;
    // 发完消息主动请求一次自动投递状态（不计入 60~90 秒的心跳节拍）。
    if (beat !== null) void beat.start();
  } catch (e) {
    const text = errText(e);
    const moved = await rebindDeadSession(friend, text);
    if (moved !== null) {
      // 会话 uid 变了（旧 uid 在 Boss 上已经不存在）：选中态换到新 uid，再用新 uid 发一次。
      // 只重发这一层，`jobId` 沿用本次算出的值——重走 `send()` 会把岗位标记丢掉。
      ui.value = pickFriend(ui.value, moved.uid);
      try {
        const reply = await chatSend(moved.uid, text, jobId);
        const outgoing = String(reply.text ?? '') !== '' ? String(reply.text) : text;
        messages.value = seedLocalMsgIds([
          ...messages.value,
          { sender: 'me', content: outgoing, time: nowText(), msg_id: '', recalled: false },
        ], messages.value);
        draft.value = '';
        note.value = '消息已发送';
        bumpRow(moved.uid, outgoing);
        if (jobId !== null) greetJob.value = null;
        if (beat !== null) void beat.start();
        return;
      } catch (e2) {
        const t2 = errText(e2);
        note.value = navAbortOf(t2, RETRY_SEND) || SEND_FAIL_PREFIX + t2;
        return;
      }
    }
    note.value = navAbortOf(text, RETRY_SEND) || SEND_FAIL_PREFIX + text;
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
  // 岗位 id 两个来源：从概览页卡片打招呼时后端记在 `greetJob`（会话可能还是占位行，
  // 身上没有 `job_id`），从聊天页对已有会话打招呼时用 `friend.job_id`。
  const jobId =
    greetJob.value !== null && greetJob.value.uid === friend.uid
      ? greetJob.value.jobId
      : (friend.job_id ?? null);
  try {
    // 带上岗位 id：后端发成功后顺带标「已投递」，否则这条岗位还留在未投递里。
    await greetOne(friend.uid, text, jobId);
    messages.value = seedLocalMsgIds([
      ...messages.value,
      { sender: 'me', content: text, time: nowText(), msg_id: '', recalled: false },
    ], messages.value);
    greet.value = null;
    // 打招呼成功即把对方挂进 Boss 列表：占位会话（无后端会话）此时才真正入列。
    if (isPendingFriend(friend)) {
      friends.value = withPending(
        friends.value.filter((f) => f.uid !== friend.uid),
        friend,
      );
      savePendingFriends(friends.value);
    }
    // 打招呼也是发消息：同样让那一行立刻显示 `[送达]`（否则只有右边对话有气泡）。
    bumpRow(friend.uid, text);
    // 这次岗位标记已经用掉：之后再对同一 Boss 发消息不得重复标记。
    if (jobId !== null) greetJob.value = null;
    note.value = GREET_SENT_NOTE;
  } catch (e) {
    note.value = `发送失败：${errText(e)}`;
  } finally {
    sendingGreet.value = false;
  }
}

/**
 * 生成打招呼话术。`reply` 非空时走「根据 HR 回复生成」。`Friend` 里没有 `jd_text`，
 * JD 由后端按岗位 id 从库里取：优先概览页带过来的 `greetJob`，否则用这条会话自己
 * 精确关联的 `job_id`（`boss:<encryptJobId>`）。两者都没有就不带 JD 组提示词，回执
 * `jd_used=false`——**不再按公司名翻库**：同一家公司多个岗位必然挑错（用户 m00720）。
 */
async function generateGreet(reply = ''): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || generating.value) return;
  generating.value = true;
  try {
    greet.value = await greetDraft({
      // 没有本地岗位时先补抓一次 JD：空 JD 写出来的话术必然不对（用户 m00833）。
      jobId:
        greetJob.value?.uid === friend.uid ? greetJob.value.jobId : await ensureFriendJob(friend),
      title: friend.title,
      company: friend.company,
      salary: '',
      experience: '',
      jd: '',
      reply,
    });
    // 这次生成花了 token：让概览把累计重读一遍（顶栏读数当场跳一下）。
    markTokensSpent();
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
  // 用 `?? []` 兜住 undefined：测试里 ChatDetach 可能拿到未传的 `messages`，
  // 原来这里抛 `Cannot read properties of undefined (reading 'length')`，
  // 被 Vue 记为「渲染期间未捕获错误」，长期污染测试输出。
  const list = messages.value ?? [];
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const m = list[i];
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

/**
 * 输入框上方「AI 生成」：生成的句子**直接落进输入框**，不另开草稿框（右栏「生成回复」则留在
 * 草稿框等确认）。提示词三件套：Boss 回复上下文走 `aiReplyContext` 的 `reply` 字段；JD 由后端
 * 按公司名翻库拿整段；我的技能后端从配置画像取，前端不碰。
 */
async function generateAiDraft(): Promise<void> {
  const friend = talkableFriend.value;
  if (friend === null || generating.value) return;
  generating.value = true;
  try {
    const reply = await greetDraft({
      // 同上：没有本地岗位时先补抓 JD，提示词才有正文可用。
      jobId:
        greetJob.value?.uid === friend.uid ? greetJob.value.jobId : await ensureFriendJob(friend),
      title: friend.title,
      company: friend.company,
      salary: '',
      experience: '',
      jd: '',
      reply: aiReplyContext(messages.value ?? []),
    });
    // 同「生成打招呼」：回执一到就通知概览重读累计（空串回执也可能已经花掉了 token）。
    markTokensSpent();
    const text = reply.text.trim();
    if (text === '') {
      // 规则兜底与模型都可能给出空串：不把空内容塞进输入框，也不清掉用户已打的字。
      note.value = AI_DRAFT_FAIL_PREFIX + (reply.note || '模型没有返回内容');
      return;
    }
    draft.value = text;
    // 回执里的降级原因照实说：走的是规则话术时用户要知道。
    if (reply.note !== '') note.value = reply.note;
    composeRef.value?.focus();
  } catch (e) {
    note.value = AI_DRAFT_FAIL_PREFIX + errText(e);
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
    // uid 后缀必须是**岗位 id**（`pending:<job.id>`）：`openSessionFor` 会把这段当作
    // `chat_open` 的 jobId 去 Boss 上建会话，填公司名必然建不出来。
    const placeholder = pendingFriend(intent.jobId || intent.company, intent.company);
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
  const t = text.trim();
  if (t === '') return;
  autoNote.value = t;
  note.value = t;
  if (autoNoteTimer !== null) {
    clearTimeout(autoNoteTimer);
    autoNoteTimer = null;
  }
  // 心跳成功时写的是「自动投递已发出一条」；其余都是要人处理的失败/引导语。
  autoNoteOk.value = t.startsWith('自动投递已发出');
  if (!autoNoteOk.value) return;
  // 成功只是回执：绿条留 2 秒（CSS 同步淡出），随后清掉，避免一直挂在右栏。
  autoNoteTimer = setTimeout(() => {
    autoNoteTimer = null;
    autoNoteOk.value = false;
    if (autoNote.value === t) autoNote.value = '';
    if (note.value === t) note.value = '';
  }, 2000);
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
      // `unknown` = 浏览器（sidecar）还没拉起来；`no_login` 等 = 连上了但没登录。
      // 两种情况都要拦下来，但**说法不能一样**：库里还有登录标记时，
      // 顶栏显示「已登录」，这时说「请先登录」等于让人去点已经写着已登录的按钮。
      if (!isLoggedIn(st.state)) {
        autoNote.value = st.state === 'unknown' && st.saved === true
          ? BROWSER_DETACHED_NOTE
          : '浏览器未连接：请先登录';
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
  // Esc 分层：一次 keydown 只关最上面那一层。
  // 决策记录 / 配置 / 岗位抽屉 / 敏感库开着时，这层不出手（优先级表单源在
  // `lib_ui/overlay-stack.ts`，四个消费方共用，不在各自文件里抄选择器）。
  if (hasHigherOverlay('chat')) return;
  ev.stopPropagation();
  close();
}

/**
 * 登录态订阅：登录成功后「未登录 / 浏览器未连接」提示必须消失，直到再次未登录才重现。
 * `beat.start()` 只在挂载、开始投递、发消息时各读一次登录态，之后没人去清，所以由后端
 * `login-state` 旁路推送在这里收口。非 Tauri 环境 `listenLoginState` 返回 no-op。
 */
async function onLoginState(payload: { platform?: string; state: string }): Promise<void> {
  // watcher 每 500ms 读一次快照，**只在值真的变了**才播；`unknown` 是降级值（sidecar 不在线
  // 读不到快照），会话抖起来会播出一串 unknown → waiting → unknown，每一次都白跑一遍下面这套
  // 流程。所以先按「已知态」过滤：未知态不是结论，不该驱动任何后续动作。
  if (!isKnownState(payload.state)) return;
  // 只有「从没登录」或「登录态真的变了」才值得动状态机：同态重复推送直接返回。
  if (payload.state === lastLoginState) return;
  lastLoginState = payload.state;
  // 登录态确认变化 → 清掉登录类提示（`waiting` / `logged_in` / `closed` 都说明浏览器已经能
  // 答话）。只清登录类，不能连「发送失败：……」一起抹掉——那是与登录无关的真错误。
  const clearedNote = isSessionNote(note.value);
  const clearedAuto = isSessionNote(autoNote.value);
  if (clearedNote) note.value = '';
  if (clearedAuto) autoNote.value = '';
  // 广播已消费：回执给后端，全员回话后它就不再每 5 秒补发。
  // 放在清理之后、开表之前：开表可能重试几秒，没必要让后端陪着等。
  // 平台优先取事件里的原值，缺了才退回本页写死的 'boss'，免得确认对不上轮次。
  void loginAck(payload.platform ?? 'boss', payload.state);
  if (isLoggedIn(payload.state)) {
    // 重新开表：`start()` 会再读一次登录态，但它读不到时会**立刻把引导语写回来**。广播不是
    // 可靠传输（订阅晚于广播 / WebView 在后台丢事件都会白丢），所以这里带确认：开表成功且提示
    // 确实清掉才算办成，否则退避重试，最多 3 次。
    await withLoginAck(
      async () => {
        if (disposed) return true;
        if (beat !== null) await beat.start();
        const dirty = isSessionNote(note.value) || isSessionNote(autoNote.value);
        if (dirty) {
          if (clearedNote) note.value = '';
          if (clearedAuto) autoNote.value = '';
          beat?.stop();
          return false;
        }
        return beat !== null && beat.running();
      },
      {
        onGiveUp: () => {
          // 3 次都没落账：把表停下，让用户点一次刷新重新判，不偷偷起定时器。
          beat?.stop();
        },
      },
    );
    return;
  }
  // 掉登录 / 登录窗口关闭：重新判一次，由 `beat.start()` 自己写出对应的引导语。
  if (beat !== null) await beat.start();
}

/**
 * 这条提示是不是「登录类」（未登录 / 浏览器未连接 / 浏览器未启动）。判据用文案前缀而不是
 * 错误码：引导语都是前端常量，登录成功后要成对清掉，但不能误伤其它错误。新增文案时加进
 * `LOGIN_NOTE_PREFIXES`。
 */
const LOGIN_NOTE_PREFIXES: readonly string[] = [
  '浏览器未连接',
  '浏览器未启动',
  '未登录',
  '登录已失效',
];

function isSessionNote(text: string): boolean {
  const t = text.trim();
  // `needsSession` 兜的是「尚未启动 / 没有会话」这类 sidecar 文案：它们同样是
  // 「浏览器没连上」，登录成功后必须一起消失。
  if (needsSession(t)) return true;
  return LOGIN_NOTE_PREFIXES.some((p) => t.startsWith(p));
}

/**
 * 上一次**确认**过的登录态（只记已知态）。用来挡住重复推送与 `unknown` 抖动，否则会白跑一遍
 * 「重开心跳」，表现为 `login_status` 刷屏。空串 = 还没收到过任何已知态。
 */
let lastLoginState = '';

/** 重试是否在跑：只挡重复点击，不锁其它交互。 */
const checkingBrowser = ref(false);

/**
 * 只读一次登录态：不拉浏览器、不发起登录，纯粹确认「连上了没 / 登录还在不在」。
 * 连上了就清掉登录类提示并让心跳重新开表；仍连不上就把原因写成最新的一句。
 */
async function retryBrowserCheck(): Promise<void> {
  if (checkingBrowser.value) return;
  checkingBrowser.value = true;
  try {
    const st = await loginStatus('boss').catch(() => ({ state: 'unknown', saved: false }));
    const state = String(st.state ?? 'unknown');
    if (isLoggedIn(state)) {
      if (isSessionNote(note.value)) note.value = '';
      if (isSessionNote(autoNote.value)) autoNote.value = '';
      lastLoginState = state;
      await beat?.start();
      return;
    }
    autoNote.value = '';
    note.value = state === 'unknown' && st.saved === true ? BROWSER_DETACHED_NOTE : NOT_LOGGED_IN_NOTE;
  } finally {
    checkingBrowser.value = false;
  }
}

/** 只有「浏览器/登录」类提示才配重试：真错误各自有刷新路径。 */
const noteNeedsCheck = computed(() => note.value !== '' && isSessionNote(note.value));
const autoNoteNeedsCheck = computed(() => extraNote.value !== '' && isSessionNote(extraNote.value));

async function subscribeLoginState(): Promise<void> {
  if (unlistenLogin !== null) return;
  try {
    const off = await listenLoginState(
      (payload) => void onLoginState(payload),
      () => undefined,
    );
    // 订阅是 await 的，期间组件可能已卸载：立刻退订，别留悬挂监听。
    if (disposed) {
      off();
      return;
    }
    unlistenLogin = off;
  } catch {
    // 订阅失败只影响实时性：挂载、开始投递、发消息三处仍各查一次。
    unlistenLogin = null;
  }
}

onMounted(() => {
  overlayRoot.value = document.querySelector('#app-overlay-root');
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
    // 未登录 / 浏览器未连接则不开表，只给一句引导。这句引导随后由
    // `subscribeLoginState` 在真的登录成功时清掉。
    beat = startHeartbeat({
      onStatus(st) {
        autoStatus.value = st;
      },
      onSending(v) {
        autoSending.value = v;
      },
      onNote: onAutoNote,
      onSent(jobId) {
        markJobSent(jobId);
      },
    });
    await beat.start();
    await subscribeLoginState();
    // 开表后若仍未登录，`beat.start()` 已把引导语写进 `note`/`autoNote`；
    // 之后登录成功由 `onLoginState` 收到广播清掉，不再起轮询兜底。
    // 后台已在跑的投递必须当场显示「暂停」。只在**一点都没读到**时补读：
    // `beat.start()` 成功时已经调过 `autoGreetStatus` 并 `onStatus` 了，
    // 这里再打一发就是同一毫秒里的第二次请求。
    if (beat.lastStatus() === null) {
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
  disposed = true;
  if (autoNoteTimer !== null) {
    clearTimeout(autoNoteTimer);
    autoNoteTimer = null;
  }
  window.removeEventListener('resize', onResize);
  window.removeEventListener('keydown', onKeydown);
  if (unlistenLogin !== null) {
    unlistenLogin();
    unlistenLogin = null;
  }
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
    // 别抢在 pick() 前面再拉一次：pick() 自己会拉，这里只补「收起后又重开」那一档。
    // 少了这个守卫，点开会连发两条 chat_messages，那条会话若已失效就会重建两次会话。
    if (loadingMessages.value) return;
    if (feedUid.value !== friend?.uid) return;
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
  topNote,
  bottomNote,
  freshNote,
  extraNote,
  noteTone,
  friends,
  messages,
  feedUid,
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
      :unread="unreadSessions(friends, readMarks)"
      :read-marks="readMarks"
      :friends="visibleFriends"
      :active-uid="ui.activeFriendsUid"
      :loading="loadingFriends"
      :error="friendsError"
      :search="search"
      @pick="pick"
      @greet="greetFor"
      @detail="openFriendDetail"
      :removable-uids="removableUids"
      @remove="removePending"
      @refresh="loadFriends"
      @retry="loadFriends"
      @search="search = $event"
    >
    </FriendList>
    <!-- 右栏 aria-live 汇报区：与对话框里的 chat-detach-state-announce 同源，文案都由常量给出。 -->
    <span class="sr-only" data-testid="chat-rail-state-announce" aria-live="polite">{{
      CHAT_STATE_ANNOUNCE_TEXT
    }}</span>

    <!-- 两条瞬态提示行：发送失败 / 会话启动的反馈依赖它。
         展开态不渲染这条，对话框里的 #notices 会再渲染一份同源提示，两处必须**互斥**——
         ChatDetach 收起时用 [hidden] 改成零宽盒（motion.css）而不是 display:none，不互斥时
         两份会落在同一屏上，看起来就是同一句提示出现了两次。 -->
    <template v-if="!ui.chatOpen">
      <!-- 「已是最新消息」不再出现在列表头顶（用户要求撤掉那条灰条），
           也不落到下面这条红底瞬态条上（`freshNote` 在这里只负责让位）。 -->
      <div
        v-if="note !== '' && freshNote === ''"
        class="cfg-hint chat-note"
        :class="{ 'is-bad': noteTone === 'bad', 'is-ok': autoNoteOk && note === autoNote }"
        data-testid="chat-note"
      >
        <span class="note-text">{{ note }}</span>
        <button
          v-if="noteNeedsCheck"
          class="btn btn-sm btn-ghost chat-note-retry"
          type="button"
          data-testid="chat-note-retry"
          title="只重新检查一次浏览器连接状态，不会重新登录"
          :disabled="checkingBrowser"
          @click="retryBrowserCheck"
        >{{ checkingBrowser ? '检查中…' : '重试' }}</button>
      </div>
      <!-- `onAutoNote` 会把同一句话同时写进 `note` 与 `autoNote`（那一行是给对话框
           #notices 与右栏共用一份来源），所以这里必须去重：内容相同时只留上面那条，
           否则右栏会上下并排出现两条一模一样的提示。去重口径见 `extraNote`：
           不只是「一模一样」，登录/连接类说同一件事的两句话也只留一条。 -->
      <div
        v-if="extraNote !== ''"
        class="cfg-hint chat-note"
        :class="{ 'is-bad': autoNoteBad, 'is-ok': autoNoteOk }"
        data-testid="auto-note"
      >
        <span class="note-text">{{ extraNote }}</span>
        <button
          v-if="autoNoteNeedsCheck"
          class="btn btn-sm btn-ghost chat-note-retry"
          type="button"
          data-testid="auto-note-retry"
          title="只重新检查一次浏览器连接状态，不会重新登录"
          :disabled="checkingBrowser"
          @click="retryBrowserCheck"
        >{{ checkingBrowser ? '检查中…' : '重试' }}</button>
      </div>
    </template>
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
        :unread="unreadSessions(friends, readMarks)"
        :read-marks="readMarks"
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
        @detail="openFriendDetail"
        :removable-uids="removableUids"
        @remove="removePending"
      >
      </FriendList>
    </template>

    <ChatThread
      :friend="talkableFriend"
      :messages="messages"
      :loading-messages="loadingMessages"
      :greet="greet"
      :generating="generating"
      :sending-greet="sendingGreet"
      :recalling="recalling"
      :has-hr-reply="lastHrMessage.length > 0"
      @generate-greet="regenerateGreet"
      @generate-reply="generateReply"
      @recall="recall"
      @send-greet="sendGreet"
      @greet-to-draft="greetToDraft"
      @discard-greet="greet = null"
    >
      <template #notices>
        <!-- 顶部只挂真错误（红条）：右栏的 note / autoNote 在对话框展开时看不到，这里同源再渲染一份。
             灰色说明不占顶部，见下方 #footer。 -->
        <div v-if="topNote !== '' || extraNote !== ''" class="detach-notes">
          <div
            v-if="topNote !== ''"
            class="detach-note is-bad"
            data-testid="chat-note-detach"
          >
            <span class="note-text">{{ topNote }}</span>
            <button
              v-if="noteNeedsCheck"
              class="btn btn-sm btn-ghost chat-note-retry"
              type="button"
              data-testid="chat-note-detach-retry"
              title="只重新检查一次浏览器连接状态，不会重新登录"
              :disabled="checkingBrowser"
              @click="retryBrowserCheck"
            >{{ checkingBrowser ? '检查中…' : '重试' }}</button>
          </div>
          <div
            v-if="extraNote !== ''"
            class="detach-note"
            :class="{ 'is-bad': autoNoteBad, 'is-ok': autoNoteOk }"
            data-testid="auto-note-detach"
          >
            <span class="note-text">{{ extraNote }}</span>
            <button
              v-if="autoNoteNeedsCheck"
              class="btn btn-sm btn-ghost chat-note-retry"
              type="button"
              data-testid="auto-note-detach-retry"
              title="只重新检查一次浏览器连接状态，不会重新登录"
              :disabled="checkingBrowser"
              @click="retryBrowserCheck"
            >{{ checkingBrowser ? '检查中…' : '重试' }}</button>
          </div>
        </div>
      </template>
      <template #footer>
        <!-- 消息流末尾的说明条：不住在输入框上方，
             而是跟着内容走、紧接最后一条气泡，灰色小字居中。
             这是「已是最新消息」唯一的落点。 -->
        <div v-if="bottomNote !== ''" class="detach-hint" data-testid="chat-detach-hint">{{ bottomNote }}</div>
      </template>
      <template #compose>
        <!-- 唯一输入框：显式绑 modelValue + update（不走 v-model 的模板展开）。 -->
        <ChatCompose
          v-if="canTalk"
          ref="composeRef"
          :model-value="draft"
          :sending="sending"
          :generating="generating"
          @update:model-value="draft = $event"
          @send="send"
          @generate="generateAiDraft"
        />
      </template>
    </ChatThread>
  </ChatDetach>

  <!-- 岗位详情抽屉：全窗浮层，必须挂在没有 overflow / transform 的浮层根上，
       否则会被页面滚动容器裁进一列（与概览页同一处坑）。 -->
  <Teleport to="#app-overlay-root" :disabled="overlayRoot === null">
    <JobDetailDrawer
      :job="detailJob"
      :open="detailJob !== null"
      :min-score="detailMinScore"
      :score-max="detailScoreMax"
      @close="closeFriendDetail"
      @act="onFriendDetailAct"
      @chat="onFriendDetailChat"
    />
  </Teleport>
</template>
