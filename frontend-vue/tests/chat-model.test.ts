import { describe, expect, it } from 'vitest';

import type { ChatMessage, Friend } from '../src/api/types';
import {
  CHAT_COMPOSE_PLACEHOLDER,
  CHAT_DETACH_EMPTY_SUB,
  CHAT_DETACH_EMPTY_TITLE,
  CHAT_RAIL_TITLE,
  CHAT_SEND_LABEL,
  CHAT_SEARCH_PLACEHOLDER,
  DETACH_MAX_VW,
  DETACH_MIN_W,
  chatAreaWidth,
  closeDetach,
  createChatUi,
  detachWidth,
  filterFriends,
  friendLine,
  friendName,
  headSub,
  isSendableFriend,
  isSendHotkey,
  isPendingFriend,
  isTypingMessage,
  matchFriend,
  matchFriendByCompany,
  msgTestId,
  normCompany,
  pendingFriend,
  pickFriend,
  railWidth,
  senderSlug,
  splitThread,
  unreadText,
  withPending,
} from '../src/lib_ui/chat-model';

function friend(patch: Partial<Friend> = {}): Friend {
  return {
    uid: 'u1',
    name: '张三',
    company: '星瀚科技',
    title: '后端工程师',
    last_msg: '你好',
    last_time: '10:00',
    unread: 0,
    avatar: '',
    ...patch,
  };
}

function msg(patch: Partial<ChatMessage> = {}): ChatMessage {
  return { sender: 'hr', content: '在吗', time: '10:01', msg_id: 'm1', recalled: false, ...patch };
}

describe('对话框宽度公式（SPEC §2.2 / §16.6，A49）', () => {
  // 四档验收值直接抄自规格，任何一档变了都是契约破坏。
  const CASES: Array<{ w: number; want: number }> = [
    { w: 1200, want: 600 },
    { w: 1280, want: 640 },
    { w: 1440, want: 720 },
    { w: 1600, want: 800 },
    { w: 1920, want: 960 },
    { w: 2560, want: 1280 },
  ];

  for (const c of CASES) {
    it(`innerWidth ${c.w} → ${c.want}`, () => {
      expect(detachWidth(c.w)).toBe(c.want);
    });
  }

  it('下限 600：窗口再窄也不会缩到 600 以下', () => {
    expect(detachWidth(1000)).toBe(DETACH_MIN_W);
    expect(detachWidth(800)).toBe(DETACH_MIN_W);
  });

  it('50vw 与 80vw 都不越界：clamp 的两端在宽屏上等于 50vw', () => {
    // 关键性质是 width === round(min(max(w/2, 600), w*0.8))，两端都要守住。
    for (const w of [1200, 1280, 1440, 1600, 1920, 2560, 3000, 3840]) {
      const got = detachWidth(w);
      expect(got).toBe(Math.round(Math.min(Math.max(w * 0.5, DETACH_MIN_W), w * DETACH_MAX_VW)));
      expect(got).toBeLessThanOrEqual(Math.round(w * DETACH_MAX_VW));
      expect(got).toBeGreaterThanOrEqual(DETACH_MIN_W);
    }
  });

  it('非法输入回落下限（首帧 innerWidth 可能还是 0）', () => {
    expect(detachWidth(0)).toBe(DETACH_MIN_W);
    expect(detachWidth(-1)).toBe(DETACH_MIN_W);
    expect(detachWidth(Number.NaN)).toBe(DETACH_MIN_W);
    expect(detachWidth(Number.POSITIVE_INFINITY)).toBe(DETACH_MIN_W);
  });

  it('右栏宽：≥1280 基准 300，以下 260；≤1920 时 scale=1 故原值不变', () => {
    expect(railWidth(1280)).toBe(300);
    expect(railWidth(1600)).toBe(300);
    expect(railWidth(1279)).toBe(260);
    expect(railWidth(1200)).toBe(260);
  });

  it('右栏宽随 --ui-scale 放大（用户裁决 K/v7）：2560 → 300×1.333', () => {
    // 大屏上字号放大到 1.333 倍，右栏与对话框一起放大，否则正文会挤在 300px 栏里。
    expect(railWidth(2560)).toBe(400);
    expect(railWidth(3840)).toBe(450); // 450 × 1.5（封顶）
  });

  it('聊天区可用宽 = 对话框宽 − 右栏宽，规格下限 320 在四档内都满足', () => {
    expect(chatAreaWidth(1200)).toBe(340);
    expect(chatAreaWidth(1280)).toBe(340);
    expect(chatAreaWidth(1440)).toBe(420);
    expect(chatAreaWidth(1600)).toBe(500);
    for (const w of [1200, 1280, 1440, 1600, 1920]) {
      expect(chatAreaWidth(w)).toBeGreaterThanOrEqual(320);
    }
  });
});

describe('展开/收起状态机（SPEC §3）', () => {
  it('初始：收起、无选中', () => {
    expect(createChatUi()).toEqual({ chatOpen: false, activeFriendsUid: null });
  });

  it('点某人 ⇒ 选中并展开', () => {
    const s = pickFriend(createChatUi(), 'u1');
    expect(s).toEqual({ chatOpen: true, activeFriendsUid: 'u1' });
  });

  it('展开态切人只换 uid，仍保持展开', () => {
    const s = pickFriend(pickFriend(createChatUi(), 'u1'), 'u2');
    expect(s).toEqual({ chatOpen: true, activeFriendsUid: 'u2' });
  });

  it('A7 收起保留选中', () => {
    const s = closeDetach(pickFriend(createChatUi(), 'u1'));
    expect(s).toEqual({ chatOpen: false, activeFriendsUid: 'u1' });
  });

  it('A8 收起后再点同一位能重新展开', () => {
    const s = pickFriend(closeDetach(pickFriend(createChatUi(), 'u1')), 'u1');
    expect(s).toEqual({ chatOpen: true, activeFriendsUid: 'u1' });
  });

  it('收起是幂等的（连按两次 Esc 不会变出别的状态）', () => {
    const closed = closeDetach(pickFriend(createChatUi(), 'u1'));
    expect(closeDetach(closed)).toEqual(closed);
  });

  it('状态机是纯函数：不改入参', () => {
    const before = createChatUi();
    const snapshot = { ...before };
    pickFriend(before, 'u9');
    closeDetach(before);
    expect(before).toEqual(snapshot);
  });
});

describe('会话列表', () => {
  it('搜索命中 name / company / title / last_msg 任一字段', () => {
    const f = friend();
    expect(matchFriend(f, '')).toBe(true);
    expect(matchFriend(f, '张')).toBe(true);
    expect(matchFriend(f, '星瀚')).toBe(true);
    expect(matchFriend(f, '后端')).toBe(true);
    expect(matchFriend(f, '你好')).toBe(true);
    expect(matchFriend(f, '不存在的词')).toBe(false);
  });

  it('搜索大小写无关，且前后空白被忽略', () => {
    const f = friend({ company: 'ByteDance' });
    expect(matchFriend(f, 'bytedance')).toBe(true);
    expect(matchFriend(f, '  BYTE  ')).toBe(true);
  });

  it('filterFriends 保持原顺序', () => {
    const list = [friend({ uid: 'a', name: '甲' }), friend({ uid: 'b', name: '乙' })];
    expect(filterFriends(list, '乙').map((x) => x.uid)).toEqual(['b']);
    expect(filterFriends(list, '').map((x) => x.uid)).toEqual(['a', 'b']);
  });

  it('副行回退链：最近消息 → 岗位 →（暂无消息）', () => {
    expect(friendLine(friend({ last_msg: '在吗' }))).toBe('在吗');
    expect(friendLine(friend({ last_msg: '', title: '后端工程师' }))).toBe('后端工程师');
    expect(friendLine(friend({ last_msg: '  ', title: ' ' }))).toBe('（暂无消息）');
  });

  it('名字兜底到 uid', () => {
    expect(friendName(friend({ name: '' }))).toBe('u1');
    expect(friendName(friend({ name: ' 李四 ' }))).toBe('李四');
  });

  it('未读角标：>99 显示 99+，其余原样', () => {
    expect(unreadText(1)).toBe('1');
    expect(unreadText(99)).toBe('99');
    expect(unreadText(100)).toBe('99+');
    expect(unreadText(1234)).toBe('99+');
  });
});

describe('线程渲染规则', () => {
  it('sender_slug 映射（me/self、system/typing、空）', () => {
    expect(senderSlug('me')).toBe('me');
    expect(senderSlug('SELF')).toBe('me');
    expect(senderSlug('system')).toBe('system');
    expect(senderSlug('typing')).toBe('system');
    expect(senderSlug('')).toBe('hr');
    expect(senderSlug('hr')).toBe('hr');
  });

  it('「正在输入」不是气泡，拆成 typing 标记', () => {
    const m = msg({ sender: 'system', content: '对方正在输入…' });
    expect(isTypingMessage(m)).toBe(true);
    expect(isTypingMessage(msg({ sender: 'system', content: '系统提示' }))).toBe(false);
    // 非 system 发送者即使文案含「正在输入」也照常当气泡。
    expect(isTypingMessage(msg({ sender: 'hr', content: '正在输入' }))).toBe(false);
  });

  it('splitThread 把 typing 抽出来且保序', () => {
    const list = [
      msg({ msg_id: 'a', content: '一' }),
      msg({ msg_id: 'b', sender: 'system', content: '对方正在输入…' }),
      msg({ msg_id: 'c', content: '三' }),
    ];
    const out = splitThread(list);
    expect(out.typing).toBe(true);
    expect(out.bubbles.map((m) => m.msg_id)).toEqual(['a', 'c']);
  });

  it('没有 typing 帧时 typing=false', () => {
    expect(splitThread([msg()]).typing).toBe(false);
    expect(splitThread([]).bubbles).toEqual([]);
  });

  it('气泡 testid：只有后端 msg_id 才给锚点（本地乐观气泡没有）', () => {
    expect(msgTestId(msg({ msg_id: 'abc' }))).toBe('chat-msg-abc');
    expect(msgTestId(msg({ msg_id: '' }))).toBeNull();
    expect(msgTestId(msg({ msg_id: '  ' }))).toBeNull();
  });

  it('对话头副行：公司 · 岗位，都空则 —', () => {
    expect(headSub(friend())).toBe('星瀚科技 · 后端工程师');
    expect(headSub(friend({ title: '' }))).toBe('星瀚科技');
    expect(headSub(friend({ company: '', title: '' }))).toBe('—');
    expect(headSub(null)).toBe('—');
  });

  it('Ctrl/⌘ + Enter 才是发送键；单独 Enter 是换行', () => {
    expect(isSendHotkey({ key: 'Enter', ctrlKey: true, metaKey: false })).toBe(true);
    expect(isSendHotkey({ key: 'Enter', ctrlKey: false, metaKey: true })).toBe(true);
    expect(isSendHotkey({ key: 'Enter', ctrlKey: false, metaKey: false })).toBe(false);
    expect(isSendHotkey({ key: 'a', ctrlKey: true, metaKey: false })).toBe(false);
  });
});

describe('打招呼链路（SPEC §16.5 裁决 T）', () => {
  it('公司名归一：去空白、去括号后缀', () => {
    expect(normCompany(' 星瀚 科技 ')).toBe('星瀚科技');
    expect(normCompany('星瀚科技（杭州）有限公司')).toBe('星瀚科技有限公司');
    expect(normCompany('星瀚科技(北京)')).toBe('星瀚科技');
  });

  it('前缀双向命中：公司名互为前缀时找到同一位', () => {
    const list = [friend({ uid: 'a', company: '星瀚科技有限公司' })];
    expect(matchFriendByCompany(list, '星瀚科技')?.uid).toBe('a');
    expect(matchFriendByCompany(list, '星瀚科技有限公司杭州分公司')?.uid).toBe('a');
  });

  it('前缀不中时退到包含匹配', () => {
    const list = [friend({ uid: 'b', company: '杭州星瀚科技' })];
    expect(matchFriendByCompany(list, '星瀚科技')?.uid).toBe('b');
  });

  it('完全匹配不上返回 null（调用方据此插占位会话）', () => {
    const list = [friend({ uid: 'c', company: '别的公司' })];
    expect(matchFriendByCompany(list, '星瀚科技')).toBeNull();
    expect(matchFriendByCompany([], '星瀚科技')).toBeNull();
    expect(matchFriendByCompany(list, '   ')).toBeNull();
  });

  it('占位会话：uid 带 g 前缀、名字是「待打招呼 · 公司」', () => {
    const p = pendingFriend('job-7', '星瀚科技', '后端工程师');
    expect(p.uid).toBe('gjob-7');
    expect(p.name).toBe('待打招呼 · 星瀚科技');
    expect(p.company).toBe('星瀚科技');
    expect(p.title).toBe('后端工程师');
    expect(p.unread).toBe(0);
    expect(isPendingFriend(p)).toBe(true);
  });

  it('占位会话不能用 chat_send（后端不认识这个 uid）', () => {
    expect(isSendableFriend(pendingFriend('x', '星瀚科技'))).toBe(false);
    expect(isSendableFriend(friend({ uid: 'u1', name: '张三' }))).toBe(true);
    // 名字里只是含「待打招呼」但不在开头，仍可发送。
    expect(isSendableFriend(friend({ name: '客户待打招呼' }))).toBe(true);
  });

  it('占位永远插在列表最前', () => {
    const head = [friend({ uid: 'a' }), friend({ uid: 'b' })];
    const out = withPending(head, pendingFriend('job-7', '星瀚科技'));
    expect(out.map((f) => f.uid)).toEqual(['gjob-7', 'a', 'b']);
    expect(withPending(head, null)).toBe(head);
  });
});

describe('逐字文案（SPEC §5，改名即契约破坏）', () => {
  it('搜索占位 / 标题 / 发送 / 输入框占位', () => {
    expect(CHAT_SEARCH_PLACEHOLDER).toBe('搜索 HR / 公司 / 消息');
    expect(CHAT_RAIL_TITLE).toBe('BOSS 消息');
    expect(CHAT_SEND_LABEL).toBe('发送');
    expect(CHAT_COMPOSE_PLACEHOLDER).toBe('输入消息…');
  });

  it('对话框空态两句', () => {
    expect(CHAT_DETACH_EMPTY_TITLE).toBe('在左侧 Boss 列表点一位，开始聊天');
    expect(CHAT_DETACH_EMPTY_SUB).toBe('点左侧 Boss 列表里的任意一位即可查看聊天');
  });
});
