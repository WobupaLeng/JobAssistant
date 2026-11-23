import { describe, expect, it } from 'vitest';
import {
  DECISION_FILTERS,
  LOG_VIEWS,
  decisionCountLabel,
  decisionFilterAccepts,
  filterRuns,
  rowsFromReply,
  runCountLabel,
  toggleLevel,
} from '../src/features/logs/model';
import type { LogRow } from '../src/api/types';

function row(seq: number, ts: number, level: string, message: string): LogRow {
  return { seq, ts, level, message };
}

describe('toggleLevel', () => {
  it('未选 → 选中', () => {
    expect(toggleLevel([], 'warn')).toEqual(['warn']);
  });
  it('已选 → 取消', () => {
    expect(toggleLevel(['warn', 'error'], 'warn')).toEqual(['error']);
  });
  it('选满三档 → 回到全部（空数组）', () => {
    expect(toggleLevel(['error', 'warn'], 'info')).toEqual([]);
  });
});

describe('filterRuns', () => {
  const rows = rowsFromReply([
    row(1, 1000, 'info', '抓取开始'),
    row(2, 1001, 'warn', '薪资面议'),
    row(3, 1002, 'error', '登录失败'),
  ]);

  it('无筛选 → 全量', () => {
    expect(filterRuns(rows, [], '')).toHaveLength(3);
  });
  it('按级别筛选', () => {
    expect(filterRuns(rows, ['error'], '').map((r) => r.seq)).toEqual([3]);
  });
  it('按关键字筛选（命中消息）', () => {
    expect(filterRuns(rows, [], '薪资').map((r) => r.seq)).toEqual([2]);
  });
  it('级别 + 关键字联合', () => {
    expect(filterRuns(rows, ['warn', 'error'], '登录').map((r) => r.seq)).toEqual([3]);
  });
});

describe('计数文案', () => {
  it('runCountLabel：筛选时 shown / total，否则 total', () => {
    expect(runCountLabel(2, 5, true)).toBe('2 / 5 条');
    expect(runCountLabel(5, 5, false)).toBe('5 条');
  });
  it('decisionCountLabel 同口径', () => {
    expect(decisionCountLabel(1, 3, true)).toBe('1 / 3 条');
    expect(decisionCountLabel(3, 3, false)).toBe('3 条');
  });
});

describe('元数据常量', () => {
  it('LOG_VIEWS 两个视图 slug 不重名', () => {
    const slugs = LOG_VIEWS.map((v) => v.slug);
    expect(slugs).toEqual(['decision', 'runtime']);
    expect(new Set(slugs).size).toBe(2);
  });
  it('DECISION_FILTERS 五档', () => {
    expect(DECISION_FILTERS.map((f) => f.slug)).toEqual([
      'all',
      'rescore',
      'reject',
      'accept',
      'other',
    ]);
  });
});

describe('decisionFilterAccepts — 按 tone 判定', () => {
  it('all 收一切', () => {
    expect(decisionFilterAccepts('all', 'default')).toBe(true);
  });
  it('rescore↔accent / reject↔bad / accept↔primary / other↔default', () => {
    expect(decisionFilterAccepts('rescore', 'accent')).toBe(true);
    expect(decisionFilterAccepts('reject', 'bad')).toBe(true);
    expect(decisionFilterAccepts('accept', 'primary')).toBe(true);
    expect(decisionFilterAccepts('other', 'default')).toBe(true);
    expect(decisionFilterAccepts('accept', 'bad')).toBe(false);
    expect(decisionFilterAccepts('other', 'primary')).toBe(false);
  });
});
