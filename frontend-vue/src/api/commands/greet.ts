/**
 * `commands_greet.rs` 的 2 条命令（话术生成 / 单发）。
 */

import { invokeArgs, invokeFlat } from '../invoke';
import type { GreetReply } from '../types';

/**
 * 生成一段打招呼话术（**不发出去**）。载荷 `{ args }`，键名驼峰。
 * `useLlm` 恒为 `true`：用 LLM 还是规则模板由后端「启用大模型」开关决定。
 * 聊天页用 `findByCompany`（好友列表拿不到 `jd_text`）；「生成回复」额外带 `reply`。
 */
export function greetDraft(params: {
  jobId: string;
  title: string;
  company: string;
  salary: string;
  experience: string;
  jd: string;
  /** 聊天页用：按公司名从库里找岗位（补 JD）。与 `jobId` 互斥，`jobId` 优先。 */
  findByCompany?: string;
  /** HR 刚说的话（「生成回复」时传）。 */
  reply?: string;
}): Promise<GreetReply> {
  return invokeArgs<GreetReply>('greet_draft', {
    jobId: params.jobId,
    title: params.title,
    company: params.company,
    salary: params.salary,
    experience: params.experience,
    jd: params.jd,
    findByCompany: params.findByCompany ?? '',
    reply: params.reply ?? '',
    useLlm: true,
  });
}

/**
 * 把一段话术发给某个 HR，返回**真正发出去的正文**（后端可能按截断规则改写）。
 * 平铺 `{ uid, text }`。
 */
export function greetOne(uid: string, text: string): Promise<string> {
  return invokeFlat<string>('greet_one', { uid, text });
}
