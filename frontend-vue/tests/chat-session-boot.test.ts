// @vitest-environment jsdom
/**
 * 「还没打过招呼」的岗位 → 建会话不再挡住界面（用户 m00393）。
 *
 * 背景：`chat_open` 要真的在浏览器里打开岗位页、点「立即沟通」、再等页面自己跳聊天页，
 * 实测 5–15 秒（最坏要等 goto 45s + 按钮 15s + 点击 10s + 跳转 10s）。早先 `pick()` 直接
 * `await` 它，这一段时间整页卡在选中态上，连点别的会话都做不到。
 *
 * 现在：点下去只把占位行留在列表里并挂一句「正在打开 Boss 会话…」，随后在后台建；
 * 建好后回填这一行的真 uid，人还停在这一行上就顺势进入 + 拉消息。
 */
import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Friend } from '../src/api/types';
import { PENDING_STORAGE_KEY } from '../src/lib_ui/chat-model';
import ChatPage from '../src/pages/ChatPage.vue';

interface Call {
  cmd: string;
  payload: unknown;
}

const calls: Call[] = [];

function friend(uid: string, patch: Partial<Friend> = {}): Friend {
  return {
    uid,
    name: `HR-${uid}`,
    company: '星瀚科技',
    title: '后端工程师',
    last_msg: '你好',
    last_time: '10:00',
    unread: 0,
    avatar: '',
    ...patch,
  };
}

/** 占位行：名字就是公司名、没消息没时间、uid 是 `pending:<岗位 id>`。 */
function pendingRow(jobId: string): Friend {
  return {
    uid: `pending:${jobId}`,
    name: '星瀚科技',
    company: '星瀚科技',
    title: '后端工程师',
    last_msg: '',
    last_time: '',
    unread: 0,
    avatar: '',
  };
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (v: T) => void;
  reject: (e: unknown) => void;
}

/** `chat_open` 的可控桩：不 resolve 之前，建会话就一直「在飞」。 */
function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

let openResult: Deferred<{ uid: string }> | null = null;
let openCalls = 0;
const askedUids: string[] = [];

/** `script setup` 的 `defineExpose` 只放出来一部分；这里按测试用到的形状取值。 */
function activeUid(w: Awaited<ReturnType<typeof mountPage>>): string | null {
  return (w.vm as unknown as { ui: { activeFriendsUid: string | null } }).ui.activeFriendsUid;
}

function friendsReturning(remote: Friend[]) {
  return (cmd: string, payload?: unknown): Promise<unknown> => {
    calls.push({ cmd, payload });
    switch (cmd) {
      case 'chat_friends':
        return Promise.resolve({ items: remote });
      case 'chat_friends_local':
        return Promise.resolve({ items: [] });
      case 'chat_messages':
        askedUids.push(String((payload as { uid?: string }).uid));
        return Promise.resolve({ items: [] });
      case 'chat_messages_local':
        return Promise.resolve({ items: [] });
      case 'chat_open':
        openCalls += 1;
        return openResult === null ? Promise.resolve({ uid: 'u9' }) : openResult.promise;
      case 'chat_resumes':
        return Promise.resolve({ items: [] });
      case 'list_expectations':
        return Promise.resolve([]);
      case 'login_status':
        return Promise.resolve({ platform: 'boss', state: 'logged_in', saved: true, error: '' });
      case 'auto_greet_status':
        return Promise.resolve({
          running: false,
          paused: false,
          sent: 0,
          failures: 0,
          next_at_ms: 0,
          remaining_ms: 0,
          stop_reason: '',
          pending: 0,
        });
      default:
        return Promise.resolve(true);
    }
  };
}

async function mountPage(remote: Friend[]) {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
    invoke: friendsReturning(remote),
  };
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  const wrapper = mount(ChatPage, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

function seedPending(rows: Friend[]): void {
  localStorage.setItem(PENDING_STORAGE_KEY, JSON.stringify(rows));
}

beforeEach(() => {
  calls.length = 0;
  askedUids.length = 0;
  openCalls = 0;
  openResult = null;
  localStorage.clear();
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  localStorage.clear();
  vi.restoreAllMocks();
});

describe('建会话不再挡住界面（用户 m00393）', () => {
  it('会话还没建好时，已经能点开另一位 Boss', async () => {
    seedPending([pendingRow('job-7')]);
    openResult = deferred<{ uid: string }>();
    const w = await mountPage([friend('u2', { job_id: 'job-u2' })]);

    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();
    // 点下去就发出建会话的请求，界面挂一句提示，选中停在占位行。
    expect(openCalls).toBe(1);
    expect((w.vm as { note: string }).note).toContain('正在打开 Boss 会话');
    expect(activeUid(w)).toBe('pending:job-7');

    // 关键：`chat_open` 还悬着的时候就能切到别的会话，并且真的去拉它的话题。
    await w.find('[data-testid="friend-u2"]').trigger('click');
    await flushPromises();
    expect(activeUid(w)).toBe('u2');
    expect(askedUids).toContain('u2');

    // 建好之后只回填那一行，不把用户从 u2 抢走。
    openResult.resolve({ uid: 'c83b66ecd36a48070X1-3ti-FFBZ' });
    await flushPromises();
    expect(w.find('[data-testid="friend-c83b66ecd36a48070X1-3ti-FFBZ"]').exists()).toBe(true);
    expect(w.find('[data-testid="friend-pending:job-7"]').exists()).toBe(false);
    expect(activeUid(w)).toBe('u2');
    w.unmount();
  });

  it('人还停在那一行时，建好就顺势进入并拉消息', async () => {
    seedPending([pendingRow('job-7')]);
    const w = await mountPage([friend('u2', { job_id: 'job-u2' })]);

    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();
    expect(openResult).toBeNull(); // 默认桩立刻返回 u9
    await flushPromises();

    expect(activeUid(w)).toBe('u9');
    expect(askedUids).toContain('u9');
    // 回填的这一行要落盘：刷新页面回来还在，且不会被下一次 chat_friends 合并吃掉。
    const saved = JSON.parse(localStorage.getItem(PENDING_STORAGE_KEY) ?? '[]') as Friend[];
    expect(saved.map((f) => f.uid)).toContain('u9');
    expect(saved.find((f) => f.uid === 'u9')?.local_only).toBe(true);
    w.unmount();
  });

  it('切走再切回同一行，不会重复建会话', async () => {
    seedPending([pendingRow('job-7')]);
    openResult = deferred<{ uid: string }>();
    const w = await mountPage([friend('u2', { job_id: 'job-u2' })]);

    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();
    await w.find('[data-testid="friend-u2"]').trigger('click');
    await flushPromises();
    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();

    expect(openCalls).toBe(1);
    w.unmount();
  });

  it('建会话失败 ⇒ 占位行留着说明原因，再点一次能重试', async () => {
    seedPending([pendingRow('job-7')]);
    const w = await mountPage([friend('u2', { job_id: 'job-u2' })]);

    openResult = deferred<{ uid: string }>();
    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();
    // 用一条**不是**导航类失败的错：导航失败（浏览器没连上）会走「先去扫码登录」的引导语，
    // 那条另有测试覆盖；这里要验的是普通失败照样把行留着。
    openResult.reject(new Error('点了「立即沟通」但页面没有进入聊天页（可能触发了验证或风控）'));
    await flushPromises();

    expect((w.vm as { note: string }).note).toContain('建立会话失败');
    // 行还在：失败不该把用户的草稿行也一起丢掉。
    expect(w.find('[data-testid="friend-pending:job-7"]').exists()).toBe(true);

    // 换一条能成的结果再点一次：失败之后允许重试。
    openResult = deferred<{ uid: string }>();
    await w.find('[data-testid="friend-u2"]').trigger('click');
    await flushPromises();
    await w.find('[data-testid="friend-pending:job-7"]').trigger('click');
    await flushPromises();
    expect(openCalls).toBe(2);
    w.unmount();
  });
});
