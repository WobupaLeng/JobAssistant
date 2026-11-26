/**
 * `commands_company.rs` 的 3 条命令：搜公司 → 看 JD（只读）→ 加入待投递（写库）。
 *
 * 平铺参数的键名按 Tauri 默认转驼峰（Rust `job_id` → 前端 `jobId`）。
 */

import { invokeFlat } from '../invoke';
import type { CompanyJobsReply, Job } from '../types';

/** 按公司名去 Boss 搜岗位。只读，不落库。 */
export function companyJobs(company: string): Promise<CompanyJobsReply> {
  return invokeFlat<CompanyJobsReply>('company_jobs', { company });
}

/** 读一条岗位的 JD 正文用于预览。只读：不入库、不出现在任何列表里。 */
export function companyJobPreview(jobId: string): Promise<Job> {
  return invokeFlat<Job>('company_job_preview', { jobId });
}

/** 把一条搜到的岗位加进待投递列表（幂等：已在库直接返回原行）。 */
export function jobImport(args: {
  job_id: string;
  title?: string;
  company?: string;
  salary?: string;
  location?: string;
  experience?: string;
  degree?: string;
  job_url?: string;
  jd?: string;
}): Promise<Job> {
  // 参数名对齐 Rust 的 `JobImportArgs`：结构体反序列化按字段名，不吃驼峰别名
  // 之外的花样，所以这里显式写蛇形键。
  return invokeFlat<Job>('job_import', {
    jobId: args.job_id,
    title: args.title,
    company: args.company,
    salary: args.salary,
    location: args.location,
    experience: args.experience,
    degree: args.degree,
    jobUrl: args.job_url,
    jd: args.jd,
  });
}