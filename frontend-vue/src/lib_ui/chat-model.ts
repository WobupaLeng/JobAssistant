/**
 * Boss 消息页的纯逻辑层（无 IO、无 DOM）。
 *
 * 全部规则是从 Leptos 版 `features/chat/{state,list,thread}.rs` 直译过来的，
 * 差异只有一处且必须记住：**页面 DOM 口径按冻结原型 v3 走**
 * （`prototypes/ui-v1` + `docs/ui/UI-SPEC-chat-detach-v3.md`），
 * 不是 Leptos 那套整页 `.chat-grid`。两边的行为规则一致，结构不同：
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
/** 未读数封顶显示。 */
export const UNREAD_CAP = 99;
/**
 * 自动投递轮询间隔：**已废弃**，恒为 undefined。
 *
 * 保留这个导出只是为了让契约测试能断言「前端不再有轮询间隔」；状态请求现在
 * 全由 60~90 秒随机心跳与用户操作驱动（见 `auto-heartbeat.ts`）。
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

/** Esc 分层：上层还有可关对象时由上层消费，这一层不动（A10）。 */
export function escClosesDetach(hasUpperLayer: boolean): boolean {
  return !hasUpperLayer;
}

/** 刷新会话列表不得改变展开态与选中（SPEC §3「刷新会话列表」一行）。 */
export function stateSurvivesRefresh(state: ChatUiState): ChatUiState {
  return state;
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

/** 列表名字兜底：没名字就显示 uid（Leptos `name or uid`）。 */
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
/** 收起按钮逐字（§5）。 */
export const CHAT_CLOSE_LABEL = '收起对话框';
/** 输入框占位与发送按钮逐字（§5）。 */
export const CHAT_COMPOSE_PLACEHOLDER = '输入消息…';
export const CHAT_SEND_LABEL = '发送';
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

/** Leptos `sender_slug`：me/self→me，system/typing→system，空→hr，其余原样。 */
export function senderSlug(raw: string): SenderSlug | string {
  const s = String(raw ?? '').trim().toLowerCase();
  if (s === 'me' || s === 'self') return 'me';
  if (s === 'system' || s === 'typing') return 'system';
  if (s === '') return 'hr';
  return s;
}

/** 「对方正在输入」不是气泡，渲染成 typing 行（Leptos `load_messages` 的拆分规则）。 */
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

/** 气泡 testid：只有后端给了 msg_id 才有锚点（本地乐观气泡没有）。 */
export function msgTestId(msg: ChatMessage): string | null {
  const id = String(msg.msg_id ?? '').trim();
  return id === '' ? null : `chat-msg-${id}`;
}

/** 撤回样式覆盖正文（Leptos `.bubble-recall`）。 */
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

/** Ctrl/⌘ + Enter 触发发送（Leptos composer 的键盘规则）。 */
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
    uid: `${PENDING_UID_PREFIX}${uid}`,
    name: String(company ?? '').trim(),
    company,
    title,
    last_msg: '',
    last_time: '',
    unread: 0,
    avatar: '',
  };
}

/** 占位会话判定：uid 以 `g` 开头且不在后端列表里。 */
export function isPendingFriend(friend: Friend): boolean {
  return friend.uid.startsWith(PENDING_UID_PREFIX) && friend.last_time === '' && friend.unread === 0;
}

/** 该会话能不能用 `chat_send` 发消息（占位会话的 uid 以 `g` 开头）。 */
export function isSendableFriend(friend: Friend): boolean {
  return !isPendingFriend(friend);
}

/** 会话列表 + 占位（占位永远排在最前）。 */
export function withPending(head: Friend[], pending: Friend | null): Friend[] {
  return pending === null ? head : [pending, ...head];
}

/* ------------------------------------------------------------------ *
 * 占位会话持久化：刷新页面不得丢掉「生成打招呼」刚加入的 HR。
 * 只存占位字段（无密钥、无 cookie），读写失败一律静默降级为空。
 * ------------------------------------------------------------------ */

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
 * 错误态（Leptos `load_friends` 的三条分支）
 * ------------------------------------------------------------------ */

export const SESSION_BOOTING_NOTE = '正在启动浏览器…';
export const FRIENDS_EMPTY_NOTE = '会话列表是空的。确认已登录 Boss；未登录可到「概览」页点「扫码登录」。';
export const FRIENDS_FAIL_PREFIX = '读取会话失败：';
export const MESSAGES_FAIL_PREFIX = '读取消息失败：';
export const SEND_FAIL_PREFIX = '发送失败：';
export const RESUME_SENT_NOTE = '简历已发出';
export const RECALL_DONE_NOTE = '已撤回上一条消息';
export const GREET_SENT_NOTE = '打招呼已发送';

/** 读会话失败时是否值得先拉一次浏览器会话再重试（`needs_session`）。 */
export function shouldRetryWithSession(errText: string): boolean {
  return needsSession(errText);
}
