/**
 * 侧边车（sidecar）会话的**可用性判定**。命令层在「sidecar 还没起来」时返回的是中文
 * 文案而非错误码（见 `commands_chat.rs` 的 `handle_of`），期望 / 聊天 / 会话三个面板都
 * 据此决定「要不要自己拉起浏览器」。将来若改为结构化错误，只需改这里。
 */

export const SESSION_MISSING_MARKERS: readonly string[] = ['尚未启动', '没有会话'];

/** 这条错误是不是「sidecar 没有会话」。 */
export function needsSession(message: string): boolean {
  return SESSION_MISSING_MARKERS.some((m) => message.includes(m));
}
