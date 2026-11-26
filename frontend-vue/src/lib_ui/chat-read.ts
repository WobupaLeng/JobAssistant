/**
 * 会话「读到哪一条」的水位。
 *
 * 为什么需要它：Boss 接口里的 `unreadMsgCount` 实测恒为 0（真实账号 100 条会话全 0，
 * Boss 自己的「未读」页签也不报数字），所以「有没有新回复」这件事 Boss 不给，只能自己判。
 * 用户拍板的口径是：**HR 最后发言 = 未读，打开会话后清除**。
 *
 * 「我读到哪一条」是本地事实，不落库：后端 `chat_friends` 没有这一列，
 * 为一次界面判定加迁移不划算。按 uid 记 `last_ts` 毫秒水位，存 localStorage。
 */
import type { Friend } from '../api/types';
import { isPendingFriend } from './chat-model';

/** uid → 已读水位（我最后一次看到对方消息的 `last_ts`，毫秒）。 */
export type ReadMarks = Record<string, number>;

const READ_MARKS_KEY = 'aja.chat_read_marks';

/** 读水位表。坏数据 / 隐私模式一律当空表，不抛出。 */
export function loadReadMarks(): ReadMarks {
  try {
    const raw = localStorage.getItem(READ_MARKS_KEY);
    if (raw === null) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return {};
    const marks: ReadMarks = {};
    for (const [uid, ts] of Object.entries(parsed as Record<string, unknown>)) {
      const n = Number(ts);
      if (uid !== '' && Number.isFinite(n) && n > 0) marks[uid] = n;
    }
    return marks;
  } catch {
    return {};
  }
}

function writeReadMarks(marks: ReadMarks): void {
  try {
    localStorage.setItem(READ_MARKS_KEY, JSON.stringify(marks));
  } catch {
    // 存不进只影响「下次还记不记得」，不影响当前这次渲染，静默即可。
  }
}

/**
 * 记下「这个会话我看到 `ts` 了」。返回**新对象**：调用方要把它放回 ref 才会重渲染。
 * 水位只升不降，重复打开同一会话不会产生新写盘。
 */
export function saveReadMark(marks: ReadMarks, uid: string, ts: number): ReadMarks {
  if (uid === '' || !Number.isFinite(ts) || ts <= 0) return marks;
  if (ts <= (marks[uid] ?? 0)) return marks;
  const next: ReadMarks = { ...marks, [uid]: ts };
  writeReadMarks(next);
  return next;
}

/**
 * 这一行算不算未读：
 * 1. Boss 报的条数 > 0 —— 有就照它（将来 Boss 真报数时不用改）；
 * 2. 否则「HR 最后发言 + 晚于我的已读水位」。
 * 占位会话（还没在 Boss 上存在）与没有时刻的行不算，否则新加的行会常亮红点。
 */
export function isUnreadRow(friend: Friend, marks: ReadMarks): boolean {
  if ((friend.unread ?? 0) > 0) return true;
  if (friend.last_from_me === true) return false;
  if (isPendingFriend(friend)) return false;
  const ts = friend.last_ts ?? 0;
  if (ts <= 0) return false;
  return ts > (marks[friend.uid] ?? 0);
}

/** 未读**会话数**（不是条数）。Boss 的条数拿不到时，标题旁只能报会话数。 */
export function unreadSessions(friends: readonly Friend[], marks: ReadMarks): number {
  let n = 0;
  for (const f of friends) if (isUnreadRow(f, marks)) n += 1;
  return n;
}