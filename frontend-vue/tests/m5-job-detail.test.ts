import { describe, expect, it } from 'vitest';

import {
  EMPTY_DETAIL,
  closeDetail,
  dash,
  detailChips,
  detailActions,
  detailCompany,
  detailJdText,
  detailKv,
  detailLink,
  detailReason,
  detailScoreClass,
  detailScoreMaxText,
  detailScoreText,
  detailScoreTone,
  detailTitle,
  detailWhen,
  hiringText,
  hrText,
  isOpen,
  needsJdFetch,
  openDetail,
  passesThreshold,
  actionFailPrefix,
  actionNote,
  deliverAction,
  rejectAction,
} from '../src/lib_ui/job-detail-model';
import type { Job } from '../src/api/types';

function job(patch: Partial<Job> = {}): Job {
  const base = {
    job_id: 'j1',
    platform: 'boss',
    title: '后端工程师',
    company: '星瀚科技',
    salary: '20-30K',
    status: 'pending',
    created_at: 1_700_000_000_000,
    job_url: 'https://example.invalid/job/1',
    jd_text: '负责服务端开发。',
    location: '杭州',
    experience: '3-5年',
    degree: '本科',
    hr_name: '王女士',
    hr_position: 'HR',
    hr_active_status: '今日活跃',
    company_industry: '互联网',
    company_scale: '500-999人',
    company_stage: 'B 轮',
    company_address: '杭州市余杭区文一西路 969 号\n星瀚大厦 12F',
    company_intro: '',
    hiring_count: 3,
    founded_years: 8,
    match_score: 80,
    rejected: 0,
    score_source: 'rule',
    score_reason: '技能匹配',
    updated_at: 1_700_000_100_000,
    score_scale: 100,
  } as unknown as Job;
  return { ...base, ...patch };
}

describe('job-detail-model / 空值口径', () => {
  it('dash 把空白折成破折号', () => {
    expect(dash('')).toBe('—');
    expect(dash('   ')).toBe('—');
    expect(dash(null)).toBe('—');
    expect(dash('x')).toBe('x');
  });

  it('标题/公司缺省有兜底', () => {
    expect(detailTitle(job({ title: '  ' }))).toBe('（无标题）');
    expect(detailCompany(job({ company: '' }))).toBe('—');
  });
});

describe('job-detail-model / 分数口径（C10：满分与门槛都来自参数）', () => {
  it('分数为 0 显示破折号', () => {
    expect(detailScoreText(0)).toBe('—');
    expect(detailScoreText(72)).toBe('72');
  });

  it('满分未知时显示破折号', () => {
    expect(detailScoreMaxText(null)).toBe('/ — 分');
    expect(detailScoreMaxText(0)).toBe('/ — 分');
    expect(detailScoreMaxText(100)).toBe('/ 100 分');
  });

  it('色阶由代码算出（与列表卡同一套 scoreTone）', () => {
    // detailScoreTone 不接收满分：满分只影响 `/ N 分` 的展示，色阶由后端分档
    expect(detailScoreTone(80, 70)).toBe('pass');
    expect(detailScoreTone(60, 70)).toBe('fail');
    expect(detailScoreTone(0, 70)).toBe('none');
    expect(detailScoreClass(80, 70)).toBe('score-num drawer-score is-pass');
  });

  it('门槛未知或分数为 0 时不给判定', () => {
    expect(passesThreshold(80, null)).toBeNull();
    expect(passesThreshold(0, 70)).toBeNull();
    expect(passesThreshold(80, 70)).toBe(true);
    expect(passesThreshold(60, 70)).toBe(false);
  });
});

describe('job-detail-model / 头部 chip 与理由', () => {
  it('chip 顺序：来源 → 门槛 → 制式 → 平台', () => {
    const chips = detailChips(job(), 70, 100);
    expect(chips.map((c) => c.text)).toEqual(['rule', '达到门槛 70', '制式 100', 'boss']);
    expect(chips[0].tone).toBe('accent');
    expect(chips[1].tone).toBe('primary');
    expect(chips[2].tone).toBe('default');
  });

  it('低于门槛时 chip 转 bad', () => {
    const chips = detailChips(job({ match_score: 55 }), 70, 100);
    expect(chips[1].tone).toBe('bad');
    expect(chips[1].text).toBe('低于门槛 70');
    expect(chips[1].title).toBe('分数低于当前入库门槛');
  });

  it('门槛未知、满分为空时不出对应 chip', () => {
    expect(detailChips(job(), null, null).map((c) => c.text)).toEqual(['rule', 'boss']);
  });

  it('理由空时按读取态给文案', () => {
    expect(detailReason(job({ score_reason: '' }), true)).toBe('读取中…');
    expect(detailReason(job({ score_reason: '  ' }), false)).toBe('无打分说明');
    expect(detailReason(job({ score_reason: '技能匹配' }), false)).toBe('技能匹配');
  });
});

describe('job-detail-model / JD 四态降级（顺序不能换）', () => {
  it('有原文给原文', () => {
    expect(detailJdText(job(), false, '')).toBe('负责服务端开发。');
  });

  it('加载中且无错误时是读取中', () => {
    expect(detailJdText(job({ jd_text: '' }), true, '')).toBe('读取中…');
  });

  it('有错误时优先报错误', () => {
    expect(detailJdText(job({ jd_text: '' }), true, '网络错误')).toBe(
      '（读取职位描述失败：网络错误）',
    );
  });

  it('本来就没有 JD', () => {
    expect(detailJdText(job({ jd_text: '' }), false, '')).toBe(
      '（该岗位没有 JD 原文，抓取时通常按「无 JD」降权处理）',
    );
  });

  it('只有 jd_text 为空才需要补拉', () => {
    expect(needsJdFetch(job({ jd_text: '' }))).toBe(true);
    expect(needsJdFetch(job())).toBe(false);
  });
});

describe('job-detail-model / kv 十行', () => {
  it('行序与 Leptos 一致', () => {
    expect(detailKv(job()).map((r) => r.label)).toEqual([
      '经验',
      '学历',
      '行业',
      '规模',
      '阶段',
      '在招',
      'HR',
      '状态',
      '入库',
      '链接',
    ]);
  });

  it('在招与 HR 的组合口径', () => {
    expect(hiringText(3)).toBe('3 个岗位');
    expect(hiringText(0)).toBe('—');
    expect(hrText(job())).toBe('王女士 · HR');
    expect(hrText(job({ hr_position: '' }))).toBe('王女士');
    expect(hrText(job({ hr_name: '  ' }))).toBe('HR');
    expect(hrText(job({ hr_name: '', hr_position: '' }))).toBe('—');
  });

  it('HR 活跃度进 chip，状态行在被否决时带 chip', () => {
    const rows = detailKv(job({ rejected: 1, status: 'rejected' }));
    expect(rows.find((r) => r.label === 'HR')!.chip?.text).toBe('今日活跃');
    expect(rows.find((r) => r.label === '状态')!.chip?.text).toBe('已排除');
    expect(detailKv(job({ hr_active_status: '' })).find((r) => r.label === 'HR')!.chip).toBeUndefined();
  });

  it('入库时间优先用 updated_at，全为 0 时破折号', () => {
    expect(detailWhen(job())).toMatch(/^\d{2}-\d{2} \d{2}:\d{2}$/);
    expect(detailWhen(job({ updated_at: 0, created_at: 0 }))).toBe('—');
  });

  it('链接缺失时给 null', () => {
    expect(detailLink(job({ job_url: '' }))).toBeNull();
    expect(detailLink(job())).toBe('https://example.invalid/job/1');
  });
});

describe('job-detail-model / 动作口径', () => {
  it('未否决时首按钮是不投递', () => {
    const a = rejectAction(job());
    expect(a.testId).toBe('job-detail-reject-btn');
    expect(a.label).toBe('不投递');
    expect(a.btnClass).toContain('btn-danger');
    expect(a.disabled).toBe(false);
  });

  it('已否决时首按钮反向为允许投递', () => {
    const a = rejectAction(job({ rejected: 1 }));
    expect(a.testId).toBe('job-detail-allow-btn');
    expect(a.label).toBe('允许投递');
    expect(a.btnClass).toContain('btn-ghost');
  });

  it('已投递时标记按钮禁用', () => {
    const a = deliverAction(job({ status: 'delivered' }));
    expect(a.disabled).toBe(true);
    expect(a.label).toBe('已标记投递');
    expect(deliverAction(job()).disabled).toBe(false);
    expect(deliverAction(job()).label).toBe('标记已投递');
  });

  it('成功文案与失败前缀（逐字）', () => {
    expect(actionNote('reject', 1)).toBe('已标为不投递：影响 1 条');
    expect(actionNote('allow', 1)).toBe('已恢复投递：影响 1 条');
    expect(actionNote('deliver', 1)).toBe('已标记投递');
    expect(actionFailPrefix('reject')).toBe('标记不投递失败：');
    expect(actionFailPrefix('allow')).toBe('恢复投递失败：');
    expect(actionFailPrefix('deliver')).toBe('标记投递失败：');
  });

  it('抽屉底部按钮跟页签走（与列表批量条同口径）', () => {
    const pending = detailActions(job(), 'pending');
    expect(pending.map((a) => a.action)).toEqual(['reject', 'deliver']);
    const rejected = detailActions(job({ rejected: 1 }), 'rejected');
    expect(rejected).toHaveLength(1);
    expect(rejected[0].action).toBe('allow');
    expect(rejected[0].label).toBe('撤销否决');
    const delivered = detailActions(job({ status: 'delivered' }), 'delivered');
    expect(delivered).toHaveLength(1);
    expect(delivered[0].action).toBe('undo-deliver');
    expect(delivered[0].label).toBe('取消投递');
  });
});

describe('job-detail-model / 开关状态', () => {
  it('打开后 isOpen 为真，同一个 id 不重建 state', () => {
    const s1 = openDetail(EMPTY_DETAIL, 'a');
    expect(isOpen(s1, 'a')).toBe(true);
    expect(isOpen(s1, 'b')).toBe(false);
    expect(openDetail(s1, 'a')).toBe(s1);
    expect(openDetail(s1, 'b').jobId).toBe('b');
  });

  it('关闭后回到空态', () => {
    expect(closeDetail().jobId).toBeNull();
    expect(isOpen(closeDetail(), 'a')).toBe(false);
  });

  it('纯函数：不修改入参 state', () => {
    const s = openDetail(EMPTY_DETAIL, 'a');
    const snapshot = { ...s };
    openDetail(s, 'b');
    closeDetail();
    expect(s).toEqual(snapshot);
    expect(EMPTY_DETAIL.jobId).toBeNull();
  });
});
