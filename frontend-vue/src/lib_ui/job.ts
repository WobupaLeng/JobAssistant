/**
 * 岗位域内的判定与格式化。纯函数，不依赖框架。
 *
 * 字段与 `api/types.ts` 的 `Job`（= Rust 侧 serde 输出）逐字一致，**蛇形命名**：
 * 契约只此一份，DTO 字段不做驼峰转换（只有调用入参用驼峰，见 AGENTS.md C3）。
 */

/**
 * 岗位实体（界面层视角）。
 *
 * 改字段时必须同一次改动里核对 `backend-tauri/src/core/domain/job.rs`：
 * 这里少一个字段不会报错，只会在运行时显示空白。
 */
export interface Job {
  job_id: string;
  platform: string;
  title: string;
  company: string;
  salary: string;
  status: string;
  created_at: number;
  job_url: string;
  jd_text: string;
  location: string;
  experience: string;
  degree: string;
  hr_name: string;
  hr_position: string;
  hr_active_status: string;
  company_industry: string;
  company_scale: string;
  company_stage: string;
  company_address: string;
  company_intro: string;
  hiring_count: number;
  founded_years: number;
  match_score: number;
  rejected: number;
  score_source: string;
  score_reason: string;
  updated_at: number;
  score_scale: number;
}

/** 空岗位：表单初值 / 占位，字段与后端一致（全空串与 0）。 */
export function emptyJob(): Job {
  return {
    job_id: '',
    platform: '',
    title: '',
    company: '',
    salary: '',
    status: '',
    created_at: 0,
    job_url: '',
    jd_text: '',
    location: '',
    experience: '',
    degree: '',
    hr_name: '',
    hr_position: '',
    hr_active_status: '',
    company_industry: '',
    company_scale: '',
    company_stage: '',
    company_address: '',
    company_intro: '',
    hiring_count: 0,
    founded_years: 0,
    match_score: 0,
    rejected: 0,
    score_source: '',
    score_reason: '',
    updated_at: 0,
    score_scale: 0,
  };
}

/**
 * 规则否决的岗位。`rejected` 是 number 不是 boolean（史遗留列），非 0 即否决。
 *
 * SQLite 没有布尔列——这个判定只看 `!= 0`，不要改成真值判断
 * （`-1` 等非 0 值同样是「被排除」）。
 */
export function isRejected(job: Job): boolean {
  return job.rejected !== 0;
}

/**
 * 分数落在门槛的哪一侧：用于 CSS 类 `score-num is-{tone}`。
 *
 * `score === 0` 视为「未打分」，必须与「0 分」区分开——否则未打分岗位
 * 会被渲染成刺眼的失败红。门槛未知（`null`）时一律按通过处理，
 * 因为此时还无从判断。
 */
export type ScoreTone = 'pass' | 'fail' | 'none';

export function scoreTone(score: number, minScore: number | null): ScoreTone {
  if (score === 0) return 'none';
  if (minScore === null) return 'pass';
  return score >= minScore ? 'pass' : 'fail';
}
