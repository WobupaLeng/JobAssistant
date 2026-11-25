/**
 * 概览页日志卡「一条流」（用户 m00182 / m00374）：运行日志与岗位事件流（✔ / 跳过）合并成
 * 一条按时间正序的流，再用单选页签「全部 / 错误 / 警告 / 信息 / 事件」筛。
 * 决策记录不进这条线（m00404），仍走「决策记录」覆盖页。
 *
 * 这里只测纯函数：同毫秒的先后、截断、页签筛选与空态文案、key 唯一。
 */
import { describe, expect, it } from 'vitest';
import { mergeMix, mixEmptyText, mixTabs, mixVisible, pickMix, type MixLog } from '../src/lib_ui/log-mix';
import type { FeedLine } from '../src/lib_ui/scrape-panel-model';

function log(over: Partial<MixLog> = {}): MixLog {
  return { ts: 1_700_000_000_000, seq: 1, level: 'info', message: '一行日志', ...over };
}

function feed(over: Partial<FeedLine> = {}): FeedLine {
  return { ts: 1_700_000_000_000, n: 1, tag: '跳过', msg: '外包公司', tone: 'skip', ...over };
}

describe('mergeMix — 两类行混成一条时间线', () => {
  it('按时间正序，事件行补的接收时刻与日志同一刻度', () => {
    const rows = mergeMix(
      [log({ ts: 1_700_000_000_000, seq: 1, message: '早' }), log({ ts: 1_700_000_020_000, seq: 2, message: '晚' })],
      [feed({ ts: 1_700_000_010_000, n: 7 })],
    );
    expect(rows.map((r) => (r.kind === 'log' ? r.message : r.tag))).toEqual(['早', '跳过', '晚']);
  });

  it('同一毫秒：运行日志在前、事件在后（事件是「这个岗位怎么处理了」的结果）', () => {
    const rows = mergeMix([log({ ts: 1_700_000_000_000, seq: 3 })], [feed({ ts: 1_700_000_000_000, n: 9 })]);
    expect(rows.map((r) => r.kind)).toEqual(['log', 'feed']);
  });

  it('事件的 ts 缺失 / 非法时按 0 处理，不许把排序变成 NaN 比较', () => {
    const rows = mergeMix([log({ ts: 5 })], [feed({ ts: Number.NaN }), feed({ ts: 0, n: 2 })]);
    expect(rows.map((r) => r.kind)).toEqual(['feed', 'feed', 'log']);
  });

  it('超过上限只留最后 N 行（卡里那一屏够用，别攒整环 DOM）', () => {
    const logs = Array.from({ length: 6 }, (_, i) => log({ ts: 1_000 + i * 1000, seq: i + 1, message: `第${i}` }));
    const rows = mergeMix(logs, [feed({ ts: 1_700_000_100_000, n: 1 })], 3);
    expect(rows).toHaveLength(3);
    expect(rows[rows.length - 1]?.kind).toBe('feed');
  });

  it('key 唯一：同毫秒的两条日志靠 seq 区分，事件靠 n', () => {
    const rows = mergeMix(
      [log({ ts: 1, seq: 1 }), log({ ts: 1, seq: 2 })],
      [feed({ ts: 1, n: 5 }), feed({ ts: 1, n: 6 })],
    );
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  });
});

describe('页签筛选', () => {
  const rows = mergeMix(
    [
      log({ ts: 1_700_000_000_000, seq: 1, level: 'info', message: '信息' }),
      log({ ts: 1_700_000_001_000, seq: 2, level: 'warn', message: '警告' }),
      log({ ts: 1_700_000_002_000, seq: 3, level: 'error', message: '错误' }),
    ],
    [feed({ ts: 1_700_000_003_000, n: 4, tag: '✔', msg: '86 分 · 后端开发', tone: 'ok' })],
  );

  it('「全部」= 两类混排（不筛）', () => {
    expect(pickMix(rows, 'all')).toHaveLength(4);
  });

  it('错误 / 警告 / 信息各只看同级别的运行日志', () => {
    expect(pickMix(rows, 'error').map((r) => (r.kind === 'log' ? r.message : r.tag))).toEqual(['错误']);
    expect(pickMix(rows, 'warn').map((r) => (r.kind === 'log' ? r.message : r.tag))).toEqual(['警告']);
    expect(pickMix(rows, 'info').map((r) => (r.kind === 'log' ? r.message : r.tag))).toEqual(['信息']);
  });

  it('「事件」只看 ✔ / 跳过，一条运行日志都不许混进来', () => {
    const picked = pickMix(rows, 'event');
    expect(picked).toHaveLength(1);
    expect(picked[0]?.kind).toBe('feed');
  });

  it('mixVisible 与 pickMix 同口径', () => {
    for (const tab of ['all', 'error', 'warn', 'info', 'event'] as const) {
      expect(rows.filter((r) => mixVisible(r, tab))).toEqual(pickMix(rows, tab));
    }
  });
});

describe('页签顺序与空态文案', () => {
  it('五项：全部在前，事件在最后', () => {
    expect(mixTabs().map((t) => t.label)).toEqual(['全部', '错误', '警告', '信息', '事件']);
  });

  it('每类的空态各说各的，「全部」保留原来那句（别让用户以为筛坏了）', () => {
    expect(mixEmptyText('all')).toBe('尚无事件。点击「开始抓取」后这里会滚动显示进度。');
    expect(mixEmptyText('error')).toBe('还没有错误日志。');
    expect(mixEmptyText('warn')).toBe('还没有警告日志。');
    expect(mixEmptyText('info')).toBe('还没有信息日志。');
    expect(mixEmptyText('event')).toContain('还没有岗位事件');
  });
});