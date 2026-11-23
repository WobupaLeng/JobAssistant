// @vitest-environment jsdom
/**
 * `src/api/events.ts` 的契约测试：**不引 @tauri-apps/api**，只走
 * `window.__TAURI_INTERNALS__` 的两个内部函数。
 *
 * 三条边界：
 * 1. 非 Tauri 环境（vitest 默认就是）→ no-op 退订，**不抛错**；
 * 2. 真 Tauri 形状 → `plugin:event|listen` 带 `{ event, target: {kind:'Any'}, handler }`，
 *    `unlisten` 带 `{ event, eventId }`，退订幂等；
 * 3. 载荷形状 → `{ event, id, payload }` 与裸载荷都认，半截载荷丢弃。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  LOGIN_STATE_EVENT,
  hasTauriEvents,
  listenLoginState,
  normalizeLoginState,
  type LoginStatePayload,
} from '../src/api/events';

type Invoke = (cmd: string, args?: unknown) => Promise<unknown>;

/** 装一份假的 Tauri 2 事件内部函数，返回可检查的调用记录。 */
function installFakeTauri(payloads: unknown[] = []) {
  const calls: { cmd: string; args: unknown }[] = [];
  let nextEventId = 100;
  let lastHandler: ((event: unknown) => void) | null = null;

  const invoke: Invoke = (cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === 'plugin:event|listen') return Promise.resolve(nextEventId++);
    if (cmd === 'plugin:event|unlisten') return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  };

  const transformCallback = (fn: (...a: unknown[]) => void): number => {
    lastHandler = fn as (event: unknown) => void;
    return 7;
  };

  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
  };

  return {
    calls,
    /** 模拟后端一次 emit（Tauri 2 的回调参数形状）。 */
    emit(body: unknown) {
      payloads.push(body);
      lastHandler?.({ event: LOGIN_STATE_EVENT, id: Date.now(), payload: body });
    },
    emitRaw(raw: unknown) {
      lastHandler?.(raw);
    },
  };
}

afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.restoreAllMocks();
});

describe('events: 非 Tauri 环境降级', () => {
  it('没有 __TAURI_INTERNALS__ 时返回 no-op 退订且不抛错', async () => {
    expect(hasTauriEvents()).toBe(false);
    const handler = vi.fn();
    const off = await listenLoginState(handler);
    expect(typeof off).toBe('function');
    expect(() => off()).not.toThrow();
    expect(handler).not.toHaveBeenCalled();
  });

  it('只有 invoke 没有 transformCallback 也当降级（不算事件 API 可用）', async () => {
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: () => Promise.resolve(undefined),
    };
    expect(hasTauriEvents()).toBe(false);
    const off = await listenLoginState(() => undefined);
    await expect(Promise.resolve(off())).resolves.toBeUndefined();
  });
});

describe('events: 真 Tauri 形状', () => {
  it('listen 载荷是 { event, target:{kind:"Any"}, handler }，退订用 eventId', async () => {
    const fake = installFakeTauri();
    expect(hasTauriEvents()).toBe(true);
    const off = await listenLoginState(() => undefined);

    const listen = fake.calls.find((c) => c.cmd === 'plugin:event|listen');
    expect(listen).toBeDefined();
    expect(listen?.args).toEqual({
      event: 'login-state',
      target: { kind: 'Any' },
      handler: 7,
    });

    off();
    const unlisten = fake.calls.find((c) => c.cmd === 'plugin:event|unlisten');
    expect(unlisten?.args).toEqual({ event: 'login-state', eventId: 100 });
  });

  it('退订幂等：第二次调用不再发 unlisten', async () => {
    const fake = installFakeTauri();
    const off = await listenLoginState(() => undefined);
    off();
    off();
    expect(fake.calls.filter((c) => c.cmd === 'plugin:event|unlisten')).toHaveLength(1);
  });

  it('收到事件回调时把 payload 交给上层', async () => {
    const fake = installFakeTauri();
    const seen: LoginStatePayload[] = [];
    await listenLoginState((p) => seen.push(p));
    fake.emit({ platform: 'boss', state: 'logged_in', logged_in: true });
    expect(seen).toEqual([{ platform: 'boss', state: 'logged_in', logged_in: true }]);
  });

  it('裸载荷（不是 {payload} 信封）也认', async () => {
    const fake = installFakeTauri();
    const seen: LoginStatePayload[] = [];
    await listenLoginState((p) => seen.push(p));
    fake.emitRaw({ platform: 'boss', state: 'closed', logged_in: false });
    expect(seen).toHaveLength(1);
    expect(seen[0].state).toBe('closed');
  });

  it('回调抛错不会炸掉监听：转给 onError', async () => {
    const fake = installFakeTauri();
    const onError = vi.fn();
    await listenLoginState(() => {
      throw new Error('boom');
    }, onError);
    fake.emit({ platform: 'boss', state: 'error', logged_in: false });
    expect(onError).toHaveBeenCalledTimes(1);
  });
});

describe('events: normalizeLoginState', () => {
  it('缺字段的载荷一律丢弃（宁可不更新，也不拿半个状态覆盖）', () => {
    expect(normalizeLoginState(null)).toBeNull();
    expect(normalizeLoginState('x')).toBeNull();
    expect(normalizeLoginState({})).toBeNull();
    expect(normalizeLoginState({ platform: 'boss' })).toBeNull();
    expect(normalizeLoginState({ state: 'logged_in' })).toEqual({
      platform: '',
      state: 'logged_in',
      logged_in: true,
    });
  });

  it('logged_in 缺省时按 state 现算（与后端 lib.rs:526 同口径）', () => {
    expect(normalizeLoginState({ platform: 'boss', state: 'logged_in' })?.logged_in).toBe(true);
    expect(normalizeLoginState({ platform: 'boss', state: 'closed' })?.logged_in).toBe(false);
  });

  it('显式 logged_in 以载荷为准', () => {
    expect(
      normalizeLoginState({ platform: 'boss', state: 'logged_in', logged_in: false })?.logged_in,
    ).toBe(false);
  });
});
