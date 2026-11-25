import { describe, expect, it } from 'vitest';

import type { ChatMessage, Friend } from '../src/api/types';
import {
  CHAT_AI_BUSY_LABEL,
  CHAT_AI_LABEL,
  CHAT_COMPOSE_PLACEHOLDER,
  CHAT_DETACH_EMPTY_SUB,
  CHAT_DETACH_EMPTY_TITLE,
  CHAT_RAIL_TITLE,
  CHAT_SEND_LABEL,
  CHAT_SEARCH_PLACEHOLDER,
  DETACH_MAX_VW,
  DETACH_MIN_W,
  aiReplyContext,
  chatAreaWidth,
  closeDetach,
  createChatUi,
  detachWidth,
  filterFriends,
  friendLine,
  friendMsgTone,
  friendName,
  headSub,
  isSendableFriend,
  isSendHotkey,
  isPendingFriend,
  isTypingMessage,
  matchFriend,
  matchFriendByCompany,
  makeMessagesMemo,
  mergeFriends,
  mergeMessages,
  msgKey,
  msgTestId,
  msgTestIdOrLocal,
  normCompany,
  pendingFriend,
  pickFriend,
  railWidth,
  senderSlug,
  seedLocalMsgIds,
  sortFriends,
  splitThread,
  unreadText,
  unreadTotal,
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

  it('右栏宽：≥1360 基准 360，以下 280；≤1920 时 scale=1 故原值不变', () => {
    expect(railWidth(1360)).toBe(360);
    expect(railWidth(1600)).toBe(360);
    expect(railWidth(1359)).toBe(280);
    expect(railWidth(1280)).toBe(280);
    expect(railWidth(1200)).toBe(280);
  });

  it('右栏宽随 --ui-scale 放大（用户裁决 K/v7）：2560 → 360×1.333', () => {
    // 大屏上字号放大到 1.333 倍，右栏与对话框一起放大，否则正文会挤在同一条固定宽度里。
    expect(railWidth(2560)).toBe(480);
    expect(railWidth(3840)).toBe(540); // 360 × 1.5（封顶）
  });

  it('聊天区可用宽 = 对话框宽 − 右栏宽，规格下限 320 在四档内都满足', () => {
    expect(chatAreaWidth(1200)).toBe(320);
    expect(chatAreaWidth(1280)).toBe(360);
    expect(chatAreaWidth(1440)).toBe(360);
    expect(chatAreaWidth(1600)).toBe(440);
    // 分档点上最紧：1360 时对话框 680 − 右栏 360 = 320，恰好触底。
    expect(chatAreaWidth(1360)).toBe(320);
    for (const w of [1200, 1280, 1360, 1440, 1600, 1920]) {
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

describe('列表的未读计数与状态色（用户 m01427）', () => {
  it('标题计数是未读条数之和，不是会话数', () => {
    const list = [
      friend({ uid: 'a', unread: 3 }),
      friend({ uid: 'b', unread: 0 }),
      friend({ uid: 'c', unread: 7 }),
    ];
    expect(unreadTotal(list)).toBe(10);
    // 会话数 3 —— 两者必须分得开，否则又退回「只统计会话数」。
    expect(unreadTotal(list)).not.toBe(list.length);
  });

  it('全无未读时和为 0（调用方据此不渲染徽标）', () => {
    expect(unreadTotal([friend({ unread: 0 }), friend({ uid: 'b', unread: 0 })])).toBe(0);
    expect(unreadTotal([])).toBe(0);
  });

  it('负数不参与求和（脏数据不许把总数拉低）', () => {
    expect(unreadTotal([friend({ unread: -5 }), friend({ uid: 'b', unread: 2 })])).toBe(2);
  });

  it('有未读 ⇒ 未读色，优先于一切送达态', () => {
    const f = friend({ unread: 2, last_from_me: true, last_status: 2 });
    expect(friendMsgTone(f)).toBe('unread');
    // 对方发的但没读，也算未读
    expect(friendMsgTone(friend({ unread: 1, last_from_me: false }))).toBe('unread');
  });

  it('我发的最后一条：status 2 ⇒ 已读、1 ⇒ 已送达（实测映射）', () => {
    expect(friendMsgTone(friend({ unread: 0, last_from_me: true, last_status: 2 }))).toBe('read');
    expect(friendMsgTone(friend({ unread: 0, last_from_me: true, last_status: 1 }))).toBe('sent');
  });

  it('对方发的最后一条不加状态词（Boss 原文也只给我的消息标 [已读]/[送达]）', () => {
    expect(friendMsgTone(friend({ unread: 0, last_from_me: false, last_status: 2 }))).toBe('plain');
    expect(friendMsgTone(friend({ unread: 0, last_from_me: false, last_status: 0 }))).toBe('plain');
  });

  it('缺字段的老数据（没有 last_from_me/last_status）按对方发的处理', () => {
    expect(friendMsgTone(friend({ unread: 0 }))).toBe('plain');
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

  /* 用户报「点 Boss 看对话消息会卡住」的三条根因（2026-11-25）：
     远程结果整体替换、乐观气泡 key 落下标、入场动画随重建重播。这里锁住前两条的模型层。 */
  it('乐观气泡的 local_id 稳定：同一批数组重新 map 也不换 id', () => {
    const first = seedLocalMsgIds([msg({ msg_id: '' })], []);
    const id = first[0]!.local_id;
    expect(id).toBeTruthy();
    // 模拟「从持久化里重新读一遍」：全新对象、同内容 ⇒ 必须回填同一个 id。
    const again = seedLocalMsgIds([msg({ msg_id: '' })], first);
    expect(again[0]!.local_id).toBe(id);
    // 已经带 id 的条目原样保留引用，不重新分配。
    expect(seedLocalMsgIds(first, [])[0]).toBe(first[0]);
    // 有 msg_id 的消息永远不补 local_id（远程身份就是它的锚点）。
    expect(seedLocalMsgIds([msg({ msg_id: 'a' })], [])[0]!.local_id).toBeUndefined();
  });

  it('列表 key 与 testid：远程用 msg_id，乐观气泡按 local_id，不再按下标', () => {
    const remote = msg({ msg_id: 'abc' });
    expect(msgKey(remote, 3)).toBe('chat-msg-abc');
    expect(msgTestIdOrLocal(remote, 3)).toBe('chat-msg-abc');
    const local = { ...msg({ msg_id: '' }), local_id: 'lm-7' };
    expect(msgKey(local, 3)).toBe('local-lm-7');
    expect(msgTestIdOrLocal(local, 3)).toBe('chat-msg-local-lm-7');
    // 两个下标不同但同一 local_id 的条目必须同 key（下标变了也不会让节点搬家重建）。
    expect(msgKey(local, 0)).toBe(msgKey(local, 9));
  });

  it('mergeMessages：远程结果不整体替换，未变的行保留原对象、乐观气泡留在尾部', () => {
    const a = msg({ msg_id: 'a', content: '一' });
    const b = msg({ msg_id: 'b', content: '二' });
    const local = seedLocalMsgIds([{ sender: 'me', content: '刚发的', time: '10:02', msg_id: '', recalled: false }], [])[0]!;
    const current = [a, b, local];
    // 远程把 a 原样带回来 + 新增 c：a/b/local 引用都不该变。
    const out = mergeMessages(current, [
      msg({ msg_id: 'a', content: '一' }),
      msg({ msg_id: 'b', content: '二' }),
      msg({ msg_id: 'c', content: '三' }),
    ]);
    expect(out.map((m) => m.msg_id)).toEqual(['a', 'b', 'c', '']);
    expect(out[0]).toBe(a);
    expect(out[1]).toBe(b);
    expect(out[3]).toBe(local);
    // 内容变了的行换成远程的新对象（气泡要重渲染，但不重播整体动画）。
    const changed = mergeMessages([a], [msg({ msg_id: 'a', content: '一（改）' })]);
    expect(changed[0]!.content).toBe('一（改）');
    expect(changed[0]).not.toBe(a);
  });

  it('makeMessagesMemo：只有 last_ts 相等才跳过，且失败/缺时间绝不记账', () => {
    const memo = makeMessagesMemo();
    // 从没拉过：不跳过。
    expect(memo.shouldSkip('u1', 1000)).toBe(false);
    memo.noteFetched('u1', 1000);
    // 拉过之后同样的 last_ts：跳过（服务端说没有新消息）。
    expect(memo.shouldSkip('u1', 1000)).toBe(true);
    // 列表刷新带来更大的 last_ts：账本自动失效，必须照常拉取。
    expect(memo.shouldSkip('u1', 2000)).toBe(false);
    memo.noteFetched('u1', 2000);
    expect(memo.shouldSkip('u1', 2000)).toBe(true);
    // DOM 兜底路径 last_ts 是 null：无论拉过什么，一律照常拉取。
    memo.noteFetched('u2', null);
    expect(memo.shouldSkip('u2', null)).toBe(false);
    expect(memo.shouldSkip('u2', 0)).toBe(false);
    // 0 / 负数不是合法时间戳，记账也不该生效。
    memo.noteFetched('u3', 0);
    expect(memo.shouldSkip('u3', 0)).toBe(false);
    // uid 之间互不串账。
    expect(memo.shouldSkip('u9', 2000)).toBe(false);
  });

  it('mergeFriends：刷新只改真正变化的行，占位置顶且远程已有同 uid 时不重复', () => {
    const p = pendingFriend('job-1', '星瀚科技');
    const u1 = friend({ uid: 'u1', last_ts: 100 });
    const u2 = friend({ uid: 'u2', last_ts: 200 });
    const current = [p, u2, u1];

    // 远程原样返回：三行引用全部不变（keyed v-for 不重建任何一行）。
    const same = mergeFriends(current, [friend({ uid: 'u1', last_ts: 100 }), friend({ uid: 'u2', last_ts: 200 })]);
    expect(same).toHaveLength(3);
    expect(same.find((f) => f.uid === 'u1')).toBe(u1);
    expect(same.find((f) => f.uid === 'u2')).toBe(u2);
    expect(same[0]).toBe(p);

    // u1 有新消息 ⇒ 只有它换新对象，并且按 last_ts 排到 u2 前面。
    const next = mergeFriends(current, [friend({ uid: 'u1', last_ts: 300 }), friend({ uid: 'u2', last_ts: 200 })]);
    expect(next.map((f) => f.uid)).toEqual([p.uid, 'u1', 'u2']);
    expect(next[1]).not.toBe(u1);
    expect(next[2]).toBe(u2);

    // 后端已有该会话：占位副本必须消失，只留一条真实行。
    const replaced = mergeFriends(current, [friend({ uid: p.uid, last_ts: 400 })]);
    expect(replaced.map((f) => f.uid)).toEqual([p.uid]);
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

  it('占位会话：uid 带完整标记、名字就是公司名', () => {
    const p = pendingFriend('job-7', '星瀚科技', '后端工程师');
    expect(p.uid).toBe('pending:job-7');
    expect(p.name).toBe('星瀚科技');
    expect(p.company).toBe('星瀚科技');
    expect(p.title).toBe('后端工程师');
    expect(p.unread).toBe(0);
    expect(isPendingFriend(p)).toBe(true);
  });

  /**
   * Boss 真实会话的 uid 常常以 `g` 开头（加密 uid）。曾经只判前缀 `'g'`，
   * 于是库里的真实会话整批被误判成占位：发不出消息，缓存里的行还被当草稿读回来
   * （用户 2026-11-25「没有读本地数据」）。
   */
  it('以 g 开头的真实会话不算占位（uid 前缀不唯一）', () => {
    const real = friend({ uid: 'g4f2a9c', name: '李女士', company: '星瀚科技', last_time: '' });
    expect(isPendingFriend(real)).toBe(false);
    expect(isSendableFriend(real)).toBe(true);
  });

  it('历史版本落盘的旧占位行仍认得出来', () => {
    const legacy = friend({ uid: 'gjob-7', name: '星瀚科技', company: '星瀚科技', last_time: '' });
    expect(isPendingFriend(legacy)).toBe(true);
  });

  it('占位会话不能用 chat_send（后端不认识这个 uid）', () => {
    expect(isSendableFriend(pendingFriend('x', '星瀚科技'))).toBe(false);
    expect(isSendableFriend(friend({ uid: 'u1', name: '张三' }))).toBe(true);
    // 名字里带「待打招呼」但形态是真实会话（uid 无标记）→ 照常可发。
    expect(isSendableFriend(friend({ uid: 'u2', name: '客户待打招呼' }))).toBe(true);
  });

  it('占位永远插在列表最前', () => {
    const head = [friend({ uid: 'a' }), friend({ uid: 'b' })];
    const out = withPending(head, pendingFriend('job-7', '星瀚科技'));
    expect(out.map((f) => f.uid)).toEqual(['pending:job-7', 'a', 'b']);
    expect(withPending(head, null)).toBe(head);
  });

  it('排序：草稿置顶 → 最近聊天在前 → 无时间戳的垫底（用户 m01539）', () => {
    const list = [
      friend({ uid: 'old', last_ts: 1_000 }),
      friend({ uid: 'none', last_ts: null }),
      pendingFriend('job-7', '星瀚科技'),
      friend({ uid: 'new', last_ts: 9_000 }),
      friend({ uid: 'mid', last_ts: 5_000 }),
    ];
    const out = sortFriends(list);
    expect(out.map((f) => f.uid)).toEqual(['pending:job-7', 'new', 'mid', 'old', 'none']);
    // 纯函数：不改入参顺序
    expect(list.map((f) => f.uid)).toEqual(['old', 'none', 'pending:job-7', 'new', 'mid']);
  });

  it('排序：缺 last_ts 字段（老数据）同样垫底，同段时间倒序后按 uid 稳定', () => {
    const out = sortFriends([
      friend({ uid: 'z-noTs' }),
      friend({ uid: 'b-same', last_ts: 2_000 }),
      friend({ uid: 'a-same', last_ts: 2_000 }),
    ]);
    expect(out.map((f) => f.uid)).toEqual(['a-same', 'b-same', 'z-noTs']);
  });
});

describe('逐字文案（SPEC §5，改名即契约破坏）', () => {
  it('搜索占位 / 标题 / 发送 / 输入框占位', () => {
    expect(CHAT_SEARCH_PLACEHOLDER).toBe('搜索 HR / 公司 / 消息');
    expect(CHAT_RAIL_TITLE).toBe('BOSS 消息');
    expect(CHAT_SEND_LABEL).toBe('发送');
    expect(CHAT_COMPOSE_PLACEHOLDER).toBe('输入消息…');
  });

  it('发送按钮上方的「AI 生成」两态文案（用户 m01148）', () => {
    expect(CHAT_AI_LABEL).toBe('AI 生成');
    expect(CHAT_AI_BUSY_LABEL).toBe('生成中…');
  });

  it('对话框空态两句', () => {
    expect(CHAT_DETACH_EMPTY_TITLE).toBe('在左侧 Boss 列表点一位，开始聊天');
    expect(CHAT_DETACH_EMPTY_SUB).toBe('点左侧 Boss 列表里的任意一位即可查看聊天');
  });
});

describe('AI 生成的对话上下文（用户 m01148）', () => {
  function msg(sender: string, content: string, patch: Partial<ChatMessage> = {}): ChatMessage {
    return { sender, content, time: '10:00', msg_id: '', recalled: false, ...patch };
  }

  it('把最近几条压成「谁：说了什么」，我自己标成「我」', () => {
    const got = aiReplyContext([
      msg('me', '您好，我是做后端的'),
      msg('hr', '方便发一份简历吗'),
      msg('me', '好的，稍等'),
    ]);
    expect(got).toBe('我：您好，我是做后端的\nHR：方便发一份简历吗\n我：好的，稍等');
  });

  it('撤回行与空正文不进上下文（撤回的话不能让模型接）', () => {
    const got = aiReplyContext([
      msg('hr', '这条撤回了', { recalled: true }),
      msg('hr', '   '),
      msg('hr', '还在吗'),
    ]);
    expect(got).toBe('HR：还在吗');
  });

  it('只取最后 take 条：上下文不是全量聊天记录', () => {
    const many = Array.from({ length: 20 }, (_, i) => msg('hr', `第${i}句`));
    const got = aiReplyContext(many, 3).split('\n');
    expect(got).toEqual(['HR：第17句', 'HR：第18句', 'HR：第19句']);
  });

  it('没有消息时给空串（后端据此走「首次打招呼」提示词）', () => {
    expect(aiReplyContext([])).toBe('');
  });
});
