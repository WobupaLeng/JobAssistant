import { describe, expect, it } from 'vitest';

import type { Friend } from '../src/api/types';
import { friendMsgTone } from '../src/lib_ui/chat-model';
import { isUnreadRow, saveReadMark, unreadSessions, type ReadMarks } from '../src/lib_ui/chat-read';

function friend(uid: string, patch: Partial<Friend> = {}): Friend {
  return {
    uid,
    name: uid,
    company: '',
    title: '',
    last_msg: '',
    last_time: '',
    unread: 0,
    avatar: '',
    ...patch,
  };
}

describe('会话未读派生（Boss 的 unreadMsgCount 恒为 0）', () => {
  /** 真实账号实测：HR 最后发言的会话 unreadMsgCount 也是 0、last_status 也是 0。 */
  const bossLast = friend('u1', { last_from_me: false, last_status: 0, last_ts: 1_700_000_000_000 });

  it('HR 最后发言且没读过 → 未读', () => {
    expect(isUnreadRow(bossLast, {})).toBe(true);
    expect(friendMsgTone(bossLast, isUnreadRow(bossLast, {}))).toBe('unread');
  });

  it('读过（水位追平）→ 不算未读，副行回到无状态词', () => {
    const marks: ReadMarks = saveReadMark({}, 'u1', bossLast.last_ts ?? 0);
    expect(isUnreadRow(bossLast, marks)).toBe(false);
    expect(friendMsgTone(bossLast, isUnreadRow(bossLast, marks))).toBe('plain');
  });

  it('我最后发言 → 不算未读（水位与 last_ts 无关）', () => {
    const mine = friend('u2', { last_from_me: true, last_status: 1, last_ts: 1_700_000_000_000 });
    expect(isUnreadRow(mine, {})).toBe(false);
  });

  it('Boss 真报了条数时以条数为准', () => {
    const counted = friend('u3', { unread: 3, last_from_me: true, last_ts: 1 });
    expect(isUnreadRow(counted, {})).toBe(true);
  });

  it('占位会话 / 没有时刻的行不点红点', () => {
    expect(isUnreadRow(friend('pending:job-1', { name: '星瀚科技', company: '星瀚科技' }), {})).toBe(false);
    expect(isUnreadRow(friend('u4', { last_from_me: false, last_ts: null }), {})).toBe(false);
  });

  it('水位只升不降，旧时刻不覆盖新时刻', () => {
    const marks = saveReadMark({}, 'u1', 2_000);
    expect(saveReadMark(marks, 'u1', 1_000)).toBe(marks);
    expect(saveReadMark(marks, 'u1', 3_000).u1).toBe(3_000);
  });

  it('标题计数按会话数，不按条数', () => {
    const list = [
      bossLast,
      friend('u5', { last_from_me: false, last_ts: 1_700_000_000_001 }),
      friend('u6', { last_from_me: true, last_ts: 1_700_000_000_002 }),
    ];
    expect(unreadSessions(list, {})).toBe(2);
  });
});