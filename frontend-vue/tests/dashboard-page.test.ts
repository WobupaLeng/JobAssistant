// @vitest-environment jsdom
/**
 * 外壳结构契约（二级导航撤除后，ADR-004 / SPEC-001 决策 7 与 13）。
 *
 * 整页由 `App.vue` 分发，`DashboardPage` 依赖 Tauri invoke（jsdom 里没有），
 * 所以这里挂载 `AppShell` 来钉顶栏与骨架，而不是硬渲染整页。
 *
 * 守住的三条：
 * 1. **`page-*` 锚点全站命中 0**（SPEC-001 AC9）：外壳与页面都不再挂页锚点，
 *    概览容器改用原型的 `shell-dashboard`，两个浮层窗用各自的 `config-panel` / `decisions-panel`；
 * 2. **没有页签行**：顶栏一行到底，`nav-*` 锚点与 `.nav-tabs` 全部退役；
 * 3. **`theme-toggle` 全站恰好一处**，住在概览的 `.tb-tail`（TopbarTail）里，外壳不再自带一颗。
 */
import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import AppShell from '../src/components/AppShell.vue';
import { createDraft, decode, encode } from '../src/lib_ui/scrape-filter-model';

describe('AppShell 结构', () => {
  it('外壳没有页锚点与页签，只给页面骨架槽', () => {
    const w = mount(AppShell);
    expect(w.findAll('[data-testid^="page-"]')).toHaveLength(0);
    expect(w.find('[data-testid="app-root"]').exists()).toBe(true);
    expect(w.find('[data-testid="topbar"]').exists()).toBe(true);
    expect(w.find('.app-body').exists()).toBe(true);
    w.unmount();
  });

  it('二级导航已撤除：没有页签，也没有外壳自带的主题开关', () => {
    const w = mount(AppShell);
    expect(w.findAll('[data-testid^="nav-"]')).toHaveLength(0);
    expect(w.find('.nav-tabs').exists()).toBe(false);
    expect(w.find('[data-testid="theme-toggle"]').exists()).toBe(false);
    w.unmount();
  });

  it('顶栏留一个落位盒给概览的尾段（platform 卡 + .tb-tail teleport 进来）', () => {
    const w = mount(AppShell);
    const slot = w.find('[data-testid="topbar-tail-mount"]');
    expect(slot.exists()).toBe(true);
    // 落位盒必须是 .topbar 的直接子项，且自身不产生布局盒（display: contents，见 vue-dashboard.css）。
    expect(slot.element.parentElement?.getAttribute('data-testid')).toBe('topbar');
    w.unmount();
  });

  it('is-chat-open 由 prop 驱动，且挂在全应用唯一那个 .app-shell 上（列数 = 收起 − 1 的 CSS 钩子）', () => {
    const off = mount(AppShell);
    expect(off.findAll('.app-shell')).toHaveLength(1);
    expect(off.find('.app-shell').classes()).not.toContain('is-chat-open');
    off.unmount();
    const on = mount(AppShell, { props: { isChatOpen: true } });
    expect(on.findAll('.app-shell')).toHaveLength(1);
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
