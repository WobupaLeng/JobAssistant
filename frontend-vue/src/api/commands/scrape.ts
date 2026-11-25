/**
 * `commands_scrape.rs` 的 10 条命令 + 登录态 4 条。载荷形状**逐条按签名走**（AGENTS.md C2）：
 * 只有 `scrape_start(args: ScrapeArgs)` 包一层 `{ args }`，其余全部平铺。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import type {
  LoginSaveReply,
  LoginStatus,
  LogoutReply,
  ScrapeProgress,
  ScrapeReply,
  ScrapeUpdate,
} from '../types';

/**
 * 启动抓取。`scrape_start` 的 `args` 是 struct 参数，**必须**包一层 `{ args }`。
 * 门槛只从 `get_scoring_spec` 取（C10），调用方不得硬编码。
 */
export function scrapeStart(params: {
  platform: string;
  keywords: readonly string[];
  maxJd: number;
  minScore: number;
}): Promise<ScrapeReply> {
  return invokeArgs<ScrapeReply>('scrape_start', {
    platform: params.platform,
    keywords: [...params.keywords],
    maxJd: params.maxJd,
    minScore: params.minScore,
  });
}

/** 停止抓取（保留会话）。 */
export function scrapeStop(): Promise<void> {
  return invokeNone<void>('scrape_stop');
}

/**
 * 关窗收尾：停抓取 + 关掉 sidecar 浏览器会话。只 stop 不关 stdin 会让 Python 主循环残留，
 * 所以应用启动时无条件调一次。
 */
export function scrapeClose(): Promise<void> {
  return invokeNone<void>('scrape_close');
}

/**
 * 抓取进度快照（前端 1 秒轮询）。**不抛错**：sidecar 没起来时也给一份空进度 + 一句
 * last_log，由界面自己判断。
 */
export function scrapeProgress(): Promise<ScrapeProgress> {
  return invokeNone<ScrapeProgress>('scrape_progress');
}

/**
 * 取走一批抓取事件（增量，取走即清）。平铺 `{ limit }`。`job` 帧的 `score` 可能是
 * `null`（未打分 / 被否决），不是 0。
 */
export function scrapeDrain(limit: number): Promise<ScrapeUpdate[]> {
  return invokeFlat<ScrapeUpdate[]>('scrape_drain', { limit });
}

/** 打开登录页（拉一个持久化 profile 的浏览器）。返回 sidecar 进程 id。 */
export function loginStart(platform: string): Promise<number> {
  return invokeFlat<number>('login_start', { platform });
}

/** 查登录态。`state` 取 `logged_in` / `no_login` / `unknown`。 */
export function loginStatus(platform: string): Promise<LoginStatus> {
  return invokeFlat<LoginStatus>('login_status', { platform });
}

/**
 * 把「已登录」落库（只写一个可见标记，cookie 由持久化 profile 管）。未登录时回
 * `ok:false` + `error`，**不抛异常**，调用方要判 `ok`。
 */
export function loginSave(platform: string): Promise<LoginSaveReply> {
  return invokeFlat<LoginSaveReply>('login_save', { platform });
}

/** 退出登录：清标记并关掉 sidecar 浏览器会话。 */
export function logout(platform: string): Promise<LogoutReply> {
  return invokeFlat<LogoutReply>('logout', { platform });
}

/**
 * 确认消费了一次 `login-state` 广播。后端只在状态变化时发事件，之后每 5 秒补发、最多 3 次，
 * 全员回话即停。`platform` + `state` 必须是事件里的原值，对不上的迟到确认被忽略。返回还缺
 * 几个成员回话（`0` = 已结束）；**不抛错**。
 */
export function loginAck(platform: string, state: string): Promise<number> {
  return invokeFlat<number>('login_ack', { platform, state }).catch(() => 0);
}

/**
 * 在已存在的 sidecar 会话里打开某个页面（如切到 `chat`）。平铺，`page` **有值才发**——
 * 空串会被后端当成「要打开的空页面」。
 */
export function sessionOpen(platform: string, page?: string | null): Promise<number> {
  return invokeFlat<number>('session_open', { platform, page: page ?? undefined });
}
