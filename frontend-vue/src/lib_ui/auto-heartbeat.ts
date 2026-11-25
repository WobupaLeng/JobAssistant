/**
 * 自动投递的**登录态心跳**，也是自动投递状态唯一的周期来源。状态请求只发生在四个时刻：
 * 60~90 秒随机到期、每次投递完成、发完消息、用户主动刷新；连续失败 `maxFails` 次即停表。间隔
 * 取随机值是因为 sidecar 的浏览器命令是**单车道**的。投递节奏由回执的 `next_at_ms` 精确决胜。
 */
import { autoGreetNext, autoGreetStatus } from '../api/commands/auto';
import { loginStatus } from '../api/commands/scrape';
import type { AutoStatusReply } from '../api/types';
import { replyNote, shouldSend } from './auto-model';
import { isLoggedIn } from './login-model';

/** 心跳间隔下限（毫秒）：60 秒。 */
export const HEARTBEAT_MIN_MS = 60_000;
/** 心跳间隔上限（毫秒）：90 秒。 */
export const HEARTBEAT_MAX_MS = 90_000;
/** 连续失败多少次就停表。 */
export const HEARTBEAT_MAX_FAILS = 3;
/**
 * 本地准点发送的窗口上限（毫秒）。离 `next_at_ms` 超过这么久就不 arm，交给下一次心跳。
 * 这个上限是**防浪费岗位**的：本地定时器挂着时用户手动暂停，触发后会白耗一次岗位机会。
 */
export const SEND_ARM_MAX_MS = 60_000;

export const LOGIN_EXPIRED_NOTE = '登录已失效：请重新扫码登录后再自动投递';
export const NOT_LOGGED_IN_NOTE = '浏览器未连接：请先登录';
export const BOSS_UNREACHABLE_NOTE = '浏览器未连接：请先登录';
/**
 * 库里存着登录标记、但浏览器（sidecar）没起来。与 `BOSS_UNREACHABLE_NOTE` 的区别只在
 * 用户该做什么：此时说「请先登录」会误导（头像旁就写着「已登录」），该做的是拉起浏览器。
 */
export const BROWSER_DETACHED_NOTE = '浏览器未启动：点一下让浏览器重新连接';

export interface HeartbeatHooks {
  /** 拿到新的后端快照（含 idle 态，计数与倒计时都靠它）。 */
  onStatus(st: AutoStatusReply): void;
  /** 一条正在路上：置 true / false。 */
  onSending(sending: boolean): void;
  /** 界面提示（登录失效、失败重试、已发出一条）。 */
  onNote(text: string): void;
}

export interface HeartbeatTimer {
  /**
   * 按登录态开表，或已开表时「立刻补一拍」。两种用法都是**主动刷新**：
   * 开表、投递完成、发完消息、用户手动刷新都调它。
   */
  start(): Promise<void>;
  /** 停表（幂等）。 */
  stop(): void;
  /** 是否在跑（调试 / 单测用）。 */
  running(): boolean;
  /** 最近一次读到的后端快照（没读到过为 null）。 */
  lastStatus(): AutoStatusReply | null;
}

/** 60~90 秒之间的随机间隔（含两端）。 */
export function heartbeatDelay(random: () => number = Math.random): number {
  const span = HEARTBEAT_MAX_MS - HEARTBEAT_MIN_MS + 1;
  return HEARTBEAT_MIN_MS + Math.floor(random() * span);
}

function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function startHeartbeat(hooks: HeartbeatHooks, maxFails = HEARTBEAT_MAX_FAILS): HeartbeatTimer {
  let alive = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let fails = 0;
  let busy = false;
  let last: AutoStatusReply | null = null;
  /** 本地准点发送的定时器（到 `next_at_ms` 那一毫秒直接发，不轮询）。 */
  let sendTimer: ReturnType<typeof setTimeout> | null = null;
  /** 已经为哪个 `next_at_ms` arm 过，避免每拍重复挂表。 */
  let armedFor = 0;

  function clear(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function clearSend(): void {
    if (sendTimer !== null) {
      clearTimeout(sendTimer);
      sendTimer = null;
    }
    armedFor = 0;
  }

  function schedule(delay: number): void {
    if (!alive) return;
    clear();
    timer = setTimeout(() => {
      timer = null;
      void beat();
    }, delay);
  }

  /**
   * 拿到快照后本地排一次准点发送：到期那一刻**直接**调 `auto_greet_next`，不发状态请求。
   * 窗口外（还没到期或超过 `SEND_ARM_MAX_MS`）不 arm，交给下一拍。
   */
  function armSend(st: AutoStatusReply): void {
    const next = Number(st.next_at_ms ?? 0);
    if (next === 0 || st.paused || !st.running) {
      clearSend();
      return;
    }
    if (next === armedFor) return;
    const wait = next - Date.now();
    if (wait > SEND_ARM_MAX_MS) return;
    clearSend();
    armedFor = next;
    sendTimer = setTimeout(() => {
      sendTimer = null;
      void fireSend();
    }, Math.max(0, wait));
  }

  /** 准点那一刻：先确认后端仍在跑（可能刚被暂停），再发一条。 */
  async function fireSend(): Promise<void> {
    if (!alive || busy) return;
    busy = true;
    try {
      if (last === null || !last.running || last.paused) return;
      await sendOne();
    } finally {
      busy = false;
    }
    schedule(heartbeatDelay());
  }

  /** 读一次登录态，带上 `saved`（库里是否有登录标记），用来区分「没连上」与「没登录」。 */
  async function loginState(): Promise<{ state: string; saved: boolean }> {
    // sidecar 没起来时 `login_status` 自己会抛：等价于「浏览器未连接」。
    const st = await loginStatus('boss').catch(() => ({ state: 'unknown', saved: false }));
    return { state: String(st.state ?? 'unknown'), saved: st.saved === true };
  }

  /** 已登录 → 开表；未登录 / 浏览器未连接 → 停表并给出引导。 */
  /**
   * 开表 / 立刻补一拍（幂等）。已登录时永远是「立刻读一次，然后重新随机间隔」，
   * 所以调用它就是一次**主动刷新**：每次投递完成、发完消息、手动刷新走这里。
   */
  async function start(): Promise<void> {
    // 已在跑时别自己再验一次登录：`beat()` 一拍里已经验过了，
    // 这里再来一次就是同一毫秒里的第二次 `login_status`。
    if (alive) {
      schedule(0);
      return;
    }
    const { state, saved } = await loginState();
    if (!isLoggedIn(state)) {
      // 「浏览器没起来」和「连上了但没登录」是两件事：本地有标记时说「请先登录」
      // 会让人去点已经写着「已登录」的按钮，所以分开给话。
      if (state === 'unknown') {
        hooks.onNote(saved ? BROWSER_DETACHED_NOTE : BOSS_UNREACHABLE_NOTE);
      } else {
        hooks.onNote(NOT_LOGGED_IN_NOTE);
      }
      stop();
      return;
    }
    fails = 0;
    alive = true;
    schedule(0);
  }

  async function beat(): Promise<void> {
    if (!alive || busy) return;
    busy = true;
    try {
      // 每一拍都验登录：掉线即停表，重新登录后由用户操作再开。
      const probe = await loginState();
      if (!isLoggedIn(probe.state)) {
        if (alive) {
          stop();
          // 浏览器整个没连上 ≠ 登录过期：后者才需要「重新扫码」。
          hooks.onNote(probe.state === 'unknown' ? BOSS_UNREACHABLE_NOTE : LOGIN_EXPIRED_NOTE);
        }
        return;
      }
      const st = await autoGreetStatus();
      if (!alive) return;
      fails = 0;
      last = st;
      hooks.onStatus(st);
      // 后端在跑且倒计时到期才发一条。**这里只发一条**：不排后续的一秒节拍，
      // 下一次状态请求完全由「随机间隔」或「用户操作」驱动。
      if (shouldSend(Date.now(), st, false)) {
        clearSend();
        await sendOne();
      } else {
        // 还没到期：按后端给的 `next_at_ms` 在本地准点发，投递节奏不受心跳限制。
        armSend(st);
      }
    } catch (e) {
      if (!alive) return;
      fails += 1;
      if (fails >= maxFails) {
        stop();
        hooks.onNote(`读取自动投递状态失败（已重试 ${fails} 次）：${errText(e)}`);
        return;
      }
      hooks.onNote(`读取自动投递状态失败（第 ${fails} 次，稍后重试）：${errText(e)}`);
    } finally {
      busy = false;
    }
    // 每次请求（成功或失败）结束都重新随机一次间隔。
    schedule(heartbeatDelay());
  }

  /**
   * 发一条。发完不论成功失败都**主动请求一次状态**——这就是「每次投递完成 /
   * 发完消息请求一次」；本函数不会再触发第二条投递。
   */
  async function sendOne(): Promise<void> {
    hooks.onSending(true);
    try {
      const reply = await autoGreetNext(null);
      if (!alive) return;
      if (!reply.ok) {
        const msg = String(reply.error ?? '').trim() !== '' ? String(reply.error) : replyNote(reply);
        if (msg.trim() !== '') hooks.onNote(msg.trim());
      } else {
        hooks.onNote('自动投递已发出一条');
      }
    } catch (e) {
      if (alive) hooks.onNote(`发送招呼失败：${errText(e)}`);
    } finally {
      if (alive) {
        // 投递完成后的那次主动请求（失败也不抛出：状态读不到只影响显示）。
        await autoGreetStatus()
          .then((st) => {
            if (alive) {
              fails = 0;
              last = st;
              hooks.onStatus(st);
              // 发完一条后后端会给出新的 `next_at_ms`：就地续上准点节奏，
              // 不用等下一次心跳。
              armSend(st);
            }
          })
          .catch((e: unknown) => {
            if (alive) hooks.onNote(`刷新自动投递状态失败：${errText(e)}`);
          });
      }
      hooks.onSending(false);
    }
  }

  function stop(): void {
    alive = false;
    fails = 0;
    clear();
    clearSend();
  }

  return {
    start,
    stop,
    running(): boolean {
      return alive;
    },
    lastStatus(): AutoStatusReply | null {
      return last;
    },
  };
}
