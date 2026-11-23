/**
 * `commands_greet.rs` 的 2 条命令（话术生成 / 单发）。
 */

import { invokeArgs, invokeFlat } from '../invoke';
import type { GreetReply } from '../types';

/**
 * 生成一段打招呼话术（**不发出去**）。
 *
 * 载荷结构：`{ args: GreetArgs }`，键名驼峰（`jobId` / `useLlm`）。
 * `useLlm` 恒为 `true`：Leptos 版也是写死 `true`，走后端配置里的
 * 「启用大模型」开关来判断最终用 LLM 还是规则模板——前端不参与这个决策。
 */
export function greetDraft(params: {
  jobId: string;
  title: string;
  company: string;
  salary: string;
  experience: string;
  jd: string;
}): Promise<GreetReply> {
  return invokeArgs<GreetReply>('greet_draft', {
    jobId: params.jobId,
    title: params.title,
    company: params.company,
    salary: params.salary,
    experience: params.experience,
    jd: params.jd,
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
