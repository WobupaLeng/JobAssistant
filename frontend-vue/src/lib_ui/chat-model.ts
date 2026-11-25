/**
 * Boss 消息页的纯逻辑层（无 IO、无 DOM）。
 *
 * 全部规则对齐后端聊天命令的返回口径，
 * 差异只有一处且必须记住：**页面 DOM 口径按冻结原型 v3 走**
 * （`prototypes/ui-v1` + `docs/ui/UI-SPEC-chat-detach-v3.md`），
 * 不是整页 `.chat-grid` 布局。行为规则一致，结构不同：
 *
 * - 收起态：右栏 `aside.chat-rail` 常驻（Boss 列表），对话框 `.chat-detach` 带 `hidden`；
 * - 展开态：点任意 Boss ⇒ 对话框展开（宽 `clamp(600px, 50vw, 80vw)`），
 *   右栏让位，对话框内含「左 Boss 列表段 + 右聊天区」；
 * - 收起保留 `activeFriend`（SPEC §3 状态机 A7），再点同一位可重开。
 *
 * 这里只放能在 node 里单测的东西：状态机、宽度公式、筛选、气泡分型、文案。
 */
import type { Friend, ChatMessage } from '../api/types';
import { computeRailWidth } from './canvas';
import { needsSession } from './session';

/** 对话框宽度下限（v10 裁决 U，用户 m00332：「把下限降到 600px」）。 */
export const DETACH_MIN_W = 600;
/** 50% 口径 —— 与下限一起构成 `clamp(600px, 50vw, 80vw)` 的下半段。 */
export const DETACH_VW = 0.5;
/** 上限 80vw，防大屏过宽。 */
export const DETACH_MAX_VW = 0.8;
/** 右栏基线宽：≥1280 为 300，1200–1279 为 260（legacy §2.3，保留）。 */
export const RAIL_W_WIDE = 300;
export const RAIL_W_NARROW = 260;
/** 折叠断点：窗口 < 1280 时右栏收窄到 260。 */
export const RAIL_NARROW_BELOW = 1280;

/** 打招呼占位会话的 uid 前缀（无匹配好友时插一条临时会话）。 */
export const PENDING_UID_PREFIX = 'g';
/**
 * 占位会话 uid 的**完整标记**：`pendingFriend()` 生成的是 `g` + 岗位标识，
 * 而 Boss 真实会话的 uid 本身常常就以 `g` 开头（加密 uid）。
 *
 * 只用前缀 `'g'` 判定，会把库里的真实会话整批判成占位：`isSendableFriend` 全部返回
 * false（发不出消息），`loadPendingFriends` 也把它们当草稿读回来（用户报「没有读本地
 * 数据」，本地列表多出一堆来历不明的行）。所以这里的判据必须比前缀更严。
 */
export const PENDING_UID_MARK = 'pending:';
/** 未读数封顶显示。 */
export const UNREAD_CAP = 99;
/**
 * 自动投递**没有**轮询间隔：恒为 undefined，保留导出供契约测试断言。
 *
 * 状态请求全由 60~90 秒随机心跳与用户操作驱动（见 `auto-heartbeat.ts`）。
 */
export const AUTO_TICK_MS = undefined;

/**
 * 对话框宽度公式（SPEC §2.2 / §16.6，A49）。
 *
 * `Math.round(Math.min(Math.max(innerWidth * 0.5, 600), innerWidth * 0.8))`
 *
 * 四档验收：1200→600 / 1280→640 / 1440→720 / 1600→800。
 */
export function detachWidth(innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) return DETACH_MIN_W;
  const half = innerWidth * DETACH_VW;
  const capped = Math.min(Math.max(half, DETACH_MIN_W), innerWidth * DETACH_MAX_VW);
  return Math.round(capped);
}

/**
 * 右栏（= 对话框内左段 Boss 列表）宽度，**已乘 `--ui-scale`**（用户裁决 K/v7）：
 * 大屏上右栏要和字号一起变宽，否则 3840 宽度下 13px→19.5px 的字挤在 300px 栏里。
 *
 * 公式只有一份，在 `canvas.ts:computeRailWidth`；这里保留旧名字，
 * 免得 `ChatPage` 与既有单测跟着改（值域同时改变：见 `tests/chat-model.test.ts`
 * 的 `railWidth` 断言）。
 */
export function railWidth(innerWidth: number): number {
  return computeRailWidth(innerWidth);
}

/** 聊天区可用宽 = 对话框宽 − 左段宽（§2.2「≥ 320 为舒适下限」）。 */
export function chatAreaWidth(innerWidth: number): number {
  return detachWidth(innerWidth) - railWidth(innerWidth);
}

/* ------------------------------------------------------------------ *
 * 展开 / 收起状态机（SPEC §3，A1/A3/A5/A7/A8/A10）
 * ------------------------------------------------------------------ */

export interface ChatUiState {
  /** 对话框是否展开。`false` 时 `.chat-detach` 带 hidden。 */
  chatOpen: boolean;
  /** 当前选中好友 uid；**收起不清空**（A7）。 */
  activeFriendsUid: string | null;
}

export function createChatUi(): ChatUiState {
  return { chatOpen: false, activeFriendsUid: null };
}

/**
 * 点某位 Boss。收起态点 ⇒ 选中并展开；展开态点 ⇒ 只切人。
 * A8：收起后再点**同一**位也必须重新展开。
 */
export function pickFriend(state: ChatUiState, uid: string): ChatUiState {
  return { chatOpen: true, activeFriendsUid: uid };
}

/** 收起对话框（close-btn 或 Esc）：**保留** activeFriend。 */
export function closeDetach(state: ChatUiState): ChatUiState {
  return { chatOpen: false, activeFriendsUid: state.activeFriendsUid };
}

/* ------------------------------------------------------------------ *
 * 会话列表
 * ------------------------------------------------------------------ */

/** 搜索命中的判据：name / company / title / last_msg 任一包含（大小写无关）。 */
export function matchFriend(friend: Friend, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  const hay = [friend.name, friend.company, friend.title, friend.last_msg];
  return hay.some((field) => String(field ?? '').toLowerCase().includes(q));
}

export function filterFriends(friends: readonly Friend[], query: string): Friend[] {
  return friends.filter((f) => matchFriend(f, query));
}

/** 列表副行：最近一条消息 → 岗位名 → 「（暂无消息）」。 */
export function friendLine(friend: Friend): string {
  const last = (friend.last_msg ?? '').trim();
  if (last !== '') return last;
  const title = (friend.title ?? '').trim();
  if (title !== '') return title;
  return '（暂无消息）';
}

/** 列表副行的语音：`unread` 未读（对方发的且未读）、`read` 已读、`sent` 已送达、`plain` 无状态词。 */
export type FriendMsgTone = 'unread' | 'read' | 'sent' | 'plain';

/**
 * 副行该用哪种颜色（用户 m01427：已送达 / 已读 / 未读 显示不同颜色）。
 *
 * 判据只有两条，都来自 Boss 原始数据，不猜：
 * - `unread > 0` → 对方有新消息没读 → 未读色（最重）；
 * - 否则看 `last_from_me` 为真时的 `last_status`：`2` 已读、`1` 已送达。
 *
 * 对方发的最后一条**不加**状态词：Boss 原文也只给我的消息标 [已读]/[送达]，
 * 对方的最后一条本就默认已读，标出来只是噪声。
 */
export function friendMsgTone(friend: Friend): FriendMsgTone {
  if ((friend.unread ?? 0) > 0) return 'unread';
  if (friend.last_from_me !== true) return 'plain';
  const status = friend.last_status ?? 0;
  if (status === 2) return 'read';
  if (status === 1) return 'sent';
  return 'plain';
}

/**
 * 标题旁的计数：**未读消息条数**之和，不是会话数（用户 m01427）。
 *
 * 会话数在这里没有意义 —— 用户问的是「还有多少条没看」。全为 0 时返回 0，
 * 由调用方决定不渲染（空态不显示 0）。
 */
export function unreadTotal(friends: readonly Friend[]): number {
  let sum = 0;
  for (const f of friends) sum += Math.max(0, f.unread ?? 0);
  return sum;
}

/** 列表名字兜底：没名字就显示 uid。 */
export function friendName(friend: Friend): string {
  const name = (friend.name ?? '').trim();
  return name !== '' ? name : friend.uid;
}

/** 未读角标文案：0 时调用方不渲染；>99 显示 99+。 */
export function unreadText(n: number): string {
  return n > UNREAD_CAP ? `${UNREAD_CAP}+` : String(n);
}

/** 右栏头部的搜索占位（§5 逐字）。两段列表共用。 */
export const CHAT_SEARCH_PLACEHOLDER = '搜索 HR / 公司 / 消息';
/** 空态逐字（SPEC §5）。 */
/* 原型 L2342 逐字「没有会话」（不是「还没有会话」）。 FriendList 必须消费这个常量，
 * 不在组件里另写一份字面量（模块 §2 单源）。 */
export const CHAT_EMPTY_TITLE = '没有会话';
export const CHAT_DETACH_EMPTY_TITLE = '在左侧 Boss 列表点一位，开始聊天';
export const CHAT_DETACH_EMPTY_SUB = '点左侧 Boss 列表里的任意一位即可查看聊天';
/** 收起按钮逐字（用户 m04331：两字「收起」；原四字「收起对话框」在竖排胶囊里溢出）。
 * 模板里它被拆成两段，竖排自上而下摆成「收 ⟩ 起」。 */
export const CHAT_CLOSE_LABEL = '收起';
/** 输入框占位与发送按钮逐字（§5）。 */
export const CHAT_COMPOSE_PLACEHOLDER = '输入消息…';
export const CHAT_SEND_LABEL = '发送';
/** 输入框上方「AI 生成」按钮逐字（用户 m01148：发送按钮的上方加上 AI 生成）。 */
export const CHAT_AI_LABEL = 'AI 生成';
export const CHAT_AI_BUSY_LABEL = '生成中…';
/** 右栏标题逐字（§5）。 */
export const CHAT_RAIL_TITLE = 'BOSS 消息';
export const CHAT_DETACH_TITLE = 'Boss 消息';
export const CHAT_DETACH_RAIL_TITLE = 'Boss 列表';
/** 两处 aria-live 汇报区共用同一句（右栏 `chat-rail-state-announce` 与
 * 对话框 `chat-detach-state-announce`），避免两处字面量各自漂移。 */
export const CHAT_STATE_ANNOUNCE_TEXT = '会话列表';

/* ------------------------------------------------------------------ *
 * 线程
 * ------------------------------------------------------------------ */

export type SenderSlug = 'me' | 'system' | 'hr';

/** 发送方归类：me/self→me，system/typing→system，空→hr，其余原样。 */
export function senderSlug(raw: string): SenderSlug | string {
  const s = String(raw ?? '').trim().toLowerCase();
  if (s === 'me' || s === 'self') return 'me';
  if (s === 'system' || s === 'typing') return 'system';
  if (s === '') return 'hr';
  return s;
}

/** 「对方正在输入」不是气泡，渲染成 typing 行（后端已在 `load_messages` 里拆好）。 */
export function isTypingMessage(msg: ChatMessage): boolean {
  return senderSlug(msg.sender) === 'system' && String(msg.content ?? '').includes('正在输入');
}

export interface ThreadSplit {
  bubbles: ChatMessage[];
  typing: boolean;
}

export function splitThread(messages: readonly ChatMessage[]): ThreadSplit {
  const bubbles: ChatMessage[] = [];
  let typing = false;
  for (const m of messages) {
    if (isTypingMessage(m)) {
      typing = true;
    } else {
      bubbles.push(m);
    }
  }
  return { bubbles, typing };
}

/** 本会话内自增的乐观消息序号：只用于生成稳定 key，丢一轮也不会出错（仅是 key）。 */
let localMsgSeq = 0;

/**
 * 给**没有 `msg_id` 的乐观气泡**补一个会话内稳定的 `local_id`，其余原样返回。
 *
 * 为什么要：`ChatThread` 的 key 曾是 `msgTestId(m) ?? `local-${i}``，下标是随时会变的，
 * 远程结果 / 前面插入一条都会让整表重 diff（用户报「点 Boss 看对话卡住」）。
 * 这里按「无 msg_id ⇒ 身份 = 发送方 + 时刻 + 正文」在**上一次的同一列表**里回填旧 id：
 * 同一批数组重新 map 出来的新对象也能认出来，后续追加不会改动已有条目的 id。
 */
export function seedLocalMsgIds(
  messages: readonly ChatMessage[],
  previous: readonly ChatMessage[] = [],
): ChatMessage[] {
  const reuse = new Map<string, string>();
  for (const m of previous) {
    if (String(m.msg_id ?? '').trim() === '' && m.local_id) reuse.set(localKeyOf(m), m.local_id);
  }
  return messages.map((m) => {
    if (String(m.msg_id ?? '').trim() !== '') return m;
    if (m.local_id) return m;
    const key = localKeyOf(m);
    const hit = reuse.get(key);
    if (hit !== undefined) return { ...m, local_id: hit };
    localMsgSeq += 1;
    const fresh = `lm-${localMsgSeq}`;
    reuse.set(key, fresh);
    return { ...m, local_id: fresh };
  });
}

/** 乐观气泡的身份：发送方 + 时刻 + 正文（同内容同时刻的极少见，撞了也只是同一 key）。 */
function localKeyOf(m: ChatMessage): string {
  return `${m.sender}\u0000${m.time}\u0000${m.content}`;
}

/** 列表 key：远程消息用 msg_id，乐观气泡用 local_id（都没有时退回下标，仅兜底）。 */
export function msgKey(msg: ChatMessage, index: number): string {
  return msgTestId(msg) ?? (msg.local_id ? `local-${msg.local_id}` : `local-${index}`);
}

/** 气泡 testid：只有后端给了 msg_id 才有锚点（本地乐观气泡没有）。 */
export function msgTestId(msg: ChatMessage): string | null {
  const id = String(msg.msg_id ?? '').trim();
  return id === '' ? null : `chat-msg-${id}`;
}

/** 气泡 testid 的兜底：远程消息走 `msgTestId`，乐观气泡按 local_id 派生（无 id 时退回下标）。 */
export function msgTestIdOrLocal(msg: ChatMessage, index: number): string {
  return msgTestId(msg) ?? (msg.local_id ? `chat-msg-local-${msg.local_id}` : `chat-msg-local-${index}`);
}

/** 撤回样式覆盖正文（`.bubble-recall`）。 */
export const RECALL_TEXT = '该消息已撤回';
export const TYPING_TEXT = '对方正在输入…';
/** 对话头副行：`公司 · 岗位`，都没有时 `—`。 */
export function headSub(friend: Friend | null): string {
  if (friend === null) return '—';
  const parts = [friend.company, friend.title].map((x) => String(x ?? '').trim()).filter((x) => x !== '');
  return parts.length === 0 ? '—' : parts.join(' · ');
}

/** 发送按钮是否可用：草稿非空白且不在发送中。 */
export function canSend(draft: string, sending: boolean): boolean {
  return draft.trim() !== '' && !sending;
}

/** Ctrl/⌘ + Enter 触发发送（输入框的键盘规则）。 */
export function isSendHotkey(ev: { key: string; ctrlKey: boolean; metaKey: boolean }): boolean {
  return ev.key === 'Enter' && (ev.ctrlKey || ev.metaKey);
}

/** 本地时间 `HH:mm`：给乐观气泡用（后端回执的 time 优先）。 */
export function nowText(now: Date = new Date()): string {
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

/* ------------------------------------------------------------------ *
 * 打招呼链路（SPEC §16.5 裁决 T）
 * ------------------------------------------------------------------ */

/** 公司名归一：去空白、去常见后缀噪声，便于「按公司前缀匹配」。 */
export function normCompany(raw: string): string {
  return String(raw ?? '')
    .replace(/\s+/g, '')
    .replace(/[（(].*?[)）]/g, '')
    .trim();
}

/**
 * 给某个岗位找对应的好友：先按公司前缀双向匹配，其次退到公司包含，
 * 都没有就返回 null（调用方据此插占位会话）。
 */
export function matchFriendByCompany(friends: readonly Friend[], company: string): Friend | null {
  const target = normCompany(company);
  if (target === '') return null;
  // 1) 前缀命中：任一方是另一方的前缀（「星瀚科技」↔「星瀚科技有限公司」）。
  for (const f of friends) {
    const c = normCompany(f.company);
    if (c === '') continue;
    if (c.startsWith(target) || target.startsWith(c)) return f;
  }
  // 2) 退一步：包含关系。
  for (const f of friends) {
    const c = normCompany(f.company);
    if (c !== '' && (c.includes(target) || target.includes(c))) return f;
  }
  return null;
}

/**
 * 无匹配时插进列表头部的临时会话。
 *
 * uid 用 `g` + 岗位标识（SPEC §16.5：`uid='g'+job.id`）。这类会话后端不认识，
 * 所以只能走到「生成打招呼 → 发送这句」那条路，不能 chat_send。
 *
 * 名字只用公司名：这一行就是 Boss 好友列表的「公司」那一列，前缀「待打招呼 · 」
 * 曾挤掉公司名本身的可用宽度（且和对话区那排按钮的语义重复），已删除。
 */
export function pendingFriend(uid: string, company: string, title = ''): Friend {
  return {
    uid: `${PENDING_UID_MARK}${uid}`,
    name: String(company ?? '').trim(),
    company,
    title,
    last_msg: '',
    last_time: '',
    unread: 0,
    avatar: '',
  };
}

/**
 * 占位会话判定。
 *
 * 判据是**完整标记** `pending:` 前缀 + 「从没聊过」的空行形态。曾经只判 `uid` 以
 * `'g'` 开头，而 Boss 真实 uid 也常以 `g` 开头，于是真实会话被误判成占位
 * （发不出消息、本地缓存被当草稿读回来）。历史版本落进 localStorage 的旧占位行
 * 仍以 `g` 开头，所以两种形态都认，但都要满足「没有最后一条消息」这个形态判据。
 */
export function isPendingFriend(friend: Friend): boolean {
  if (friend.last_time !== '' || friend.unread !== 0) return false;
  if (friend.uid.startsWith(PENDING_UID_MARK)) return true;
  // 旧形态（`g` + 岗位标识）：光看前缀会把真实会话吃掉，所以补一条形态判据——
  // 占位行的名字就是公司名，而真实会话的名字是 HR 的称呼。
  return friend.uid.startsWith(PENDING_UID_PREFIX) && friend.name === friend.company;
}

/**
 * 从占位会话的 uid 里取出岗位标识（就是 `job_queue.job_id`）。
 *
 * 占位 uid 有两种历史形态：`pending:<jobId>` 与旧的 `g<jobId>`。真实会话的 uid
 * 不是这个形状，所以非占位行一律返回空串——调用方据此判断「能不能去建会话」。
 */
export function pendingJobId(friend: Friend): string {
  if (!isPendingFriend(friend)) return '';
  if (friend.uid.startsWith(PENDING_UID_MARK)) return friend.uid.slice(PENDING_UID_MARK.length);
  if (friend.uid.startsWith(PENDING_UID_PREFIX)) return friend.uid.slice(PENDING_UID_PREFIX.length);
  return '';
}

/** 该会话能不能用 `chat_send` 发消息（占位会话的 uid 以 `g` 开头）。 */
export function isSendableFriend(friend: Friend): boolean {
  return !isPendingFriend(friend);
}

/** 会话列表 + 占位（占位永远排在最前）。 */
export function withPending(head: Friend[], pending: Friend | null): Friend[] {
  return pending === null ? head : [pending, ...head];
}

/**
 * 会话列表排序（用户 m01539「还未打招呼的草稿放在最上面」）。
 *
 * 三段，各段内部再按时间倒序：
 * 1. **草稿会话**（还没打过招呼的占位，`isPendingFriend`）——置顶。它们不属于 Boss
 *    的真实会话列表，混在按时间排的队列里会被刚聊过的 HR 挤到看不见；
 * 2. 有 `last_ts` 的会话：时间大的在前（最近聊天）；时间相同按 uid 稳定收尾；
 * 3. 没有 `last_ts` 的（DOM 兜底路径取不到时间）：排在最后。
 *
 * 后端 `list_chat_friends` 已经按 `last_ts DESC` 排过一遍，这里再排一次是因为
 * 前端列表里还掺着草稿占位与实时远程结果，必须统一口径。
 *
 * 纯函数，返回新数组，不改入参顺序。
 */
export function sortFriends(friends: readonly Friend[]): Friend[] {
  const rank = (f: Friend): number => {
    if (isPendingFriend(f)) return 0;
    return (f.last_ts ?? 0) > 0 ? 1 : 2;
  };
  const ts = (f: Friend): number => (isPendingFriend(f) ? 0 : (f.last_ts ?? 0));
  return [...friends].sort((a, b) => {
    const ra = rank(a);
    const rb = rank(b);
    if (ra !== rb) return ra - rb;
    if (ra === 1) {
      const d = ts(b) - ts(a);
      if (d !== 0) return d;
    }
    return a.uid < b.uid ? -1 : a.uid > b.uid ? 1 : 0;
  });
}

/* ------------------------------------------------------------------ *
 * 占位会话持久化：刷新页面不得丢掉「生成打招呼」刚加入的 HR。
 * 只存占位字段（无密钥、无 cookie），读写失败一律静默降级为空。
 * ------------------------------------------------------------------ */

/**
 * 会话列表合并（刷新不再整体替换）。
 *
 * 顺序口径与 `sortFriends` 一致：占位在最前，其余按 `last_ts` 倒序，同值按 uid 稳定收尾。
 * **未变更的行返回入参里的同一个对象引用**：keyed `v-for` 只重渲染真正变了的行，
 * 整表重建才是「点刷新卡住」的根因。远程已出现的会话不再保留占位副本（同一 uid 只留一条）。
 */
export function mergeFriends(current: readonly Friend[], remote: readonly Friend[]): Friend[] {
  const byUid = new Map<string, Friend>();
  for (const f of current) byUid.set(f.uid, f);
  const remoteUids = new Set(remote.map((f) => f.uid));
  const pending = current.filter((f) => isPendingFriend(f) && !remoteUids.has(f.uid));
  const rows = remote.map((next) => {
    const prev = byUid.get(next.uid);
    if (prev !== undefined && sameFriend(prev, next)) return prev;
    return next;
  });
  return sortFriends([...pending, ...rows]);
}

function sameFriend(a: Friend, b: Friend): boolean {
  return (
    a.uid === b.uid &&
    a.name === b.name &&
    a.company === b.company &&
    a.title === b.title &&
    a.last_msg === b.last_msg &&
    a.last_time === b.last_time &&
    a.unread === b.unread &&
    a.avatar === b.avatar &&
    (a.last_ts ?? 0) === (b.last_ts ?? 0)
  );
}

/**
 * 消息列表合并（远程结果不再整体替换）。
 *
 * - 远程条目按 `msg_id` 与现有行对齐：内容一致就**保留原对象**（气泡不重建、不重播动画）；
 * - 现有列表里、远程结果中没有的行（乐观气泡 / 本地缓存）留在原位，不被抹掉；
 * - 顺序以远程为准（它是权威时间序），多出来的本地行按原相对位置插回。
 *
 * 结果是「远程序列 + 本地增量」，界面不会因为抓取慢而闪一下。
 */
export function mergeMessages(current: readonly ChatMessage[], remote: readonly ChatMessage[]): ChatMessage[] {
  const byId = new Map<string, ChatMessage>();
  for (const m of current) {
    const id = String(m.msg_id ?? '').trim();
    if (id !== '') byId.set(id, m);
  }
  const remoteIds = new Set(remote.map((m) => String(m.msg_id ?? '').trim()).filter((id) => id !== ''));
  const merged = remote.map((next) => {
    const id = String(next.msg_id ?? '').trim();
    const prev = id === '' ? undefined : byId.get(id);
    return prev !== undefined && sameMessage(prev, next) ? prev : next;
  });
  // 本地独有行（乐观气泡 / 缓存里的历史行）：按原顺序补在末尾 —— 它们都是最新的会话尾巴。
  const extra = current.filter((m) => {
    const id = String(m.msg_id ?? '').trim();
    return id === '' || !remoteIds.has(id);
  });
  return [...merged, ...extra];
}

/**
 * 「这条会话该不该跳过远端拉取」的账本。
 *
 * 判据是**等值**，不是时间大小：`Friend.last_ts` 是毫秒时间戳，而消息的 `time`
 * 已经被 `chat.py::fmt_time` 格式化成「今天 14:20」这类展示串，拿它反解比较会丢掉
 * 精度、判定几乎永不成立。所以只记「上次真正拉取成功时那一刻的 `last_ts`」，
 * 点开时值没变就认为服务端没有新消息。
 *
 * **三条铁律**（错了就会漏消息）：
 * 1. `last_ts` 为 `null` / 缺省（DOM 兜底路径取不到可信时间）→ 一律照常拉取；
 * 2. 只有**成功**拉取才记账：报错、超时、空结果都不记，否则失败会被当成「已最新」；
 * 3. 列表刷新带来**更大的** `last_ts` 时账本自动失效（值不等即照常拉取）。
 */
export function makeMessagesMemo(): {
  shouldSkip: (uid: string, lastTs: number | null | undefined) => boolean;
  noteFetched: (uid: string, lastTs: number | null | undefined) => void;
} {
  const seen = new Map<string, number>();
  const valid = (ts: number | null | undefined): ts is number => ts !== null && ts !== undefined && ts > 0;
  return {
    shouldSkip(uid, lastTs) {
      return valid(lastTs) && seen.get(uid) === lastTs;
    },
    noteFetched(uid, lastTs) {
      if (valid(lastTs)) seen.set(uid, lastTs);
    },
  };
}

function sameMessage(a: ChatMessage, b: ChatMessage): boolean {
  return (
    a.sender === b.sender && a.content === b.content && a.time === b.time && a.recalled === b.recalled
  );
}

export const PENDING_STORAGE_KEY = 'chat.pendingFriends';

/** 跨页联动键：当前展开聊天的 Boss 公司名（聊天页写，概览页读）。 */
export const CHAT_ACTIVE_COMPANY_KEY = 'chat.activeCompany';
/** 同页事件名：写入方在本标签页触发，概览页监听它（storage 事件对写入方不触发）。 */
export const CHAT_ACTIVE_COMPANY_EVENT = 'chat-active-company';

export function loadPendingFriends(storage: Storage | null = safeStorage()): Friend[] {
  if (storage === null) return [];
  try {
    const raw = storage.getItem(PENDING_STORAGE_KEY);
    if (raw === null) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (f): f is Friend =>
        typeof f === 'object' && f !== null && typeof (f as Friend).uid === 'string' && isPendingFriend(f as Friend),
    );
  } catch {
    return [];
  }
}

export function savePendingFriends(friends: readonly Friend[], storage: Storage | null = safeStorage()): void {
  if (storage === null) return;
  try {
    const pending = friends.filter(isPendingFriend);
    if (pending.length === 0) storage.removeItem(PENDING_STORAGE_KEY);
    else storage.setItem(PENDING_STORAGE_KEY, JSON.stringify(pending));
  } catch {
    /* 配额满 / 隐私模式：静默降级 */
  }
}

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
}

/* ------------------------------------------------------------------ *
 * 错误态（`load_friends` 的三条分支）
 * ------------------------------------------------------------------ */

export const SESSION_BOOTING_NOTE = '正在启动浏览器…';
export const FRIENDS_EMPTY_NOTE = '会话列表是空的。确认已登录 Boss；未登录可到「概览」页点「扫码登录」。';
export const FRIENDS_FAIL_PREFIX = '读取会话失败：';
export const MESSAGES_FAIL_PREFIX = '读取消息失败：';
/** 跳过远端拉取时的提示：明确告诉用户「没新消息」，而不是静默不变。
 *  用户 m01764：按钮文案就叫「刷新」，提示里的「刷新列表」跟着改成「刷新」。 */
export const MESSAGES_FRESH_NOTE = '已是最新消息，点「刷新」可重新拉取。';
export const SEND_FAIL_PREFIX = '发送失败：';
export const RESUME_SENT_NOTE = '简历已发出';
export const RECALL_DONE_NOTE = '已撤回上一条消息';
export const GREET_SENT_NOTE = '打招呼已发送';
export const AI_DRAFT_FAIL_PREFIX = 'AI 生成失败：';
/** 正在 Boss 上为这个岗位建会话（点了「立即沟通」，要等页面跳转）。 */
export const OPENING_SESSION_NOTE = '正在打开 Boss 会话…';
export const OPEN_SESSION_FAIL_PREFIX = '建立会话失败：';

/**
 * 给大模型看的「Boss 对话上下文」：把聊天记录压成一段纯文本。
 *
 * 为什么要前端拼：聊天页拿不到 JD 也拿不到会话本身，`greet_draft` 只收一段
 * `reply` 文本，后端不做上下文拼装（它连聊天记录都看不到）。这里只做**取最近几条
 * 的原样转述**，不改写、不摘要——改写属于业务规则，归后端提示词。
 *
 * 只取 `take` 条、跳过撤回行：撤回的内容不该被模型当成 HR 说过的话。
 */
export function aiReplyContext(
  messages: readonly ChatMessage[],
  take = 8,
): string {
  const lines: string[] = [];
  for (const m of messages) {
    if (m.recalled) continue;
    const text = String(m.content ?? '').trim();
    if (text === '') continue;
    const who = m.sender === 'me' ? '我' : m.sender === 'hr' ? 'HR' : '系统';
    lines.push(`${who}：${text}`);
  }
  return lines.slice(-take).join('\n');
}

/** 读会话失败时是否值得先拉一次浏览器会话再重试（`needs_session`）。 */
export function shouldRetryWithSession(errText: string): boolean {
  return needsSession(errText);
}
