/**
 * 侧边车（sidecar）会话的**可用性判定**。
 *
 * 为什么单独成文件：命令层（`commands_scrape.rs` / `commands_chat.rs`）在
 * 「sidecar 还没起来」时返回的是一段**中文文案**，不是错误码。前端三个面板
 * （期望 / 聊天 / 会话）都要据此决定「要不要自己拉起浏览器」，此前这段判据
 * 被复制了三份，任何一处漏改都会让某个面板在没会话时直接报错而不是自愈。
 *
 * 判据用文案而非错误码，是因为 Rust 侧把提示拼进了返回字符串（见
 * `commands_chat.rs` 的 `handle_of`）。若将来改为结构化错误，只需改这里。
 */

const SESSION_MISSING_MARKERS = ['尚未启动', '没有会话'];

/** 这条错误是不是「sidecar 没有会话」。 */
export function needsSession(message: string): boolean {
  return SESSION_MISSING_MARKERS.some((m) => message.includes(m));
}
