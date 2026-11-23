import { describe, expect, it } from 'vitest';
import {
  INITIAL_CURSOR,
  advanceCursor,
  mergeRunRows,
  rowsFromReply,
} from '../src/features/logs/model';
import type { LogRow } from '../src/api/types';

function row(seq: number, ts: number, level = 'info', message = 'm'): LogRow {
  return { seq, ts, level, message };
}

describe('rowsFromReply', () => {
  it('归一化级别、保留原始串、key = ts-seq', () => {
    const rows = rowsFromReply([row(1, 1000, 'warning', 'hi'), row(2, 1000, 'ERROR', 'bad')]);
    expect(rows[0].level).toBe('warn');
    expect(rows[0].levelRaw).toBe('warning');
    expect(rows[0].key).toBe('1000-1');
    expect(rows[1].level).toBe('error');
    expect(rows[1].key).toBe('1000-2');
  });
});

describe('mergeRunRows — seq 过滤 + 倒序', () => {
  it('只收 seq > afterSeq 的新行', () => {
    const old = rowsFromReply([row(1, 1000), row(2, 1000)]);
    const incoming = rowsFromReply([row(1, 1000), row(2, 1000), row(3, 1001)]);
    const merged = mergeRunRows(old, incoming, 2);
    expect(merged.map((r) => r.seq)).toEqual([3, 2, 1]);
  });

  it('闭区间重送的边界条目（seq <= afterSeq）被丢弃，不重复', () => {
    const old = rowsFromReply([row(5, 2000)]);
    const incoming = rowsFromReply([row(5, 2000)]);
    const merged = mergeRunRows(old, incoming, 5);
    expect(merged).toHaveLength(1);
    expect(merged[0].seq).toBe(5);
  });

  it('同毫秒多条按 seq 倒序，新行不丢', () => {
    const old = rowsFromReply([row(1, 1000), row(2, 1000)]);
    const incoming = rowsFromReply([row(3, 1000), row(4, 1000)]);
    const merged = mergeRunRows(old, incoming, 2);
    expect(merged.map((r) => r.seq)).toEqual([4, 3, 2, 1]);
  });

  it('无新行时返回旧数组副本', () => {
    const old = rowsFromReply([row(1, 1000)]);
    const merged = mergeRunRows(old, [], 1);
    expect(merged).toEqual(old);
    expect(merged).not.toBe(old);
  });
});

describe('advanceCursor', () => {
  it('afterSeq 取最大 seq，since 取回执 now', () => {
    const next = advanceCursor(INITIAL_CURSOR, rowsFromReply([row(1, 1000), row(3, 1000)]), 1234);
    expect(next.afterSeq).toBe(3);
    expect(next.since).toBe(1234);
  });

  it('空回也推进 since（否则下次又全量拉），afterSeq 不变', () => {
    const prev = { afterSeq: 7, since: 1000 };
    const next = advanceCursor(prev, [], 2000);
    expect(next.afterSeq).toBe(7);
    expect(next.since).toBe(2000);
  });
});

describe('seq 游标 — 恰好一次（同毫秒边界不丢不重）', () => {
  it('三次增量拉取，边界重送被消化，同毫秒新行保留', () => {
    let cursor = { ...INITIAL_CURSOR };
    let rows = rowsFromReply([]);

    // 第一次全量：seq 1-3 都在 ts=1000，now=1000
    const p1 = rowsFromReply([row(1, 1000), row(2, 1000), row(3, 1000)]);
    rows = mergeRunRows(rows, p1, cursor.afterSeq);
    cursor = advanceCursor(cursor, p1, 1000);
    expect(rows.map((r) => r.seq)).toEqual([3, 2, 1]);
    expect(cursor).toEqual({ afterSeq: 3, since: 1000 });

    // 第二次 since=1000（闭区间）：重送 1-3 + 同毫秒新增 seq4
    const p2 = rowsFromReply([row(1, 1000), row(2, 1000), row(3, 1000), row(4, 1000)]);
    rows = mergeRunRows(rows, p2, cursor.afterSeq);
    cursor = advanceCursor(cursor, p2, 1000);
    expect(rows.map((r) => r.seq)).toEqual([4, 3, 2, 1]); // seq4 同毫秒没丢，1-3 没重
    expect(cursor.afterSeq).toBe(4);

    // 第三次：重送 seq4 + 新增 seq5@1001
    const p3 = rowsFromReply([row(4, 1000), row(5, 1001)]);
    rows = mergeRunRows(rows, p3, cursor.afterSeq);
    cursor = advanceCursor(cursor, p3, 1001);
    expect(rows.map((r) => r.seq)).toEqual([5, 4, 3, 2, 1]);
    expect(cursor).toEqual({ afterSeq: 5, since: 1001 });
  });

  it('清空后游标复位（afterSeq=0, since=null）', () => {
    expect(INITIAL_CURSOR).toEqual({ afterSeq: 0, since: null });
  });
});
