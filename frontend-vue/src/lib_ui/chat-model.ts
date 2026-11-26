/**
 * 聊天的纯逻辑层（无 IO、无 DOM）：状态机、宽度公式、合并、气泡分型、文案。收起态右栏常驻、
 * 对话框带 `hidden`；展开态右栏让位，对话框内含「左列表段 + 右聊天区」；收起保留
 * `activeFriend`，再点同一位可重开。
 */
import type { Friend, ChatMessage } from '../api/types';
import { computeRailWidth } from './canvas';
import { needsSession } from './session';

/** 对话框宽度下限。 */
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
 * 占位会话 uid 的**完整标记**。Boss 真实会话的 uid 也常以 `g` 开头（加密 uid），
 * 只判前缀会把真实会话整批判成占位：发不出消息，本地列表还会多出来历不明的草稿行。
 */
export const PENDING_UID_MARK = 'pending:';
/** 未读数封顶显示。 */
export const UNREAD_CAP = 99;
/** 自动投递**没有**轮询间隔（恒为 undefined，保留导出供契约测试断言）：状态全由心跳驱动。 */
export const AUTO_TICK_MS = undefined;

/**
 * 对话框宽度：`round(min(max(innerWidth * 0.5, 600), innerWidth * 0.8))`。
 * 四档验收：1200→600 / 1280→640 / 1440→720 / 1600→800。
 */
export function detachWidth(innerWidth: number): number {
  if (!Number.isFinite(innerWidth) || innerWidth <= 0) return DETACH_MIN_W;
  const half = innerWidth * DETACH_VW;
  const capped = Math.min(Math.max(half, DETACH_MIN_W), innerWidth * DETACH_MAX_VW);
  return Math.round(capped);
}

/**
 * 右栏（= 对话框内左段 Boss 列表）宽度，**已乘 `--ui-scale`**：大屏上右栏要和字号
 * 一起变宽。公式只有一份在 `canvas.ts:computeRailWidth`，这里保留旧名字供既有单测使用。
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
 * 副行该用哪种颜色（已送达 / 已读 / 未读）：
 * `unread`（派生未读，判据见 `chat-read.ts`）或 `unread > 0` 为未读；否则在
 * `last_from_me` 为真时看 `last_status`（`2` 已读、`1` 已送达）。对方发的最后一条
 * 只有「未读」一种状态词——已读 / 送达说的都是我发的那条。
 */
export function friendMsgTone(friend: Friend, unread = false): FriendMsgTone {
  if (unread || (friend.unread ?? 0) > 0) return 'unread';
  if (friend.last_from_me !== true) return 'plain';
  const status = friend.last_status ?? 0;
  if (status === 2) return 'read';
  if (status === 1) return 'sent';
  return 'plain';
}

/** 标题旁的计数：**未读消息条数**之和，不是会话数；全为 0 时由调用方决定不渲染。 */
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
/** 空态标题「没有会话」（不是「还没有会话」）。FriendList 必须消费这个常量，不在组件里
 * 另写一份字面量。 */
export const CHAT_EMPTY_TITLE = '没有会话';
export const CHAT_DETACH_EMPTY_TITLE = '在左侧 Boss 列表点一位，开始聊天';
export const CHAT_DETACH_EMPTY_SUB = '点左侧 Boss 列表里的任意一位即可查看聊天';
/** 收起按钮逐字，只用两字：四字在竖排胶囊里溢出。模板里拆成两段竖排。 */
export const CHAT_CLOSE_LABEL = '收起';
/** 输入框占位与发送按钮逐字（§5）。 */
export const CHAT_COMPOSE_PLACEHOLDER = '输入消息…';
export const CHAT_SEND_LABEL = '发送';
/** 输入框上方「AI 生成」按钮逐字。 */
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
 * 给**没有 `msg_id` 的乐观气泡**补一个会话内稳定的 `local_id`：用下标当 key 时，
 * 远程结果或前面插入一条都会让整表重 diff。身份取「发送方 + 时刻 + 正文」，
 * 在上一次的同列表中回填旧 id。
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

/* ------------------------- 打招呼链路 ------------------------- */

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
 * 无匹配时插进列表头部的临时会话，uid 形如 `pending:<job.id>`。这类会话后端不认识，
 * 只能走「生成打招呼 → 发送这句」，不能 chat_send。名字只用公司名，不加前缀。
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
 * 占位会话判定：`pending:` 完整标记（或历史遗留的 `g` 前缀）**且**「从没聊过」的空行形态。
 * 少了形态判据会把 Boss 真实会话误判成占位。
 */
export function isPendingFriend(friend: Friend): boolean {
  // 本机新建的真实会话（`local_only`）：uid 是 Boss 聊天页的加密 id，形态与占位行像，
  // 但它已经能发消息、能拉历史——一旦误判成占位，输入框会消失、「移除」按钮会把它删掉。
  if (friend.local_only === true) return false;
  if (friend.last_time !== '' || friend.unread !== 0) return false;
  if (friend.uid.startsWith(PENDING_UID_MARK)) return true;
  // 旧形态（`g` + 岗位标识）：光看前缀会把真实会话吃掉，所以补一条形态判据——
  // 占位行的名字就是公司名，而真实会话的名字是 HR 的称呼。
  return friend.uid.startsWith(PENDING_UID_PREFIX) && friend.name === friend.company;
}

/** 从占位 uid（`pending:<jobId>` 或旧的 `g<jobId>`）里取出岗位标识；非占位行返回空串。 */
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
 * 会话列表排序，三段：草稿占位置顶 → 有 `last_ts` 的按时间倒序 → 无时间的最后。
 * 后端已按 `last_ts DESC` 排过，但前端列表还掺着草稿与实时远程结果，需统一口径。
 * 纯函数，不改入参顺序。
 */
export function sortFriends(friends: readonly Friend[]): Friend[] {
  const rank = (f: Friend): number => {
    if (isPendingFriend(f) || f.local_only === true) return 0;
    return (f.last_ts ?? 0) > 0 ? 1 : 2;
  };
  const ts = (f: Friend): number => (isPendingFriend(f) || f.local_only === true ? 0 : (f.last_ts ?? 0));
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
 * 我发出一条消息：就地更新那一行（副行文本 + [送达]），不重拉整张列表。
 * ------------------------------------------------------------------ */

/**
 * 刚给某位 Boss 发出一条消息：把会话列表里那一行改成「我刚发了这句」。不重拉 `chat_friends`：
 * 那条命令要 reload 整个聊天页（0.7s 起步），为一行的状态刷整张表不值当。`last_status = 1` 是
 * 「已送达」，**不许伪造 2（已读）**；`unread` 必须清零，否则「未读」会一直盖住「送达」。
 */
export function markFriendSent(
  friends: readonly Friend[],
  uid: string,
  text: string,
  now: Date = new Date(),
): Friend[] {
  return sortFriends(
    friends.map((f) =>
      f.uid === uid
        ? {
            ...f,
            last_msg: text,
            last_time: nowText(now),
            last_ts: now.getTime(),
            last_from_me: true,
            last_status: 1,
            unread: 0,
          }
        : f,
    ),
  );
}

/* ------------------------------------------------------------------ *
 * 占位会话持久化：刷新页面不得丢掉「生成打招呼」刚加入的 HR。
 * 只存占位字段（无密钥、无 cookie），读写失败一律静默降级为空。
 * ------------------------------------------------------------------ */

/**
 * 会话列表合并：顺序同 `sortFriends`；**未变更的行返回同一个对象引用**，否则整表重建
 * 会让刷新卡住。远程已出现的会话不再保留占位副本。
 */
export function mergeFriends(current: readonly Friend[], remote: readonly Friend[]): Friend[] {
  const byUid = new Map<string, Friend>();
  for (const f of current) byUid.set(f.uid, f);
  const remoteUids = new Set(remote.map((f) => f.uid));
  const pending = current.filter((f) => isPendingFriend(f) && !remoteUids.has(f.uid));
  // 本机新建的真实会话：uid 与好友列表的 id 空间不同（加密 uid vs 数字 uid），
  // 按 uid 一定匹配不上，只能靠「岗位 id 相同」或「公司名相同」判远程是否已收录。
  const localOnly = current.filter(
    (f) => f.local_only === true && !remoteUids.has(f.uid) && !remote.some((r) => confirmedBy(f, r)),
  );
  const rows = remote.map((next) => {
    const prev = byUid.get(next.uid);
    if (prev !== undefined && sameFriend(prev, next)) return prev;
    return next;
  });
  return sortFriends([...pending, ...localOnly, ...rows]);
}

/** 远程的这条是否就是本地新建的那条（岗位 id 优先，退一步按公司名）。 */
function confirmedBy(local: Friend, remote: Friend): boolean {
  const jobId = local.job_id ?? '';
  if (jobId !== '' && (remote.job_id ?? '') === jobId) return true;
  const a = normCompany(local.company);
  const b = normCompany(remote.company);
  return a !== '' && b !== '' && (a === b || a.startsWith(b) || b.startsWith(a));
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
    (a.local_only ?? false) === (b.local_only ?? false) &&
    (a.last_ts ?? 0) === (b.last_ts ?? 0)
  );
}

/**
 * 消息列表合并：远程条目按 `msg_id` 对齐，内容一致就**保留原对象**（不重建、不重播动画）；
 * 本地独有的行（乐观气泡 / 本地缓存）留在原位；顺序以远程为准。
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
 * 「这条会话该不该跳过远端拉取」的账本：只记上次**成功**拉取时的 `last_ts`，值没变就
 * 认为没有新消息（消息的 `time` 是展示串，拿它比较会丢精度）。三条铁律：`last_ts` 缺失
 * 一律照常拉取；只有成功才记账；值不等即照常拉取。
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

/** 需要落 localStorage 的「本机独有行」：打招呼占位 + 本机新建、远程还没收录的真实会话。 */
function isLocalRow(friend: Friend): boolean {
  return isPendingFriend(friend) || friend.local_only === true;
}

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
        typeof f === 'object' && f !== null && typeof (f as Friend).uid === 'string' && isLocalRow(f as Friend),
    );
  } catch {
    return [];
  }
}

export function savePendingFriends(friends: readonly Friend[], storage: Storage | null = safeStorage()): void {
  if (storage === null) return;
  try {
    const pending = friends.filter(isLocalRow);
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
/** 跳过远端拉取时的提示：明确告诉用户「没新消息」，文案跟按钮统一叫「刷新」。 */
export const MESSAGES_FRESH_NOTE = '已是最新消息，点「刷新」可重新拉取。';
/**
 * 消息流末尾只留前半句：刷新按钮就在列表头顶，气泡区里再教一遍只会把
 * 一句灰字拉成两行。
 */
export const MESSAGES_FRESH_SHORT = '已是最新消息';
export const SEND_FAIL_PREFIX = '发送失败：';
export const RESUME_SENT_NOTE = '简历已发出';
export const RECALL_DONE_NOTE = '已撤回上一条消息';
export const GREET_SENT_NOTE = '打招呼已发送';
export const AI_DRAFT_FAIL_PREFIX = 'AI 生成失败：';
/** 正在 Boss 上为这个岗位建会话（点了「立即沟通」，要等页面跳转）。 */
export const OPENING_SESSION_NOTE = '正在打开 Boss 会话…';
export const OPEN_SESSION_FAIL_PREFIX = '建立会话失败：';

/**
 * 给大模型看的对话上下文：`greet_draft` 只收一段文本、看不到聊天记录，所以在这里
 * 取最近 `take` 条**原样转述**（不改写、不摘要，改写归后端提示词），并跳过撤回行。
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
