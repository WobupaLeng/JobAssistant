/**
 * 岗位域内的判定与格式化。纯函数，不依赖框架。字段与 `api/types.ts` 的 `Job` 逐字一致、
 * **蛇形命名**：DTO 字段不做驼峰转换，只有调用入参用驼峰。
 */

/**
 * 岗位实体（界面层视角）。改字段时必须同一次改动里核对
 * `backend-tauri/src/core/domain/job.rs`：少一个字段不会报错，只会在运行时显示空白。
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

/**
 * 把后端回来的岗位补齐成 `Job`。Rust 侧大量字段是 `Option<T>`，序列化后是 `null`，而界面层
 * 按 `string` / `number` 消费：不补的话 `jd_text` 为 `null` 时 `.trim()` 会抛 TypeError，
 * 详情抽屉一片空白。只做「null → 默认值」，不改任何非空值。
 */
export function normalizeJob(raw: Partial<Job> | null | undefined): Job {
  const base = emptyJob();
  if (raw === null || raw === undefined) return base;
  const out = { ...base };
  for (const key of Object.keys(base) as (keyof Job)[]) {
    const v = raw[key];
    if (v === null || v === undefined) continue;
    // 类型上应当一致；后端偶发把数字字段发成字符串也给兜住，免得比较时静默变 false。
    if (typeof base[key] === 'number') {
      const n = Number(v);
      (out[key] as number) = Number.isFinite(n) ? n : 0;
      continue;
    }
    (out[key] as string) = typeof v === 'string' ? v : String(v);
  }
  return out;
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
 * 分数落在门槛的哪一侧：用于 CSS 类 `score-num is-{tone}`。`score === 0` 视为「未打分」，
 * 否则会渲染成刺眼的失败红；门槛未知（`null`）时一律按通过处理。
 */
export type ScoreTone = 'pass' | 'fail' | 'none';

export function scoreTone(score: number, minScore: number | null): ScoreTone {
  if (score === 0) return 'none';
  if (minScore === null) return 'pass';
  return score >= minScore ? 'pass' : 'fail';
}
