/**
 * 登录态广播的**确认 + 重试**：消费方回 ack，最大重试 `LOGIN_ACK_MAX_RETRIES` 次。
 * `login-state` 只在状态变化时播一次，订阅晚或在后台都会吞掉一次变化，所以「收到广播」不等于
 * 「办成了」：重试的是**消费方动作**（清提示、开心跳），不重发广播，也不做定时轮询。
 */

/** 最大重试次数（不含首次尝试）。 */
export const LOGIN_ACK_MAX_RETRIES = 3;

/** 第 n 次重试前的等待毫秒（退避：300 / 900 / 1800）。 */
export function loginAckDelay(retry: number): number {
  const n = Math.max(1, Math.min(LOGIN_ACK_MAX_RETRIES, retry));
  return 300 * n * n;
}

/**
 * 让一次广播处理带上确认语义。`run` 返回 `true` 才算生效，`false` 或抛错都会按
 * `loginAckDelay` 退避重试；`onGiveUp` 在重试用尽后调用一次，由调用方决定怎么回到可用
 * 状态，**不在这里偷偷起定时器**。
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