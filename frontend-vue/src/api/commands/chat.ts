/**
 * `commands_chat.rs` 的 7 条命令。
 *
 * 全部平铺（键名 = Rust 参数名，需要驼峰的 `resumeId` 由 `alias` 兜），
 * 唯一例外是「一给就是整份 XReply」的读取接口，本来也无参数形状可挑。
 */

import { invokeFlat, invokeNone } from '../invoke';
import type {
  Expectation,
  FriendsReply,
  MessagesReply,
  OkReply,
  ResumesReply,
  SendReply,
} from '../types';

/** 好友（HR）列表。 */
export function chatFriends(): Promise<FriendsReply> {
  return invokeNone<FriendsReply>('chat_friends');
}

/** 与某个 HR 的消息记录。平铺 `{ uid }`。 */
export function chatMessages(uid: string): Promise<MessagesReply> {
  return invokeFlat<MessagesReply>('chat_messages', { uid });
}

/**
 * 发一条消息给 HR。
 *
 * 平铺：`{ uid, text, jobId? }`。**`jobId` 非空时后端会顺带把该岗位标记为已投递**
 * （「聊过就算投过」的唯一入口，前端不要再补一次 `mark_delivered`）。
 *
 * 回执的 `text` 是**真正发出去的正文**（后端可能截断），列表要显示它而不是本地输入。
 */
export function chatSend(uid: string, text: string, jobId?: string | null): Promise<SendReply> {
  const trimmed = jobId?.trim() ?? '';
  return invokeFlat<SendReply>('chat_send', {
    uid,
    text,
    jobId: trimmed.length > 0 ? trimmed : undefined,
  });
}

/** 简历列表（用于「发简历」）。 */
export function chatResumes(): Promise<ResumesReply> {
  return invokeNone<ResumesReply>('chat_resumes');
}

/** 发一份简历。平铺 `{ uid, resumeId }`。 */
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
