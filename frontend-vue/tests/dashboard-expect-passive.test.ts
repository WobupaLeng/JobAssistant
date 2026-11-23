// @vitest-environment jsdom
/**
 * 求职期望（list_expectations）必须是被动能力：
 * 1. 挂载时不调用；2. 未登录点击不调用；3. 已登录且用户点击才调用；4. 失败后不自动重试。
 */
import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const listExpectations = vi.fn();
const loginStatus = vi.fn();
const listLogs = vi.fn(async () => ({ items: [], next_seq: 0 }));
const scrapeProgress = vi.fn(async () => ({ running: false }));

vi.mock('../src/api/commands/chat', () => ({ listExpectations: () => listExpectations() }));
vi.mock('../src/api/commands/scrape', () => ({
  loginStatus: () => loginStatus(),
  loginStart: vi.fn(async () => undefined),
  loginSave: vi.fn(async () => ({ ok: true, saved: true, error: '' })),
  logout: vi.fn(async () => undefined),
  scrapeDrain: vi.fn(async () => ({})),
  scrapeProgress: () => scrapeProgress(),
  scrapeStart: vi.fn(async () => ({})),
  scrapeStop: vi.fn(async () => undefined),
}));
vi.mock('../src/api/events', () => ({
  listenLoginState: vi.fn(async () => () => undefined),
}));
vi.mock('../src/api/commands/db', () => ({
  allowJobs: vi.fn(async () => ({})),
  jobTabCounts: vi.fn(async () => ({})),
  listJobs: vi.fn(async () => []),
  markDelivered: vi.fn(async () => ({})),
  rejectJobs: vi.fn(async () => ({})),
}));
vi.mock('../src/api/commands/logs', () => ({ listLogs: () => listLogs() }));
vi.mock('../src/api/commands/shell', () => ({
  getScoringSpec: vi.fn(async () => ({ max_score: 100, default_threshold: 70 })),
}));
vi.mock('../src/api/commands/config', () => ({
  getAppConfig: vi.fn(async () => ({})),
  setLimit: vi.fn(async () => ({ min_score: 70 })),
  setScrapeDefaults: vi.fn(async () => ({})),
  setAppConfig: vi.fn(async () => ({})),
}));
vi.mock('../src/api/commands/sensitive', () => ({
  sensitiveList: vi.fn(async () => ({ items: [], total: 0 })),
  sensitiveRemove: vi.fn(async () => ({})),
}));

import DashboardPage from '../src/pages/DashboardPage.vue';

const LOGGED_IN = { platform: 'boss', state: 'logged_in', running: false, message: '', saved: true };
const NO_LOGIN = { platform: 'boss', state: 'no_login', running: false, message: '', saved: false };

/** 找到「读取期望」入口：按 data-testid 的 expect 相关控件。 */
async function openFilterAndFindReadExpect(wrapper: ReturnType<typeof mount>) {
  await wrapper.find('[data-testid="scrape-filter-toggle-btn"]').trigger('click');
  await flushPromises();
  const btn = wrapper.find('[data-testid="scrape-filter-expect-btn"]');
  expect(btn.exists()).toBe(true);
  return btn;
}

describe('求职期望被动化', () => {
  beforeEach(() => {
    listExpectations.mockReset();
    loginStatus.mockReset();
    listLogs.mockReset();
    listLogs.mockResolvedValue({ items: [], next_seq: 0 });
    scrapeProgress.mockReset();
    scrapeProgress.mockResolvedValue({ running: false });
  });

  it('挂载时不调用 listExpectations（即使已登录）', async () => {
    loginStatus.mockResolvedValue(LOGGED_IN);
    mount(DashboardPage);
    await flushPromises();
    expect(listExpectations).not.toHaveBeenCalled();
  });

  it('未登录时点击读取期望不调用 listExpectations', async () => {
    loginStatus.mockResolvedValue(NO_LOGIN);
    const wrapper = mount(DashboardPage);
    await flushPromises();
    const btn = await openFilterAndFindReadExpect(wrapper);
    await btn.trigger('click');
    await flushPromises();
    expect(listExpectations).not.toHaveBeenCalled();
  });

  it('已登录且用户点击才调用 listExpectations', async () => {
    loginStatus.mockResolvedValue(LOGGED_IN);
    listExpectations.mockResolvedValue([]);
    const wrapper = mount(DashboardPage);
    await flushPromises();
    expect(listExpectations).not.toHaveBeenCalled();
    const btn = await openFilterAndFindReadExpect(wrapper);
    await btn.trigger('click');
    await flushPromises();
    expect(listExpectations).toHaveBeenCalledTimes(1);
  });

  it('挂载时不轮询日志（listLogs 不被调用，即使已登录）', async () => {
    loginStatus.mockResolvedValue(LOGGED_IN);
    vi.useFakeTimers();
    mount(DashboardPage);
    await flushPromises();
    vi.advanceTimersByTime(60_000);
    await flushPromises();
    expect(listLogs).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('sidecar 失败（listLogs 抛错）后停止轮询，不循环重试，也不新增错误卡片', async () => {
    loginStatus.mockResolvedValue(LOGGED_IN);
    scrapeProgress.mockResolvedValue({ running: true });
    listLogs.mockRejectedValue(new Error('EPIPE: 管道已关闭'));
    vi.useFakeTimers();
    const wrapper = mount(DashboardPage);
    await flushPromises();
    // 进入「已登录 + 运行中」，轮询闸门打开；tick 会重新 startLogPolling。
    vi.advanceTimersByTime(20_000);
    await flushPromises();
    // 失败一次后闸门即关闭：之后再推进很久，调用次数都不能再增加。
    expect(listLogs).toHaveBeenCalled();
    const callsAfterFailure = listLogs.mock.calls.length;
    vi.advanceTimersByTime(120_000);
    await flushPromises();
    expect(listLogs.mock.calls.length).toBe(callsAfterFailure);
    expect(wrapper.find('[data-testid="app-error-bar"]').exists()).toBe(false);
    vi.useRealTimers();
  });

  it('同源错误只留一条：同 code 重复 fail 不堆叠', async () => {
    loginStatus.mockResolvedValue(LOGGED_IN);
    listExpectations.mockRejectedValue(new Error('EPIPE 1'));
    const wrapper = mount(DashboardPage);
    await flushPromises();
    const btn = await openFilterAndFindReadExpect(wrapper);
    await btn.trigger('click');
    await flushPromises();
    listExpectations.mockRejectedValue(new Error('EPIPE 2'));
    await btn.trigger('click');
    await flushPromises();
    const bar = wrapper.find('[data-testid="app-error-bar"]');
    expect(bar.exists()).toBe(true);
    expect(wrapper.find('[data-testid="app-error-bar-rest"]').exists()).toBe(false);
  });

  it('失败后不自动重试（只保留错误队列里的手动重试）', async () => {
    vi.useFakeTimers();
    loginStatus.mockResolvedValue(LOGGED_IN);
    listExpectations.mockRejectedValue(new Error('boom'));
    const wrapper = mount(DashboardPage);
    await flushPromises();
    const btn = await openFilterAndFindReadExpect(wrapper);
    await btn.trigger('click');
    await flushPromises();
    vi.advanceTimersByTime(60_000);
    await flushPromises();
    expect(listExpectations).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
