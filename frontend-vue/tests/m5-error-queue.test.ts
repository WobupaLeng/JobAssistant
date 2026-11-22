import { describe, expect, it } from 'vitest';

import {
  EMPTY_QUEUE,
  ERROR_QUEUE_MAX,
  announceText,
  clearErrors,
  dismissError,
  firstError,
  hasError,
  pendingCount,
  pendingText,
  pushError,
  seedErrors,
} from '../src/lib_ui/error-queue';

describe('error-queue / 入队', () => {
  it('空队列没有错误', () => {
    expect(hasError(EMPTY_QUEUE)).toBe(false);
    expect(firstError(EMPTY_QUEUE)).toBeNull();
    expect(pendingCount(EMPTY_QUEUE)).toBe(0);
    expect(pendingText(EMPTY_QUEUE)).toBe('');
    expect(announceText(EMPTY_QUEUE)).toBe('');
  });

  it('第一条成为 head，id 递增', () => {
    const q1 = pushError(EMPTY_QUEUE, '列表加载失败', 'list_jobs', 1000);
    const q2 = pushError(q1, '配置读取失败', 'get_app_config', 2000);
    expect(q2.items.map((i) => i.message)).toEqual(['列表加载失败', '配置读取失败']);
    expect(q2.items[0].id).not.toBe(q2.items[1].id);
    expect(firstError(q2)?.code).toBe('list_jobs');
    expect(q2.items[0].atMs).toBe(1000);
  });

  it('同 (message, code) 不重复堆叠，只刷新时间', () => {
    const q1 = pushError(EMPTY_QUEUE, '列表加载失败', 'list_jobs', 1000);
    const q2 = pushError(q1, '列表加载失败', 'list_jobs', 5000);
    expect(q2.items).toHaveLength(1);
    expect(q2.items[0].atMs).toBe(5000);
    expect(q2.dropped).toBe(0);
  });

  it('同 message 不同 code 视为两条', () => {
    const q1 = pushError(EMPTY_QUEUE, '失败', 'list_jobs', 1);
    const q2 = pushError(q1, '失败', 'job_tab_counts', 2);
    expect(q2.items).toHaveLength(2);
  });

  it('超过上限丢最旧的，并计入 dropped（不静默截断）', () => {
    let q = EMPTY_QUEUE;
    for (let i = 0; i < ERROR_QUEUE_MAX + 2; i += 1) {
      q = pushError(q, `错误 ${i}`, `code_${i}`, i);
    }
    expect(q.items).toHaveLength(ERROR_QUEUE_MAX);
    expect(q.dropped).toBe(2);
    expect(q.items[0].message).toBe('错误 2');
    expect(pendingCount(q)).toBe(ERROR_QUEUE_MAX + 2 - 1);
    expect(pendingText(q)).toBe(`另有 ${ERROR_QUEUE_MAX + 1} 条更早的错误`);
  });

  it('retryLabel 会保存下来', () => {
    const q = pushError(EMPTY_QUEUE, 'x', 'list_jobs', 1, '重试');
    expect(q.items[0].retryLabel).toBe('重试');
    const q2 = pushError(EMPTY_QUEUE, 'y', 'list_jobs', 1);
    expect(q2.items[0].retryLabel).toBeNull();
  });
});

describe('error-queue / 出队', () => {
  it('逐条关闭，后一条顶上', () => {
    const q = pushError(
      pushError(EMPTY_QUEUE, '第一条', 'a', 1),
      '第二条',
      'b',
      2,
    );
    const head = firstError(q)!;
    const after = dismissError(q, head.id);
    expect(after.items).toHaveLength(1);
    expect(firstError(after)?.message).toBe('第二条');
    expect(after.dropped).toBe(0);
  });

  it('关不存在的 id 不改变任何东西', () => {
    const q = pushError(EMPTY_QUEUE, '第一条', 'a', 1);
    expect(dismissError(q, -999)).toEqual(q);
  });

  it('清空后 dropped 归零', () => {
    let q = EMPTY_QUEUE;
    for (let i = 0; i < ERROR_QUEUE_MAX + 1; i += 1) {
      q = pushError(q, `e${i}`, `c${i}`, i);
    }
    expect(q.dropped).toBe(1);
    const cleared = clearErrors(q);
    expect(cleared.items).toHaveLength(0);
    expect(cleared.dropped).toBe(0);
    expect(hasError(cleared)).toBe(false);
  });

  it('全部关闭 = 完全空队列', () => {
    let q = pushError(pushError(EMPTY_QUEUE, 'a1', 'a', 1), 'b1', 'b', 2);
    while (q.items.length > 0) {
      q = dismissError(q, q.items[0].id);
    }
    expect(pendingCount(q)).toBe(0);
    expect(pendingText(q)).toBe('');
  });
});

describe('error-queue / 播报', () => {
  it('单条时只播报内容', () => {
    const q = pushError(EMPTY_QUEUE, '列表加载失败', 'list_jobs', 1);
    expect(announceText(q)).toBe('错误：列表加载失败');
  });

  it('多条时带上更早条数', () => {
    const q = pushError(pushError(EMPTY_QUEUE, '甲', 'a', 1), '乙', 'b', 2);
    expect(announceText(q)).toBe('错误：甲。另有 1 条更早的错误');
  });

  it('seedErrors 按顺序建队列（只接消息数组，同一 code）', () => {
    const q = seedErrors(['甲', '乙'], 'seed', 7);
    expect(q.items.map((i) => i.message)).toEqual(['甲', '乙']);
    expect(q.items.every((i) => i.code === 'seed')).toBe(true);
    expect(q.items.every((i) => i.atMs === 7)).toBe(true);
  });

  it('纯函数：不修改入参队列', () => {
    const q = pushError(EMPTY_QUEUE, '甲', 'a', 1);
    const snapshot = JSON.parse(JSON.stringify(q));
    pushError(q, '乙', 'b', 2);
    dismissError(q, q.items[0].id);
    clearErrors(q);
    expect(q).toEqual(snapshot);
  });
});
