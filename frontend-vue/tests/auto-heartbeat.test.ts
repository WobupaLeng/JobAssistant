// @vitest-environment jsdom
/**
 * 登录态心跳（`lib_ui/auto-heartbeat.ts`）的契约。
 *
 * 三条硬约束：
 * 1. 心跳间隔 60~90 秒随机（用户裁决），不是每秒；
 * 2. 开表 / 停表**只**取决于登录态，与「自动投递是否在跑」无关；
 * 3. 连续失败最多重试 3 次即停表，不做无限重试。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  HEARTBEAT_MAX_FAILS,
  HEARTBEAT_MAX_MS,
  HEARTBEAT_MIN_MS,
  LOGIN_EXPIRED_NOTE,
  NOT_LOGGED_IN_NOTE,
  heartbeatDelay,
  startHeartbeat,
} from '../src/lib_ui/auto-heartbeat';

const status = {
  running: false,
  paused: false,
  sent: 0,
  pending: 0,
  next_at_ms: 0,
  stop_reason: '',
};

let loginState = 'logged_in';
let statusCalls = 0;
let nextCalls = 0;
let statusThrows = false;

function hooks() {
  return {
    onStatus: vi.fn(),
    onSending: vi.fn(),
    onNote: vi.fn(),
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  loginState = 'logged_in';
  statusCalls = 0;
  nextCalls = 0;
  statusThrows = false;
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string) => {
      if (cmd === 'login_status')
        return Promise.resolve({ platform: 'boss', state: loginState, saved: true, error: '' });
      if (cmd === 'auto_greet_status') {
        statusCalls += 1;
        if (statusThrows) return Promise.reject('sidecar 未启动');
        return Promise.resolve(status);
      }
      if (cmd === 'auto_greet_next') {
        nextCalls += 1;
        return Promise.resolve({ ok: true, sent: true, text: '', error: '' });
      }
      return Promise.resolve(true);
    },
  };
});

afterEach(() => {
  vi.useRealTimers();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  // 用例之间必须复原：这些对象是模块级共享状态，残留的 running/next_at_ms
  // 会让后面的用例凭空触发一次准点发送。
  status.running = false;
  status.paused = false;
  status.sent = 0;
  status.pending = 0;
  status.next_at_ms = 0;
  status.stop_reason = '';
});

describe('心跳间隔', () => {
  it('落在 60~90 秒之间（含两端）', () => {
    expect(HEARTBEAT_MIN_MS).toBe(60_000);
    expect(HEARTBEAT_MAX_MS).toBe(90_000);
    expect(heartbeatDelay(() => 0)).toBe(60_000);
    expect(heartbeatDelay(() => 0.999999)).toBe(90_000);
    for (let i = 0; i < 50; i += 1) {
      const d = heartbeatDelay();
      expect(d).toBeGreaterThanOrEqual(HEARTBEAT_MIN_MS);
      expect(d).toBeLessThanOrEqual(HEARTBEAT_MAX_MS);
    }
  });

  it('两拍之间的等待就是 60~90 秒，不是 1 秒', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    const first = statusCalls;
    expect(first).toBe(1);
    // 59 秒内不会再读到状态。
    await vi.advanceTimersByTimeAsync(59_000);
    expect(statusCalls).toBe(first);
    await vi.advanceTimersByTimeAsync(31_000);
    expect(statusCalls).toBe(2);
    t.stop();
  });
});

describe('只受登录限制', () => {
  it('已登录就开表，与后端 running=false 无关', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(t.running()).toBe(true);
    expect(h.onStatus).toHaveBeenCalledTimes(1);
    t.stop();
  });

  it('未登录 → 不开表，并给登录引导', async () => {
    loginState = 'no_login';
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    expect(t.running()).toBe(false);
    expect(h.onNote).toHaveBeenLastCalledWith(NOT_LOGGED_IN_NOTE);
  });

  it('浏览器未连接（unknown）→ 不开表', async () => {
    loginState = 'unknown';
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    expect(t.running()).toBe(false);
  });

  it('心跳中掉线 → 停表并提示重新登录', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    loginState = 'no_login';
    await vi.advanceTimersByTimeAsync(HEARTBEAT_MAX_MS);
    expect(t.running()).toBe(false);
    expect(h.onNote).toHaveBeenLastCalledWith(LOGIN_EXPIRED_NOTE);
  });
});

describe('失败最多重试 3 次', () => {
  it('连续失败的次数到达上限即停表', async () => {
    expect(HEARTBEAT_MAX_FAILS).toBe(3);
    statusThrows = true;
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    // 第 1 拍失败。
    await vi.advanceTimersByTimeAsync(0);
    expect(t.running()).toBe(true);
    // 第 2 拍失败。
    await vi.advanceTimersByTimeAsync(HEARTBEAT_MAX_MS);
    expect(t.running()).toBe(true);
    // 第 3 拍失败 → 停表。
    await vi.advanceTimersByTimeAsync(HEARTBEAT_MAX_MS);
    expect(t.running()).toBe(false);
    const calls = statusCalls;
    await vi.advanceTimersByTimeAsync(HEARTBEAT_MAX_MS * 3);
    expect(statusCalls).toBe(calls);
  });
});

describe('没有任何 1 秒轮询', () => {
  it('投递进行一次后不会再排第二拍（下一次完全靠随机间隔）', async () => {
    status.running = true;
    status.next_at_ms = 0;
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    const afterStart = statusCalls;
    // 10 秒内没有任何额外请求：旧实现会在这里跑 10 拍。
    await vi.advanceTimersByTimeAsync(10_000);
    expect(statusCalls).toBe(afterStart);
    t.stop();
  });

  it('主动刷新 = 立刻请求一次，并重新随机间隔', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    const before = statusCalls;
    // 模拟「发完消息 / 投递完成后的主动刷新」。
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(statusCalls).toBe(before + 1);
    // 刷新后仍然按 60 秒起步，不会退化成每秒。
    await vi.advanceTimersByTimeAsync(59_000);
    expect(statusCalls).toBe(before + 1);
    t.stop();
  });
});

describe('投递节奏按后端 next_at_ms 准点，不等心跳', () => {
  it('快照里 next_at_ms 在窗口内 → 到点立刻发一条，期间不发状态请求', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    // 后端给出 30 秒后的下一条。
    status.running = true;
    status.next_at_ms = Date.now() + 30_000;
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    const statusBefore = statusCalls;
    const sentBefore = nextCalls;
    // 29 秒还没到期：不发。
    await vi.advanceTimersByTimeAsync(29_000);
    expect(nextCalls).toBe(sentBefore);
    // 到点即发，而且不发状态请求。
    await vi.advanceTimersByTimeAsync(1_000);
    expect(nextCalls).toBe(sentBefore + 1);
    expect(statusCalls).toBe(statusBefore + 1); // 只有「发完主动刷新」那一次
    t.stop();
  });

  it('窗口外（60 秒以上）不 arm，交给心跳', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    // 后端给出 10 分钟后的下一条：超出本地窗口，不挂定时器。
    status.running = true;
    status.next_at_ms = Date.now() + 600_000;
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(120_000);
    expect(nextCalls).toBe(0);
    t.stop();
  });

  it('暂停后到点不发（本地快照已不是 running）', async () => {
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    status.running = true;
    status.next_at_ms = Date.now() + 10_000;
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    // 用户在这期间暂停：下一次心跳读到的快照覆盖本地状态。
    status.paused = true;
    status.next_at_ms = 0;
    await t.start();
    await vi.advanceTimersByTimeAsync(20_000);
    expect(nextCalls).toBe(0);
    t.stop();
  });
});

describe('每次请求完成都重新随机间隔', () => {
  it('失败后也重新排一次（失败重试同样走 60~90 秒）', async () => {
    statusThrows = true;
    const h = hooks();
    const t = startHeartbeat(h);
    await t.start();
    await vi.advanceTimersByTimeAsync(0);
    const first = statusCalls;
    // 失败后的下一次间隔是重新随机的，落在 60~90 秒之间：这段窗口内必然会再试一次。
    await vi.advanceTimersByTimeAsync(59_000);
    expect(statusCalls).toBe(first);
    await vi.advanceTimersByTimeAsync(HEARTBEAT_MAX_MS - 59_000);
    expect(statusCalls).toBe(first + 1);
    expect(t.running()).toBe(true);
    t.stop();
  });
});