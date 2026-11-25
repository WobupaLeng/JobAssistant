/**
 * `commands_chat.rs` 的 7 条命令。除无参读取接口外全部平铺，键名 = Rust 参数名。
 */

import { invokeFlat, invokeNone } from '../invoke';
import type {
  Expectation,
  FriendsReply,
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
  // 平铺参数键名 = Rust 参数名（`job_id`），Tauri 平铺参数不支持 alias。
  return invokeFlat<OpenReply>('chat_open', { job_id: jobId });
}

/** 与某个 HR 的消息记录。平铺 `{ uid }`。 */
export function chatMessages(uid: string): Promise<MessagesReply> {
  return invokeFlat<MessagesReply>('chat_messages', { uid });
}

/**
 * 发一条消息给 HR。平铺 `{ uid, text, job_id? }`。`job_id` 非空时后端顺带把该岗位标记为
 * 已投递（「聊过就算投过」的唯一入口，前端不要再补一次 `mark_delivered`）。
 * 回执的 `text` 是**真正发出去的正文**（可能被截断），列表要显示它。
 */
export function chatSend(uid: string, text: string, jobId?: string | null): Promise<SendReply> {
  const trimmed = jobId?.trim() ?? '';
  return invokeFlat<SendReply>('chat_send', {
    uid,
    text,
    // 键名必须是蛇形：发 `jobId` 会被当成未知键静默丢弃，「聊过就算投过」永不生效。
    job_id: trimmed.length > 0 ? trimmed : undefined,
  });
}

/** 简历列表（用于「发简历」）。 */
export function chatResumes(): Promise<ResumesReply> {
  return invokeNone<ResumesReply>('chat_resumes');
}

/** 发一份简历。平铺 `{ uid, resume_id }`（键名同 Rust 参数名）。 */
export function chatSendResume(uid: string, resumeId: string): Promise<OkReply> {
  return invokeFlat<OkReply>('chat_send_resume', { uid, resume_id: resumeId });
}

/** 撤回会话（关闭与某个 HR 的聊天窗口）。平铺 `{ uid }`。 */
export function chatRecall(uid: string): Promise<OkReply> {
  return invokeFlat<OkReply>('chat_recall', { uid });
}

/** 解析出的求职期望（从 Boss 的求职意向页面）。 */
export function listExpectations(): Promise<Expectation[]> {
  return invokeNone<Expectation[]>('list_expectations');
}
