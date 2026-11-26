/**
 * 「按公司搜岗位」的展示模型（`src/lib_ui/company-search.ts`）。
 *
 * 只测纯函数：不挂组件、不打接口。断言的是**用户能看到的句子**——
 * 这些句子决定了用户会不会误以为「包含匹配」搜到的是同名公司（用户 m01529）。
 */

import { describe, expect, it } from 'vitest';

import type { CompanyJobItem, CompanyJobsReply, Job } from '../src/api/types';
import { emptyJob } from '../src/lib_ui/job';
import {
  emptyNote,
  headline,
  idleSearch,
  importNote,
  jobMeta,
  normalizeCompany,
  searchFromReply,
} from '../src/lib_ui/company-search';

function row(over: Partial<CompanyJobItem> = {}): CompanyJobItem {
  return {
    job_id: 'enc1',
    title: 'Java开发工程师（AI方向）',
    salary: '12-24K',
    company: '中安瑞智',
    location: '深圳',
    experience: '3-5年',
    degree: '本科',
    url: 'https://www.zhipin.com/job_detail/enc1.html',
    ...over,
  };
}

function reply(over: Partial<CompanyJobsReply> = {}): CompanyJobsReply {
  return { company: '中安瑞智', matched: 'exact', jobs: [row()], ...over };
}

describe('输入归一', () => {
  it('只去首尾空白，保留中间（公司名里的空格有意义）', () => {
    expect(normalizeCompany('  中安瑞智  ')).toBe('中安瑞智');
    expect(normalizeCompany(' 中 安 ')).toBe('中 安');
    expect(normalizeCompany('')).toBe('');
  });
});

describe('searchFromReply：回执 → 展示状态', () => {
  it('一律落成 done，并原样带上公司名与匹配方式', () => {
    const s = searchFromReply(reply({ matched: 'contains' }));
    expect(s.phase).toBe('done');
    expect(s.company).toBe('中安瑞智');
    expect(s.matched).toBe('contains');
    expect(s.jobs).toHaveLength(1);
    expect(s.error).toBe('');
  });
});

describe('headline：结果标题', () => {
  it('还没搜过时是空的（浮层里不显示半句话）', () => {
    expect(headline(idleSearch())).toBe('');
  });

  it('加载中说清在搜谁', () => {
    expect(headline({ ...idleSearch(), phase: 'loading', company: '中安瑞智' })).toContain('中安瑞智');
  });

  it('严格同名：报总数', () => {
    const s = searchFromReply(reply({ jobs: [row(), row({ job_id: 'enc2' })] }));
    expect(headline(s)).toBe('「中安瑞智」共 2 个在招岗位');
  });

  it('包含匹配必须点明「没有完全同名」，否则用户会以为是同一家公司', () => {
    const s = searchFromReply(reply({ company: '字节', matched: 'contains' }));
    expect(headline(s)).toContain('没有完全同名的公司');
    expect(headline(s)).toContain('字节');
  });

  it('两种情况都没结果时各说各的', () => {
    expect(headline(searchFromReply(reply({ jobs: [] })))).toBe('没有搜到「中安瑞智」在招岗位');
    expect(headline(searchFromReply(reply({ company: '字节', matched: 'contains', jobs: [] })))).toContain(
      '包含',
    );
  });

  it('出错时直接显示整句错误（不再叠一层「搜到 0 个」）', () => {
    const s = { ...idleSearch(), phase: 'error' as const, error: '搜索失败：浏览器已关闭' };
    expect(headline(s)).toBe('搜索失败：浏览器已关闭');
  });
});

describe('emptyNote：空态的下一步', () => {
  it('只有 done 且零结果时给提示', () => {
    expect(emptyNote({ ...idleSearch(), phase: 'loading' })).toBe('');
    expect(emptyNote(searchFromReply(reply()))).toBe('');
    expect(emptyNote(searchFromReply(reply({ jobs: [] })))).toContain('公司全名');
  });

  it('包含匹配落空时提示用更完整的公司名', () => {
    expect(emptyNote(searchFromReply(reply({ company: '字节', matched: 'contains', jobs: [] })))).toContain(
      '更完整',
    );
  });
});

describe('jobMeta：一行元信息', () => {
  it('地点 · 经验 · 学历，缺的自动跳过不留空档', () => {
    expect(jobMeta(row())).toBe('深圳 · 3-5年 · 本科');
    expect(jobMeta(row({ experience: '', degree: '' }))).toBe('深圳');
    expect(jobMeta(row({ location: '', experience: '', degree: '' }))).toBe('');
  });
});

/**
 * 公司搜索回的岗位可能**还没打分**：`match_score` / `title` 是 null，而界面层的 `Job`
 * 是「已补齐」的形状（`emptyJob` 全是空串与 0）。这里显式跨过类型边界，
 * 测的正是 `importNote` 的 null 分支——不补就会打印「（0 分）」。
 */
function unscoredJob(over: { title?: string | null; match_score?: number | null } = {}): Job {
  return { ...emptyJob(), ...over } as unknown as Job;
}

describe('importNote：加入成功的提示', () => {
  it('带分数，方便和列表里的分值对照', () => {
    const job = unscoredJob({ title: 'Java开发工程师', match_score: 72 });
    expect(importNote(job)).toBe('已加入待投递：Java开发工程师（72 分）');
  });

  it('没有分数时只说岗位名', () => {
    const job = unscoredJob({ title: 'Java开发工程师', match_score: null });
    expect(importNote(job)).toBe('已加入待投递：Java开发工程师');
  });

  it('标题缺失时给一个能读的占位', () => {
    const job = unscoredJob({ title: null, match_score: null });
    expect(importNote(job)).toContain('这个岗位');
  });
});