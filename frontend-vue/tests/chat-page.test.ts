// @vitest-environment jsdom
/**
 * ChatPage 独立挂载测试：只验证「点谁 ⇒ 展开成什么样」与自动投递条的搬移，
 * 不碰真实 IPC —— `window.__TAURI_INTERNALS__.invoke` 在这里被换成假桩。
 *
 * 这属于「纯状态机 + 单处渲染」的可测部分；视觉与滚动留给打包后的手工冒烟。
 */
import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AutoStatusReply, Friend } from '../src/api/types';
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

function autoStatus(patch: Partial<AutoStatusReply> = {}): AutoStatusReply {
  return {
    running: false,
    paused: false,
    sent: 0,
    failures: 0,
    next_at_ms: 0,
    remaining_ms: 0,
    stop_reason: '',
    pending: 0,
    ...patch,
  };
}

let status: AutoStatusReply = autoStatus();

function fakeInvoke(cmd: string, payload?: unknown): Promise<unknown> {
  calls.push({ cmd, payload });
  switch (cmd) {
    case 'chat_friends':
      return Promise.resolve({ items: friendsSeed() });
    case 'chat_messages':
      return Promise.resolve({ items: [] });
    case 'chat_resumes':
      return Promise.resolve({ items: [] });
    case 'list_expectations':
      return Promise.resolve([]);
    case 'auto_greet_status':
      return Promise.resolve(status);
    case 'auto_greet_next':
      return Promise.resolve({
        ok: true,
        stage: 'sent',
        error: '',
        skip: '',
        job_id: 'j1',
        company: '星瀚科技',
        title: '后端工程师',
        text: '您好',
        source: 'rule',
        note: '',
        tokens: 0,
        next_at_ms: 0,
        remaining_ms: 0,
        sent: 1,
      });
    case 'chat_send':
      return Promise.resolve({ text: '回执文本', job_id: null });
    case 'login_status':
      return Promise.resolve({ platform: 'boss', state: 'logged_in', saved: true, error: '' });
    default:
      return Promise.resolve(true);
  }
}

/** 会话列表的测试夹具：默认两位，个别用例可临时换一批（未读角标 / 空列表）。 */
let friendsOverride: Friend[] | null = null;

function friendsSeed(): Friend[] {
  return friendsOverride ?? [friend('u1'), friend('u2')];
}

/**
 * 只在真实 window 上挂假 IPC —— 不能整个替换 window，
 * 否则 jsdom 的 Event/MouseEvent 构造器一起消失，@vue/test-utils 的 trigger() 会炸。
 */
function stubs() {
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, payload?: unknown) => fakeInvoke(cmd, payload),
  };
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  // jsdom 的 innerWidth 是只读默认值，按 1440 覆盖以对齐宽度公式的验收档位。
  Object.defineProperty(window, 'innerWidth', { value: 1440, configurable: true, writable: true });
}

beforeEach(() => {
  calls.length = 0;
  status = autoStatus();
  stubs();
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  friendsOverride = null;
  vi.restoreAllMocks();
});

async function mountPage(opts: { friends?: Friend[] } = {}) {
  friendsOverride = opts.friends ?? null;
  const wrapper = mount(ChatPage, { attachTo: document.body });
  await flushPromises();
  return wrapper;
}

describe('展开 / 收起（SPEC §3 状态机 + 单处渲染）', () => {
  it('首帧：右栏与对话框是层的两个根节点，`page-chat` 整页锚点已退役（ADR-004）', async () => {
    const w = await mountPage();
    // 聊天不再是「页」：那个 <section class="page page-chat"> 一起删掉，
    // 右栏与对话框直接成为外壳 .app-body 的 flex 子项（SPEC §2.1 / A15）。
    expect(w.find('[data-testid="page-chat"]').exists()).toBe(false);
    expect(w.find('.page-chat').exists()).toBe(false);
    const roots = w.findAll('[data-testid="chat-rail"], [data-testid="chat-detach"]');
    expect(roots).toHaveLength(2);
    // DOM 顺序照原型 HTML（L1734 右栏 在前、L1773 对话框 在后）：Tab 序 = 主列 → 右栏 → 对话框。
    expect(roots[0]!.attributes('data-testid')).toBe('chat-rail');
    expect(roots[1]!.attributes('data-testid')).toBe('chat-detach');
    expect(roots[1]!.attributes('hidden')).toBeDefined();
    w.unmount();
  });

  it('点某位 Boss ⇒ 对话框展开（去掉 hidden）且该行高亮', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();

    const detach = w.find('[data-testid="chat-detach"]');
    expect(detach.attributes('hidden')).toBeUndefined();
    expect(detach.classes()).toContain('is-open');
    expect(w.find('[data-testid="friend-u1"]').classes()).toContain('is-active');
    expect(w.find('[data-testid="chat-detach-sub"]').text()).toContain('星瀚科技');
    // SPEC §4.1：消息容器的契约名是 chat-detach-thread（不是 Leptos 遗留的 chat-feed）。
    expect(w.find('[data-testid="chat-detach-thread"]').exists()).toBe(true);
    w.unmount();
  });

  it('Esc 只关最上面那一层：配置浮层开着时对话框不动（SPEC §6.1 / A10 / AC5）', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    const detach = () => w.find('[data-testid="chat-detach"]');
    expect(detach().attributes('hidden')).toBeUndefined();

    // 上层（配置窗）在场：这层 Esc 不出手。
    const overlay = document.createElement('div');
    overlay.setAttribute('data-testid', 'config-panel');
    document.body.appendChild(overlay);
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flushPromises();
    expect(detach().attributes('hidden')).toBeUndefined();

    // 上层关掉后，下一次 Esc 才收对话框（选中仍保留，A7）。
    overlay.remove();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await flushPromises();
    expect(detach().attributes('hidden')).toBeDefined();
    expect(detach().classes()).not.toContain('is-open');
    w.unmount();
  });

  it('展开态列表出 chat-detach-unread 角标，收起态只出 chat-rail 角标（SPEC §4.2）', async () => {
    const w = await mountPage({
      friends: [friend('u1', { unread: 3 }), friend('u2', { unread: 7 })],
    });
    // 收起态：右栏里是 chat-rail-unread-<uid>。
    expect(w.find('[data-testid="chat-rail-unread-u1"]').exists()).toBe(true);
    expect(w.find('[data-testid="chat-detach-unread-u1"]').exists()).toBe(false);
    expect(w.find('[data-testid="chat-rail-unread-u1"]').text()).toBe('3');

    // 展开态：列表搬进对话框，角标改用契约名 chat-detach-unread-<uid>。
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    // 点开会把这一位的未读清零（Leptos 同语义），所以断言看旁边那位 u2。
    expect(w.find('[data-testid="chat-detach-unread-u1"]').exists()).toBe(false);
    expect(w.find('[data-testid="chat-rail-unread-u1"]').exists()).toBe(false);
    expect(w.find('[data-testid="chat-detach-unread-u2"]').exists()).toBe(true);
    expect(w.find('[data-testid="chat-detach-unread-u2"]').text()).toBe('7');
    w.unmount();
  });

  it('收起按钮：回到 hidden，但选中保留；再点同一位能重开（A7/A8）', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();

    await w.find('[data-testid="chat-detach-close-btn"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-testid="chat-detach"]').attributes('hidden')).toBeDefined();
    // 选中保留：右栏里那一行仍然是 is-active。
    expect(w.find('[data-testid="friend-u1"]').classes()).toContain('is-active');

    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-testid="chat-detach"]').attributes('hidden')).toBeUndefined();
    w.unmount();
  });

  it('切人只换内容，不改变展开态', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    await w.find('[data-testid="friend-u2"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-testid="chat-detach"]').attributes('hidden')).toBeUndefined();
    expect(w.find('[data-testid="friend-u2"]').classes()).toContain('is-active');
    // 换人会重新拉消息（uid 在载荷里）。
    const fetched = calls.filter((c) => c.cmd === 'chat_messages').map((c) => c.payload);
    expect(fetched).toContainEqual({ uid: 'u2' });
    w.unmount();
  });

  // 收编后（REMAINING#3）：--detach-w 只由 AppShell 的 syncCanvas() 写到 <html>，
  // 页面不再往对话框上写行内样式。宽度仍由公式给出，但读的是同一个函数。
  it('对话框宽度由 canvas 公式给出（1440 ⇒ 720px），页面不再写行内 --detach-w', async () => {
    const w = await mountPage();
    const detach = w.find('[data-testid="chat-detach"]');
    expect(detach.attributes('style') ?? '').not.toContain('--detach-w');
    // 页面组件仍暴露同一个公式（defineExpose 的 detachW 镜像），数值与 syncCanvas 一致。
    expect((w.vm as unknown as { detachW: number }).detachW).toBe(720);
    w.unmount();
  });
});

describe('自动投递条：两处二选一，不复制节点', () => {
  it('收起态：状态条在右栏内，且全站只有一个 auto-greet-toggle-btn', async () => {
    const w = await mountPage();
    expect(w.find('[data-testid="chat-rail"] [data-testid="auto-apply-bar"]').exists()).toBe(true);
    expect(w.findAll('[data-testid="auto-greet-toggle-btn"]').length).toBe(1);
    expect(w.find('[data-testid="auto-apply-state"]').text()).toBe('未开始投递');
    expect(w.find('[data-testid="auto-greet-stop-btn"]').exists()).toBe(false);
    w.unmount();
  });

  it('展开态：状态条搬进对话框头，右栏内不再有它，仍然只有一个主按钮', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();

    expect(w.find('[data-testid="chat-detach"] [data-testid="auto-apply-bar"]').exists()).toBe(true);
    expect(w.find('[data-testid="chat-rail"] [data-testid="auto-apply-bar"]').exists()).toBe(false);
    expect(w.findAll('[data-testid="auto-greet-toggle-btn"]').length).toBe(1);
    w.unmount();
  });

  it('后端说在跑 ⇒ 主按钮变「暂停」且出现结束按钮（三态来自后端）', async () => {
    status = autoStatus({ running: true, sent: 3, pending: 5, next_at_ms: Date.now() + 60_000 });
    const w = await mountPage();
    await flushPromises();

    const toggle = w.find('[data-testid="auto-greet-toggle-btn"]');
    expect(toggle.text()).toBe('暂停');
    expect(toggle.classes()).toContain('btn-pause');
    expect(toggle.attributes('aria-pressed')).toBe('true');
    expect(w.find('[data-testid="auto-greet-stop-btn"]').exists()).toBe(true);
    expect(w.find('[data-testid="auto-apply-state"]').text()).toBe('投递中');
    expect(w.find('[data-testid="auto-apply-done"]').text()).toContain('3');
    expect(w.find('[data-testid="auto-apply-pending"]').text()).toContain('5');
    w.unmount();
  });
});

describe('唯一输入框', () => {
  it('全站只有一个 compose 输入框，且它只出现在展开态的对话区', async () => {
    const w = await mountPage();
    expect(w.findAll('[data-testid="chat-detach-compose-input"]').length).toBe(0);
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    expect(w.findAll('[data-testid="chat-detach-compose-input"]').length).toBe(1);
    // Boss 列表里没有输入框（除搜索框）。
    expect(w.find('[data-testid="chat-detach-rail"] textarea').exists()).toBe(false);
    w.unmount();
  });

  it('输入后点发送 ⇒ 走 chat_send（uid + 文本；空 jobId 键被 prune 掉）', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();

    const input = w.find('[data-testid="chat-detach-compose-input"]');
    await input.setValue('你好呀');
    await w.find('[data-testid="chat-detach-send-btn"]').trigger('click');
    await flushPromises();

    // 只认本页发出的那一条；`calls` 里可能有其它命令。
    const sent = calls.filter((c) => c.cmd === 'chat_send');
    expect(sent.length).toBe(1);
    // 平铺载荷：jobId 为空时按 C2/prune 规则整键省略（不是发 null）。
    expect(sent[0].payload).toEqual({ uid: 'u1', text: '你好呀' });
    w.unmount();
  });

  it('空草稿时发送按钮禁用', async () => {
    const w = await mountPage();
    await w.find('[data-testid="friend-u1"]').trigger('click');
    await flushPromises();
    const btn = w.find('[data-testid="chat-detach-send-btn"]');
    expect(btn.attributes('disabled')).toBeDefined();
    w.unmount();
  });
});

describe('空态与错误态', () => {
  it('未选中时对话区显示两句逐字空态', async () => {
    const w = await mountPage();
    // 还没点过任何 Boss：对话区走「在左侧 Boss 列表点一位」分支。
    expect(w.find('[data-testid="chat-detach-empty"]').exists()).toBe(true);
    expect(w.find('[data-testid="chat-detach-empty-title"]').text()).toBe(
      '在左侧 Boss 列表点一位，开始聊天',
    );
    expect(w.text()).toContain('点左侧 Boss 列表里的任意一位即可查看聊天');
    w.unmount();
  });

  it('读会话失败时右栏出错误块并给重试按钮', async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: (cmd: string) => {
        if (cmd === 'chat_friends') return Promise.reject(new Error('读取会话失败：boom'));
        return fakeInvoke(cmd);
      },
    };
    const w = await mountPage();
    expect(w.find('[data-testid="chat-rail-error"]').exists()).toBe(true);
    expect(w.find('[data-testid="chat-rail-retry-btn"]').exists()).toBe(true);
    w.unmount();
  });
});
