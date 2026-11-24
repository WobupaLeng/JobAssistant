/**
 * `commands_greet.rs` 的 2 条命令（话术生成 / 单发）。
 */

import { invokeArgs, invokeFlat } from '../invoke';
import type { GreetReply } from '../types';

/**
 * 生成一段打招呼话术（**不发出去**）。
 *
 * 载荷结构：`{ args: GreetArgs }`，键名驼峰（`jobId` / `findByCompany` / `reply` / `useLlm`）。
 * `useLlm` 恒为 `true`：前端写死 `true`，走后端配置里的
 * 「启用大模型」开关来判断最终用 LLM 还是规则模板——前端不参与这个决策。
 *
 * 三种用法：
 * - 首次：概览卡片带 `jobId`；聊天页带 `findByCompany`（后端按公司名翻库里的完整 JD，
 *   因为好友列表拿不到 `jd_text`）；
 * - 重新生成：同一份入参再调一次，模型会给出不同措辞；
 * - 生成回复：额外带 `reply`（HR 刚说的话），提示词改成「接话」而不是自我介绍。
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
 * 把一段话术发给某个 HR，返回**真正发出去的正文**。
 *
 * 平铺：`{ uid, text }`。发出去时后端会按截断规则改写文本，
 * 列表要显示回执而不是本地输入。
 */
export function greetOne(uid: string, text: string): Promise<string> {
  return invokeFlat<string>('greet_one', { uid, text });
}
