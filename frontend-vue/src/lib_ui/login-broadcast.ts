/**
 * 登录态广播的**确认 + 重试**（用户 m02345：「广播加入 ack 机制，最大重试 3 次」）。
 *
 * 为什么需要：`login-state` 是**旁路推送**——后端 `lib.rs` 的 watcher 只在
 * `(platform, state)` 变化时播一次。推送不是可靠传输，下面三种情况都会让一次
 * 变化被吞掉，界面就停在旧快照上：
 * 1. 订阅晚于广播（挂载顺序 / WebView 恢复）；
 * 2. WebView 当时在后台，事件被丢弃；
 * 3. 同一次登录里状态连跳两拍（`waiting → logged_in`），前一拍还没处理完。
 *
 * 所以「收到广播」不等于「这件事办成了」。这里在**消费方**加一层确认：
 * 收到一次广播 → 执行动作 → 动作自己回报成功与否；失败就按退避重试，
 * 最多 `LOGIN_ACK_MAX_RETRIES` 次。重试的是**消费方动作**（清提示、开心跳），
 * 不重发广播本身——广播由后端负责，前端只保证「我这边确实落账了」。
 *
 * 不做定时轮询（用户 m02245 已明确删掉 5 秒兜底）：重试是**有限次、跟着一次
 * 真实广播走**的，不会自己唤醒。
 */

/** 最大重试次数（不含首次尝试）。用户口径：「最大重试 3 次」。 */
export const LOGIN_ACK_MAX_RETRIES = 3;

/** 第 n 次重试前的等待毫秒（退避：300 / 900 / 1800）。 */
export function loginAckDelay(retry: number): number {
  const n = Math.max(1, Math.min(LOGIN_ACK_MAX_RETRIES, retry));
  return 300 * n * n;
}

/**
 * 让一次广播处理带上确认语义。
 *
 * `run` 返回 `true` 表示这一步确实生效（例如「提示确实清空了」/「心跳确实开起来了」），
 * 返回 `false` 或抛错都算未确认，会按 `loginAckDelay` 退避重试。
 *
 * `onGiveUp` 在 3 次重试都失败后调用一次，让调用方自己决定怎么回到可用状态
 * （例如让用户手动点一次刷新），**不在这里偷偷起定时器**。
 */
export async function withLoginAck(
  run: (attempt: number) => Promise<boolean>,
  opts: { wait?: (ms: number) => Promise<void>; onGiveUp?: (e: unknown) => void } = {},
): Promise<boolean> {
  const wait = opts.wait ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  let lastError: unknown = null;
  for (let attempt = 0; attempt <= LOGIN_ACK_MAX_RETRIES; attempt += 1) {
    if (attempt > 0) await wait(loginAckDelay(attempt));
    try {
      if (await run(attempt)) return true;
      lastError = null;
    } catch (e) {
      lastError = e;
    }
  }
  opts.onGiveUp?.(lastError);
  return false;
}