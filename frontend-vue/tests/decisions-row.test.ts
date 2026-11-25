import { describe, expect, it } from 'vitest';
import {
  decisionRowClass,
  decisionRowFromRecord,
  filterDecisions,
  scoreTitle,
} from '../src/features/logs/model';
import type { DecisionRecord } from '../src/api/types';

function rec(over: Partial<DecisionRecord> = {}): DecisionRecord {
  return {
    id: 1,
    platform: 'boss',
    company_job: '光蕴科技/java开发工程师',
    decision: 'accept',
    reason: '达标',
    created_at: 1_700_000_000,
    old_score: 62,
    new_score: 78,
    ...over,
  };
}

describe('decisionRowFromRecord — 四列视图模型', () => {
  it('入库行：公司/岗位按最后斜杠切，分数变化取两列，绿行', () => {
    const v = decisionRowFromRecord(rec());
    expect(v.company).toBe('光蕴科技');
    expect(v.title).toBe('java开发工程师');
    expect(v.kind).toBe('入库');
    expect(v.tone).toBe('primary');
    expect(v.verdict).toBe('62 → 78');
    expect(v.scoreDir).toBe('up');
    expect(v.rowClass).toBe('dl-row is-in');
    expect(v.reason).toBe('光蕴科技 · java开发工程师 · 达标');
    expect(v.reasonEmpty).toBe(false);
    // 列表列到秒，tooltip 带年份（created_at 是 Unix 秒 → 1_700_000_000 = 2023-11-15 06:13:20 @ UTC+8）
    expect(v.time).toBe('11-15 06:13:20');
    expect(v.timeTitle).toBe('2023-11-15 06:13:20');
    // 平台以前只存不显：现在并进「环节」列
    expect(v.platform).toBe('Boss直聘');
    // why 只放原因（不含公司/岗位），行内两层展示用
    expect(v.why).toBe('达标');
  });

  it('platform 空串 → 「—」，未知 id 原样回显', () => {
    expect(decisionRowFromRecord(rec({ platform: '' })).platform).toBe('—');
    expect(decisionRowFromRecord(rec({ platform: 'lagou' })).platform).toBe('lagou');
  });

  it('排除行：两列皆 null 且 reason 无分数 → 「—」，绝不 0 → 0，红行', () => {
    const v = decisionRowFromRecord(
      rec({ decision: 'reject', old_score: null, new_score: null, reason: '' }),
    );
    expect(v.verdict).toBe('—');
    expect(v.scoreDir).toBe('none');
    expect(v.kind).toBe('排除');
    expect(v.rowClass).toBe('dl-row is-out');
    expect(v.reasonEmpty).toBe(true);
    expect(v.reason).toContain('（未记录原因）');
  });

  it('company_job 为空 → 公司/岗位占位文案', () => {
    const v = decisionRowFromRecord(rec({ company_job: '' }));
    expect(v.company).toBe('（未知公司）');
    expect(v.title).toBe('（未记录岗位）');
  });

  it('无斜杠 → 公司空、岗位整串', () => {
    const v = decisionRowFromRecord(rec({ company_job: '高级后端' }));
    expect(v.company).toBe('（未知公司）');
    expect(v.title).toBe('高级后端');
  });

  it('重算行：两列 null 时回落 reason 解析分数，升 → 绿行', () => {
    const v = decisionRowFromRecord(
      rec({ decision: 'rescore', old_score: null, new_score: null, reason: '50 → 80' }),
    );
    expect(v.verdict).toBe('50 → 80');
    expect(v.scoreDir).toBe('up');
    expect(v.tone).toBe('accent');
    expect(v.rowClass).toBe('dl-row is-in');
  });

  it('重算降分 → 红行', () => {
    const v = decisionRowFromRecord(rec({ decision: 'rescore', old_score: 80, new_score: 50 }));
    expect(v.scoreDir).toBe('down');
    expect(v.rowClass).toBe('dl-row is-out');
  });

  it('只有新分数（旧为 null）→ 「→ 70」，dir none', () => {
    const v = decisionRowFromRecord(rec({ old_score: null, new_score: 70 }));
    expect(v.verdict).toBe('→ 70');
    expect(v.scoreDir).toBe('none');
  });

  it('未知 decision 原样显示，tone default，不着色', () => {
    const v = decisionRowFromRecord(rec({ decision: 'prescan' }));
    expect(v.kind).toBe('prescan');
    expect(v.tone).toBe('default');
    expect(v.rowClass).toBe('dl-row');
  });
});

describe('scoreTitle', () => {
  it('四档文案', () => {
    expect(scoreTitle('up')).toBe('分数上调');
    expect(scoreTitle('down')).toBe('分数下调');
    expect(scoreTitle('same')).toBe('分数未变');
    expect(scoreTitle('none')).toBe('没有分数记录');
  });
});

describe('decisionRowClass', () => {
  it('入库绿 / 排除红', () => {
    expect(decisionRowClass('primary', 'none')).toBe('dl-row is-in');
    expect(decisionRowClass('bad', 'none')).toBe('dl-row is-out');
  });
  it('重算按升降，同分/无记录不着色', () => {
    expect(decisionRowClass('accent', 'up')).toBe('dl-row is-in');
    expect(decisionRowClass('accent', 'down')).toBe('dl-row is-out');
    expect(decisionRowClass('accent', 'same')).toBe('dl-row');
    expect(decisionRowClass('accent', 'none')).toBe('dl-row');
  });
  it('其它不着色', () => {
    expect(decisionRowClass('default', 'up')).toBe('dl-row');
  });
});

describe('filterDecisions', () => {
  const rows = [
    decisionRowFromRecord(rec({ id: 1, decision: 'accept', company_job: '甲公司/后端' })),
    decisionRowFromRecord(rec({ id: 2, decision: 'reject', company_job: '乙公司/前端' })),
    decisionRowFromRecord(rec({ id: 3, decision: 'rescore', company_job: '丙公司/测试' })),
  ];

  it('all 收全部', () => {
    expect(filterDecisions(rows, 'all', '')).toHaveLength(3);
  });
  it('按行为筛选', () => {
    expect(filterDecisions(rows, 'reject', '').map((r) => r.id)).toEqual([2]);
    expect(filterDecisions(rows, 'accept', '').map((r) => r.id)).toEqual([1]);
  });
  it('关键字命中公司/岗位/原因', () => {
    expect(filterDecisions(rows, 'all', '乙公司').map((r) => r.id)).toEqual([2]);
    expect(filterDecisions(rows, 'all', '测试').map((r) => r.id)).toEqual([3]);
  });
  it('行为 + 关键字联合', () => {
    expect(filterDecisions(rows, 'accept', '后端').map((r) => r.id)).toEqual([1]);
    expect(filterDecisions(rows, 'accept', '前端')).toHaveLength(0);
  });
});
