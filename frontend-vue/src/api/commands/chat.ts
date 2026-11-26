/**
 * `commands_chat.rs` 的 7 条命令。除无参读取接口外全部平铺，键名 = Rust 参数名的
 * **驼峰**形式（Tauri 的 `#[tauri::command]` 默认按 camelCase 取键）。
 */

import { invokeFlat, invokeNone } from '../invoke';
import type {
  Expectation,
  FriendsReply,
  Job,
  MessagesReply,
  OkReply,
  OpenReply,
  ResumesReply,
  SendReply,
} from '../types';

/** 好友（HR）列表。 */
export function chatFriends(): Promise<FriendsReply> {
  return invokeNone<FriendsReply>('chat_friends');
}

/** 本地缓存的好友列表（不触发浏览器，供先展示）。 */
export function chatFriendsLocal(): Promise<FriendsReply> {
  return invokeNone<FriendsReply>('chat_friends_local');
}

/** 本地缓存的某会话消息（不触发浏览器，供先展示）。 */
export function chatMessagesLocal(uid: string): Promise<MessagesReply> {
  return invokeFlat<MessagesReply>('chat_messages_local', { uid });
}

/**
 * 为某个岗位在 Boss 上**建立会话**，返回真实 uid。只给「还没打过招呼」的岗位用。
 * 它等价于用户手动点「立即沟通」，所以只能由点击触发，**不得在挂载/轮询里自动调用**。
 */
export function chatOpen(jobId: string): Promise<OpenReply> {
  // 平铺参数的键名按 Tauri 默认转驼峰（Rust `job_id` → 前端 `jobId`）。
  // 发蛇形必填参数会直接报 `missing required key jobId`，不是「静默丢弃」。
  return invokeFlat<OpenReply>('chat_open', { jobId });
}

/** 与某个 HR 的消息记录。平铺 `{ uid }`。 */
export function chatMessages(uid: string): Promise<MessagesReply> {
  return invokeFlat<MessagesReply>('chat_messages', { uid });
}

/**
 * 按岗位加密 id 把 JD **补抓**进库，返回这条岗位。
 *
 * 只给「会话行带着 `encryptJobId`、但岗位从没进过抓取队列」的行用（用户 m00824）：
 * 那种行没有岗位详情也没有 JD，话术只能空写。已在库的岗位后端直接返回，不重抓、
 * 不覆盖、不改状态，所以对已经关联正确的会话零影响。
 *
 * 属于「点开才发生」的浏览器动作，**不得在挂载/轮询里自动调用**。
 */
export function jobFetchDetail(jobId: string): Promise<Job> {
  return invokeFlat<Job>('job_fetch_detail', { jobId });
}

/**
 * 发一条消息给 HR。平铺 `{ uid, text, jobId? }`。`jobId` 非空时后端顺带把该岗位标记为
 * 已投递（「聊过就算投过」的唯一入口，前端不要再补一次 `mark_delivered`）。
 * 回执的 `text` 是**真正发出去的正文**（可能被截断），列表要显示它。
 */
export function chatSend(uid: string, text: string, jobId?: string | null): Promise<SendReply> {
  const trimmed = jobId?.trim() ?? '';
  return invokeFlat<SendReply>('chat_send', {
    uid,
    text,
    // 键名必须是驼峰：发蛇形 `job_id` 会被当成未知键，而它是 `Option<String>`，
    // 缺键静默变 `None` ——「聊过就算投过」会无声失效，连报错都没有。
    jobId: trimmed.length > 0 ? trimmed : undefined,
  });
}

/** 简历列表（用于「发简历」）。 */
export function chatResumes(): Promise<ResumesReply> {
  return invokeNone<ResumesReply>('chat_resumes');
}

/** 发一份简历。平铺 `{ uid, resumeId }`（键名 = Rust `resume_id` 的驼峰）。 */
export function chatSendResume(uid: string, resumeId: string): Promise<OkReply> {
  return invokeFlat<OkReply>('chat_send_resume', { uid, resumeId });
}

/** 撤回会话（关闭与某个 HR 的聊天窗口）。平铺 `{ uid }`。 */
export function chatRecall(uid: string): Promise<OkReply> {
  return invokeFlat<OkReply>('chat_recall', { uid });
}

/** 解析出的求职期望（从 Boss 的求职意向页面）。 */
export function listExpectations(): Promise<Expectation[]> {
  return invokeNone<Expectation[]>('list_expectations');
}
