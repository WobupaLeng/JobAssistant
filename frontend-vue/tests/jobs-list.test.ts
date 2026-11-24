/**
 * 概览页「列表逻辑」纯函数：页签 / 排序 / 可见性 / 批量 / 卡片视图 / 保活锚点。
 *
 * 语义逐条对抄 Leptos 版 `features/jobs/state.rs` 与 `features/jobs/cards.rs`，
 * 只断言数据与字符串，不挂 DOM。
 */
import { describe, expect, it } from 'vitest';
import type { Job, JobTabCounts } from '../src/api/types';
import {
  LIST_TABS,
  LIST_TAB_DEFAULT,
  SORT_DEFAULT,
  SORT_FIELDS,
  addrFirstLine,
  allVisibleChecked,
  batchLabel,
  batchOps,
  cardView,
  emptyTabCounts,
  headCount,
  keepaliveRows,
  metaItems,
  defaultDescending,
  nextSort,
  scoreTier,
  shouldOpenCard,
  showBatchActions,
  sortArrow,
  sortOrder,
  statusChip,
  tabQuery,
  toggleChecked,
  visibleJobs,
} from '../src/lib_ui/job-list-model';
import { fmtTime } from '../src/lib_ui/time';

function job(part: Partial<Job>): Job {
  return {
    job_id: '1',
    platform: 'boss',
    title: '后端工程师',
    company: '星瀚科技',
    salary: '20-30K',
    status: 'pending',
    created_at: 1_700_000_000_000,
    job_url: '',
    jd_text: '',
    location: '深圳',
    experience: '3-5年',
    degree: '本科',
    hr_name: '',
    hr_position: '',
    hr_active_status: '',
    company_industry: '',
    company_scale: '500-999人',
    company_stage: '',
    company_address: '',
    company_intro: '',
    hiring_count: 0,
    founded_years: 0,
    match_score: 88,
    rejected: 0,
    score_source: 'rule',
    score_reason: '',
    updated_at: 0,
    score_scale: 100,
    ...part,
  };
}

describe('页签', () => {
  it('三个页签的 slug 与文案逐字不变（E2E 依赖）', () => {
    expect(LIST_TABS.map((t) => t.slug)).toEqual(['pending', 'delivered', 'rejected']);
    expect(LIST_TABS.map((t) => t.label)).toEqual(['未投递', '已投递', '不投递']);
    expect(LIST_TAB_DEFAULT).toBe('pending');
  });

  it('只有「不投递」页签向后端要 rejected', () => {
    expect(tabQuery('rejected', false)).toBe('rejected');
    expect(tabQuery('pending', false)).toBe('pending');
    expect(tabQuery('delivered', false)).toBe('delivered');
    expect(tabQuery('pending', true)).toBeUndefined();
  });
});

describe('排序', () => {
  it('加入时间的 testid slug 是 time（不是 created）', () => {
    expect(SORT_FIELDS.map((s) => s.slug)).toEqual(['time', 'salary', 'score']);
    expect(SORT_FIELDS.map((s) => s.label)).toEqual(['加入时间', '薪资', '分值']);
    expect(SORT_DEFAULT).toBe('created');
  });

  it('点当前字段翻转方向，点别的字段回到默认降序', () => {
    // nextSort(field, descending) 只翻转方向；换字段的「回到默认降序」由组件里的
    // defaultDescending(新字段) 负责（见 list-model.ts:95-98）。
    expect(nextSort('created', false)).toBe(true);
    expect(nextSort('created', true)).toBe(false);
    expect(defaultDescending('score')).toBe(true);
    expect(defaultDescending('created')).toBe(true);
    expect(sortOrder(true)).toBe('desc');
    expect(sortOrder(false)).toBe('asc');
    expect(sortArrow(true)).toBe('↓');
    expect(sortArrow(false)).toBe('↑');
  });
});

describe('可见性（后端不认 tab，前端自己筛）', () => {
  const list = [
    job({ job_id: 'a' }),
    job({ job_id: 'b', status: 'delivered' }),
    job({ job_id: 'c', rejected: 1 }),
    job({ job_id: 'd', status: 'delivered', rejected: 1 }),
  ];

  it('未投递 = 未被排除且未投递', () => {
    expect(visibleJobs(list, 'pending', false).map((j) => j.job_id)).toEqual(['a']);
  });

  it('已投递 = status 为 delivered 且未被排除', () => {
    expect(visibleJobs(list, 'delivered', false).map((j) => j.job_id)).toEqual(['b']);
  });

  it('不投递 = rejected 非 0（含已投递后被排除的）', () => {
    expect(visibleJobs(list, 'rejected', false).map((j) => j.job_id)).toEqual(['c', 'd']);
  });

  it('「全部」页签下按时间倒序，排除项与已投递都在', () => {
    expect(visibleJobs(list, 'pending', true).map((j) => j.job_id)).toEqual([
      'a',
      'b',
      'c',
      'd',
    ]);
  });
});

describe('计数', () => {
  const counts: JobTabCounts = { pending: 3, delivered: 5, rejected: 2 };

  it('单页签取后端下发的 TabCounts，不取 db_overview.total', () => {
    expect(headCount(counts, 'pending', false)).toBe(3);
    expect(headCount(counts, 'delivered', false)).toBe(5);
  });

  it('「全部」是三段求和', () => {
    expect(headCount(counts, 'pending', true)).toBe(10);
  });

  it('空计数全 0', () => {
    expect(emptyTabCounts()).toEqual({ pending: 0, delivered: 0, rejected: 0 });
  });
});

describe('批量操作', () => {
  it('勾选 ≥ 1 就出现批量按钮（用户裁决：单选也要能操作）', () => {
    expect(showBatchActions(0)).toBe(false);
    expect(showBatchActions(1)).toBe(true);
    expect(showBatchActions(2)).toBe(true);
  });

  it('按钮文案带条数', () => {
    expect(batchLabel('reject', 3)).toBe('不投递（3）');
    expect(batchLabel('deliver', 3)).toBe('已投递（3）');
    // `allow` 落在两个页签上含义不同：不投递 → 撤销否决，已投递 → 取消投递。
    expect(batchLabel('allow', 2, 'rejected')).toBe('撤销否决（2）');
    expect(batchLabel('allow', 2, 'delivered')).toBe('取消投递（2）');
  });

  it('批量动作跟页签走', () => {
    expect(batchOps('pending')).toEqual(['reject', 'deliver']);
    expect(batchOps('rejected')).toEqual(['allow']);
    expect(batchOps('delivered')).toEqual(['allow']);
  });

  it('勾选集合保序且可反选', () => {
    expect(toggleChecked([], 'b')).toEqual(['b']);
    expect(toggleChecked(['b'], 'a')).toEqual(['b', 'a']);
    expect(toggleChecked(['b', 'a'], 'b')).toEqual(['a']);
  });

  it('全选状态按可见集合判断', () => {
    const list = [job({ job_id: 'a' }), job({ job_id: 'b' })];
    expect(allVisibleChecked(['a', 'b'], list.map((j) => j.job_id))).toBe(true);
    expect(allVisibleChecked(['a'], list.map((j) => j.job_id))).toBe(false);
    expect(allVisibleChecked([], [])).toBe(false);
  });
});

describe('卡片视图（裁决 R/V）', () => {
  it('分数档位按满分比例算，不硬编码 80/60/40', () => {
    expect(scoreTier(88, 100, false)).toBe('is-high');
    expect(scoreTier(66, 100, false)).toBe('is-mid');
    expect(scoreTier(45, 100, false)).toBe('is-low');
    expect(scoreTier(30, 100, false)).toBe('is-bad');
    // 满分 60 时，54 分就是 highest 档
    expect(scoreTier(54, 60, false)).toBe('is-high');
    expect(scoreTier(0, 100, false)).toBe('is-none');
    expect(scoreTier(88, null, false)).toBe('is-none');
    expect(scoreTier(88, 100, true)).toBe('is-done');
  });

  it('状态胶囊：待投递 / 已投递 / 不投递', () => {
    expect(statusChip(job({}))).toEqual({ tone: 'default', label: '待投递' });
    expect(statusChip(job({ status: 'delivered' }))).toEqual({ tone: 'primary', label: '已投递' });
    expect(statusChip(job({ rejected: 1 }))).toEqual({ tone: 'bad', label: '不投递' });
  });

  it('meta 行按 地点 / 经验 / 学历 / 规模 顺序，空值剔除', () => {
    expect(metaItems(job({}))).toEqual(['深圳', '3-5年', '本科', '500-999人']);
    expect(metaItems(job({ location: '', experience: '' }))).toEqual(['本科', '500-999人']);
  });

  it('公司地址只取第一行', () => {
    expect(addrFirstLine('深圳市南山区\n2 号楼')).toBe('深圳市南山区');
    expect(addrFirstLine('  ')).toBe('');
  });

  it('cardView 的 testid 与文案：tid_safe 去掉冒号斜杠', () => {
    const view = cardView(job({ job_id: 'boss:100/2' }), 100);
    expect(view.tid).toBe('boss-100-2');
    // 组件侧统一用 `job-card-${tid}` / `score-${tid}` 这套前缀拼 testid（JobCard.vue），
    // 模型只提供 tid，避免两处各拼一遍导致对不上。
    expect(view.scoreTier).toBe('is-high');
    expect(view.time).toBe(fmtTime(1_700_000_000_000));
    expect(view.metaText).toBe('深圳 / 3-5年 / 本科 / 500-999人');
    expect(view.hasAddr).toBe(false);
  });

  it('空字段回落占位文案', () => {
    const view = cardView(job({ title: '', company: '', salary: '' }), 100);
    expect(view.title).toBe('（无标题）');
    expect(view.company).toBe('（未知公司）');
    expect(view.salary).toBe('面议');
  });

  it('点卡片正文才开详情：按钮/输入/打招呼方块不算', () => {
    expect(shouldOpenCard('DIV')).toBe(true);
    expect(shouldOpenCard('SPAN')).toBe(true);
    expect(shouldOpenCard('BUTTON')).toBe(false);
    expect(shouldOpenCard('INPUT')).toBe(false);
    expect(shouldOpenCard('A')).toBe(false);
    expect(shouldOpenCard('TEXTAREA')).toBe(false);
  });
});

describe('保活锚点（C8：job-row-* / score-* 必须原样保留）', () => {
  it('每个岗位一行，含 5 个单元格', () => {
    const rows = keepaliveRows([job({ job_id: 'x' }), job({ job_id: 'y', match_score: 61 })]);
    expect(rows.map((r) => r.jobId)).toEqual(['x', 'y']);
    expect(rows[1]).toEqual({
      jobId: 'y',
      score: '61',
      title: '后端工程师',
      company: '星瀚科技',
      location: '深圳',
      time: fmtTime(1_700_000_000_000),
    });
  });
});
