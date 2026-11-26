/**
 * 「按公司搜岗位」浮层的**纯展示模型**：不碰网络、不碰组件状态，只把
 * 后端返回的形状翻译成界面上的几句话。单测直接覆盖这里（`tests/company-search.test.ts`）。
 */

import type { CompanyJobItem, CompanyJobsReply, Job } from '../api/types';

/** 搜索浮层的一次请求状态。 */
export interface SearchState {
  /** `idle` = 还没搜过；`loading` 时界面显示骨架，不清空上一次结果 */
  phase: 'idle' | 'loading' | 'done' | 'error';
  /** 这次搜的是哪家公司（原样回显用户输入） */
  company: string;
  /** `exact` / `contains` / `none`，与后端一致 */
  matched: string;
  jobs: readonly CompanyJobItem[];
  /** `phase === 'error'` 时的整句提示 */
  error: string;
}

export function idleSearch(): SearchState {
  return { phase: 'idle', company: '', matched: 'none', jobs: [], error: '' };
}

/** 用户输入归一：只去首尾空白，不改中间（公司名里的空格是有意义的）。 */
export function normalizeCompany(text: string): string {
  return (text ?? '').trim();
}

/** 从后端回执构造展示状态。 */
export function searchFromReply(reply: CompanyJobsReply): SearchState {
  return {
    phase: 'done',
    company: reply.company,
    matched: reply.matched,
    jobs: reply.jobs,
    error: '',
  };
}

/**
 * 结果标题。
 *
 * 严格同名时说「共 N 个岗位」；包含匹配必须点明「名称包含」并让人看清每行公司名
 * ——否则用户会以为「搜字节」搜出来的都是字节跳动本部（用户 m01529 的裁决）。
 */
export function headline(state: SearchState): string {
  if (state.phase === 'idle') return '';
  if (state.phase === 'loading') return `正在搜「${state.company}」…`;
  if (state.phase === 'error') return state.error;
  const n = state.jobs.length;
  if (state.matched === 'contains') {
    return n === 0
      ? `没有公司名包含「${state.company}」的岗位`
      : `没有完全同名的公司，下面 ${n} 个岗位的公司名包含「${state.company}」`;
  }
  if (n === 0) return `没有搜到「${state.company}」在招岗位`;
  return `「${state.company}」共 ${n} 个在招岗位`;
}

/** 空态补充说明（`headline` 已给结论，这里只给下一步做什么）。 */
export function emptyNote(state: SearchState): string {
  if (state.phase !== 'done' || state.jobs.length > 0) return '';
  if (state.matched === 'contains') {
    return '换一个更完整的公司全名再试一次。';
  }
  return '确认公司全名是否正确；也可能这家公司当前没有在招岗位。';
}

/** 一行岗位的元信息：地点 · 经验 · 学历（缺的自动跳过，不留空档）。 */
export function jobMeta(job: CompanyJobItem): string {
  return [job.location, job.experience, job.degree]
    .map((v) => (v ?? '').trim())
    .filter((v) => v !== '')
    .join(' · ');
}

/** 「加入待投递」成功后的提示句；带分数，方便对照列表里的分值。 */
export function importNote(job: Job): string {
  const title = (job.title ?? '').trim() || '这个岗位';
  const score = job.match_score;
  if (score === null || score === undefined) {
    return `已加入待投递：${title}`;
  }
  return `已加入待投递：${title}（${score} 分）`;
}