/**
 * 登录接线的组件级行为（jsdom）：订阅 `login-state` → 徽标、退订、兜底查询、
 * 「保存登录 / 退出登录」命令化、`resolve-scan-flag` 显隐。
 *
 * 为什么必须挂真 `window.__TAURI_INTERNALS__` 而不是 `vi.stubGlobal('window', …)`：
 * 后者会把 jsdom 的 `Event` / `MouseEvent` 构造函数一起换掉，@vue/test-utils 的
 * `trigger()` 会在 `new Event(...)` 处炸成
 * `TypeError: (intermediate value)(intermediate value) is not a constructor`。
 * 所以这里只往真 window 上挂两个属性，`afterEach` 再删掉。
 */
// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';

import PlatformCard from '../src/components/PlatformCard.vue';
import DashboardPage from '../src/pages/DashboardPage.vue';

type Invoke = (cmd: string, args?: unknown) => Promise<unknown>;

interface Fake {
  calls: { cmd: string; args: Record<string, unknown> | undefined }[];
  /** 触发后端 emit：把载荷交给登录过的 login-state 回调 */
  emit(payload: unknown): void;
  setStatus(next: Record<string, unknown>): void;
  listenCount(): number;
  unlistenCount(): number;
}

function baseStatus(initial: Record<string, unknown>): Record<string, unknown> {
  return {
    platform: 'boss',
    state: 'no_login',
    running: false,
    message: '',
    saved: false,
    ...initial,
  };
}

/** 默认 IPC 回包（DashboardPage 挂载期会打一整套命令）。 */
function defaultReply(cmd: string): unknown {
  switch (cmd) {
    case 'job_tab_counts':
      return { pending: 0, delivered: 0, rejected: 0 };
    case 'list_jobs':
      return [];
    case 'list_logs':
      return { items: [], now: 0 };
    case 'scrape_progress':
      return {
        scraped: 0,
        inserted: 0,
        duplicates: 0,
        below_threshold: 0,
        rejected: 0,
        login_state: 'unknown',
        last_log: null,
        last_title: null,
        running: false,
        finished_reason: null,
      };
    case 'get_scoring_spec':
      return { score_max: 100, score_min_default: 70 };
    case 'get_app_config':
      return { score_max: 100, min_score: 70, max_jd: 50, scrape_defaults: null };
    case 'login_save':
      return { ok: true, saved: true, error: '' };
    case 'logout':
      return { ok: true, cleared: true, closed: false };
    default:
      return null;
  }
}

/**
 * 装一份假 IPC。`plugin:event|listen` 走与 `src/api/events.ts` 相同的两段式协议：
 * 先 `transformCallback(fn)` 拿 handler id，再用它登记事件。
 */
function installFake(initial: Record<string, unknown>): Fake {
  const calls: Fake['calls'] = [];
  let status = baseStatus(initial);
  const handlers = new Map<number, (event: unknown) => void>();
  /** 事件 id → 回调。真实的 `plugin:event|listen` 就是这么把 handlerId 绑到 eventId 上的。 */
  const listeners = new Map<number, (event: unknown) => void>();
  let nextHandler = 1;
  let nextEventId = 500;

  const invoke: Invoke = (cmd, args) => {
    calls.push({ cmd, args: args as Record<string, unknown> | undefined });
    if (cmd === 'login_status') return Promise.resolve({ ...status });
    if (cmd === 'plugin:event|listen') {
      const payload = args as { handler?: number };
      const fn = payload.handler !== undefined ? handlers.get(payload.handler) : undefined;
      const eventId = nextEventId++;
      if (fn) listeners.set(eventId, fn);
      return Promise.resolve(eventId);
    }
    if (cmd === 'plugin:event|unlisten') {
      listeners.delete((args as { eventId?: number }).eventId ?? -1);
      return Promise.resolve(undefined);
    }
    return Promise.resolve(defaultReply(cmd));
  };

  const transformCallback = (fn: (...a: unknown[]) => void): number => {
    const id = nextHandler++;
    handlers.set(id, fn as (event: unknown) => void);
    return id;
  };

  (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
  };

  return {
    calls,
  emit(payload: unknown) {
      for (const fn of listeners.values()) fn({ event: 'login-state', id: 1, payload });
    },
    setStatus(next) {
      status = { ...status, ...next };
    },
    listenCount: () => calls.filter((c) => c.cmd === 'plugin:event|listen').length,
    unlistenCount: () => calls.filter((c) => c.cmd === 'plugin:event|unlisten').length,
  };
}

let fake: Fake;

beforeEach(() => {
  fake = installFake({});
  vi.spyOn(window, 'confirm').mockReturnValue(true);
});

afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.restoreAllMocks();
});

const dot = (w: { find: (s: string) => { attributes: (n: string) => string | undefined } }): string =>
  w.find('[data-testid="platform-boss-status"]').attributes('aria-label') ?? '';

describe('PlatformCard: 保存 / 退出 / 扫码', () => {
  it('已登录未保存：出现 platform-login-save-btn，点击 emit save', async () => {
    const w = mount(PlatformCard, {
      props: {
        selected: 'boss',
        login: { platform: 'boss', state: 'logged_in', running: true, message: '', saved: false },
      },
    });
    const btn = w.find('[data-testid="platform-login-save-btn"]');
    expect(btn.exists()).toBe(true);
    await btn.trigger('click');
    expect(w.emitted('save')).toHaveLength(1);
    expect(w.find('[data-testid="platform-logout-btn"]').exists()).toBe(true);
  });

  it('已保存过：保存按钮收起，退出按钮仍在', () => {
    const w = mount(PlatformCard, {
      props: {
        selected: 'boss',
        login: { platform: 'boss', state: 'logged_in', running: false, message: '', saved: true },
      },
    });
    expect(w.find('[data-testid="platform-login-save-btn"]').exists()).toBe(false);
    expect(w.find('[data-testid="platform-logout-btn"]').exists()).toBe(true);
  });

  it('扫码中：resolve-scan-flag 出现且用到 data URI；非 waiting 态不出现', async () => {
    const w = mount(PlatformCard, {
      props: {
        selected: 'boss',
        login: {
          platform: 'boss',
          state: 'waiting',
          running: true,
          message: 'data:image/png;base64,AAA',
          saved: false,
        },
      },
    });
    const flag = w.find('[data-testid="resolve-scan-flag"]');
    expect(flag.exists()).toBe(true);
    expect(flag.attributes('src')).toBe('data:image/png;base64,AAA');
    await w.setProps({
      login: { platform: 'boss', state: 'logged_in', running: true, message: '', saved: false },
    });
    expect(w.find('[data-testid="resolve-scan-flag"]').exists()).toBe(false);
  });
});

describe('DashboardPage: login-state 订阅', () => {
  /**
   * 注意：`installFake` 返回的 `emit()` 会广播给所有登记过的回调，而
   * `transformCallback` 是全局单调递增的 id —— 所以每个用例只拿到自己那一次
   * 挂载产生的监听。这条 helper 把「等监听真正登记完」写成一个明确的前置断言，
   * 免得事件早于 `plugin:event|listen` 回到页面就丢了（表现为徽标纹丝不动）。
   */
  async function mountDashboard(): Promise<ReturnType<typeof mount>> {
    const w = mount(DashboardPage, { props: { activeNav: 'dashboard' } });
    await flushPromises();
    expect(fake.listenCount()).toBe(1);
    return w;
  }

  it('挂载时订阅 login-state 并主动查一次 login_status（兜底）', async () => {
    fake.setStatus({ state: 'logged_in', running: false, saved: true });
    const w = await mountDashboard();
    expect(fake.calls.some((c) => c.cmd === 'login_status')).toBe(true);
    expect(dot(w)).toBe('已登录');
    w.unmount();
  });

  it('事件到达时更新徽标：从「未登录」推进到「已登录」', async () => {
    const w = await mountDashboard();
    expect(dot(w)).toBe('未登录');
    // 顺序有讲究：`loginDot` 的口径是 `running === true` 一律算「未知」（扫码窗口
    // 还开着），只有 `running: false` 才谈得上「已登录」。事件到达那一拍会顺带补
    // 一次权威查询（拿 running / saved / message），所以假回包必须在 emit 之前就
    // 改好，否则补查询会拿旧值把徽标按回去。
    fake.setStatus({ state: 'logged_in', running: false, saved: false, message: '' });
    fake.emit({ platform: 'boss', state: 'logged_in', logged_in: true });
    await flushPromises();
    await flushPromises();
    expect(dot(w)).toBe('已登录');
    w.unmount();
  });

  it('未知/空 state 的事件不覆盖已知态（mergeLoginState 的关键规则）', async () => {
    fake.setStatus({ state: 'logged_in', running: false, saved: true });
    const w = await mountDashboard();
    fake.emit({ platform: 'boss', state: 'unknown', logged_in: false });
    await flushPromises();
    expect(dot(w)).toBe('已登录');
    w.unmount();
  });

  it('卸载时退订（plugin:event|unlisten 恰好一次）', async () => {
    const w = await mountDashboard();
    w.unmount();
    await flushPromises();
    expect(fake.unlistenCount()).toBe(1);
  });

  it('「保存登录」发 login_save（参数 { platform }）', async () => {
    fake.setStatus({ state: 'logged_in', running: false, saved: false });
    const w = await mountDashboard();
    const btn = w.find('[data-testid="platform-login-save-btn"]');
    expect(btn.exists()).toBe(true);
    await btn.trigger('click');
    await flushPromises();
    const save = fake.calls.find((c) => c.cmd === 'login_save');
    expect(save?.args).toEqual({ platform: 'boss' });
    w.unmount();
  });

  it('「退出登录」发 logout（参数 { platform }）', async () => {
    fake.setStatus({ state: 'logged_in', running: true, saved: true });
    const w = await mountDashboard();
    await w.find('[data-testid="platform-logout-btn"]').trigger('click');
    await flushPromises();
    const out = fake.calls.find((c) => c.cmd === 'logout');
    expect(out?.args).toEqual({ platform: 'boss' });
    w.unmount();
  });

  it('兜底查询失败不解约页面（状态点退回未知）', async () => {
    const bad: Invoke = (cmd, args) => {
      fake.calls.push({ cmd, args: args as Record<string, unknown> | undefined });
      if (cmd === 'login_status') return Promise.reject(new Error('sidecar 没起来'));
      return Promise.resolve(defaultReply(cmd));
    };
    (
      window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }
    ).__TAURI_INTERNALS__.invoke = bad;
    const w = await mountDashboard();
    expect(dot(w)).toBe('登录状态未知');
    w.unmount();
  });
});
