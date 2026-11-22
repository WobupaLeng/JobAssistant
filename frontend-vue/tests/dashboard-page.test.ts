// @vitest-environment jsdom
/**
 * 概览页结构契约。
 *
 * 与 M1 之前的版本相比，这里断言的是**迁移后的真实结构**：
 * 整页由 `App.vue` 分发，`DashboardPage` 依赖 Tauri invoke（jsdom 里没有），
 * 所以挂载 `AppShell` 来钉顶栏与页面锚点，而不是硬渲染整页。
 *
 * 不变的那条最重要的约束：**整份 DOM 里 page-* 锚点只能有一个**——
 * 外壳与页面各渲染一次就会有两个，E2E 的 `getByTestId('page-dashboard')`
 * 会直接抛「resolved to 2 elements」。
 */
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import AppShell from '../src/components/AppShell.vue';
import { pageTestId, navTestId } from '../src/lib_ui/nav';
import { createDraft, decode, encode } from '../src/lib_ui/scrape-filter-model';

describe('AppShell 结构', () => {
  it('外壳只渲染一个 page-* 锚点，且 slug 跟当前页走', () => {
    const w = mount(AppShell, { props: { nav: 'dashboard' } });
    const pages = w.findAll('[data-testid^="page-"]');
    expect(pages).toHaveLength(1);
    expect(pages[0]!.attributes('data-testid')).toBe(pageTestId('dashboard'));
    expect(w.find('[data-testid="app-root"]').exists()).toBe(true);
    expect(w.find('[data-testid="topbar"]').exists()).toBe(true);
    w.unmount();
  });

  it('切页只换锚点，不新增锚点', () => {
    const w = mount(AppShell, { props: { nav: 'config' } });
    expect(w.findAll('[data-testid^="page-"]')).toHaveLength(1);
    expect(w.find(`[data-testid="${pageTestId('config')}"]`).exists()).toBe(true);
    w.unmount();
  });

  it('showNav=false 时不渲染导航页签与内置主题按钮（概览页顶栏由页面自理）', () => {
    const w = mount(AppShell, { props: { nav: 'dashboard', showNav: false } });
    expect(w.find(`[data-testid="${navTestId('chat')}"]`).exists()).toBe(false);
    expect(w.find('[data-testid="theme-toggle"]').exists()).toBe(false);
    w.unmount();
  });

  it('showNav=true 时页签齐全，点击 emit navigate 且外壳自己不切页', async () => {
    const w = mount(AppShell, { props: { nav: 'dashboard', showNav: true } });
    const tabs = w.findAll('[data-testid^="nav-"]');
    expect(tabs.length).toBeGreaterThanOrEqual(4);
    await w.find(`[data-testid="${navTestId('config')}"]`).trigger('click');
    expect(w.emitted('navigate')?.[0]).toEqual(['config']);
    expect(w.find(`[data-testid="${pageTestId('dashboard')}"]`).exists()).toBe(true);
    w.unmount();
  });

  it('is-chat-open 由 prop 驱动（列数 = 收起 − 1 的 CSS 钩子）', () => {
    const off = mount(AppShell, { props: { nav: 'dashboard' } });
    expect(off.find('.app-shell').classes()).not.toContain('is-chat-open');
    off.unmount();
    const on = mount(AppShell, { props: { nav: 'dashboard', isChatOpen: true } });
    expect(on.find('.app-shell').classes()).toContain('is-chat-open');
    on.unmount();
  });
});

describe('筛选草稿的本地往返（替换旧的 createFilter/filterPayload 用例）', () => {
  it('默认草稿能原样编码后解回来', () => {
    const d = createDraft();
    expect(decode(encode(d))).toEqual(d);
  });

  it('没有本地草稿时 decode(null) 给出默认值，而不是抛错', () => {
    expect(decode(null)).toEqual(createDraft());
  });
});
