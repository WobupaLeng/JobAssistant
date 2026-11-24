/**
 * `commands_scrape.rs` 的 10 条命令 + 登录态 4 条（`login_*` / `logout` 同文件）。
 *
 * 两种载荷形状混在一起，**逐条按签名走**（AGENTS.md C2）：
 * - `scrape_start(args: ScrapeArgs)` → 包 `{ args }`（平铺发会被拒）；
 * - `scrape_drain(limit)` / `login_start(platform)` / `login_status(platform)` /
 *   `login_save(platform)` / `logout(platform)` / `session_open(platform, page)` →
 *   平铺，键名与参数名同名。
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
 * 启动抓取。
 *
 * 载荷结构：`{ args: { platform, keywords, maxJd, minScore } }`。
 * 命令层签名是 `scrape_start(app, state, store, args: ScrapeArgs)`，**必须**包一层
 * `args`；`ScrapeArgs` 的两个整数字段靠 `alias` 认驼峰（`maxJd` / `minScore`）。
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
 * 关窗收尾：停抓取 + 关掉 sidecar 浏览器会话。
 *
 * 只 stop 不关 stdin 会让 Python 主循环残留、进程退不干净，
 * 所以应用启动时就应该无条件调一次。
 */
export function scrapeClose(): Promise<void> {
  return invokeNone<void>('scrape_close');
}

/**
 * 抓取进度快照（前端 1 秒轮询）。
 *
 * 返回**不抛错**：sidecar 没起来时也给一份「空进度 + 一句 last_log」，
 * 由界面自己判断。
 */
export function scrapeProgress(): Promise<ScrapeProgress> {
  return invokeNone<ScrapeProgress>('scrape_progress');
}

/**
 * 取走一批抓取事件（增量，取走即清）。
 *
 * 平铺：`{ limit }`。返回的是判别联合 `ScrapeUpdate[]`，其中 `job` 帧的
 * `score` 可能是 `null`（未打分 / 被否决），不是 0。
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
 * 把「已登录」落库（只写一个可见标记，cookie 由持久化 profile 管）。
 *
 * 未登录时回 `ok:false` + 可读 `error`，**不抛异常**——调用方要判 `ok`。
 */
export function loginSave(platform: string): Promise<LoginSaveReply> {
  return invokeFlat<LoginSaveReply>('login_save', { platform });
}

/** 退出登录：清标记并关掉 sidecar 浏览器会话。 */
export function logout(platform: string): Promise<LogoutReply> {
  return invokeFlat<LogoutReply>('logout', { platform });
}

/**
 * 确认消费了一次 `login-state` 广播。
 *
 * 后端只在状态**变化**时发事件，然后每 5 秒补发一次、最多 3 次；收到确认且
 * 全员都回话后立刻停止补发（用户 m02396）。`platform` + `state` 必须是
 * 收到事件里的原值，对不上说明是旧轮次的迟到确认，后端会直接忽略。
 *
 * 返回还缺几个成员回话：`0` = 这一轮已结束。**不抛错**——这是旁路信号，
 * 确认丢了也只是多补发一两次，不该冒泡到界面。
 */
export function loginAck(platform: string, state: string): Promise<number> {
  return invokeFlat<number>('login_ack', { platform, state }).catch(() => 0);
}

/**
 * 在已存在的 sidecar 会话里打开某个页面（如切到 `chat`）。
 *
 * 平铺：`page` **有值才发**——发空串会被后端当成「要打开的空页面」。
 */
export function sessionOpen(platform: string, page?: string | null): Promise<number> {
  return invokeFlat<number>('session_open', { platform, page: page ?? undefined });
}
