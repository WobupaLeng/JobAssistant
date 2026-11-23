/**
 * `lib_ui/nav.ts`：层副作用规则（ARCH-003 §5.6 的那条不变量）。
 *
 * 概览层**恒为** `false`：它自己按 5 秒轮询页签条数，再叠一层「进入即重拉」
 * 会让每次切层都打两条重复请求。这条规则写错不会报错，只会静默多一倍请求。
 *
 * **二级导航已撤除**（[`ADR-004`](../docs/adr/ADR-004-legacy-chat-orchestration.md) +
 * legacy-baseline §9 Q-1 + 原型 L1413「用户裁决 5」）：`chat` 不再是层标识，
 * `nav-<slug>` 页签锚点随之退役，`page-<slug>` 只剩概览与两个浮层窗。
 */
import { describe, expect, it } from 'vitest';

import * as navApi from '../src/lib_ui/nav';

import { NAV_ALL, NAV_ENTER, entersSideEffect } from '../src/lib_ui/nav';

describe('nav/entersSideEffect', () => {
  it('只有概览没有副作用', () => {
    expect(entersSideEffect('dashboard')).toBe(false);
    expect(entersSideEffect('config')).toBe(true);
    expect(entersSideEffect('logs')).toBe(true);
  });

  it('有副作用的层集合恰好是除概览外的两个（顺序不变），且不含已退役的 chat', () => {
    expect(NAV_ALL).toEqual(['dashboard', 'config', 'logs']);
    expect(NAV_ENTER).toEqual(['config', 'logs']);
    expect(NAV_ALL.filter(entersSideEffect)).toEqual([...NAV_ENTER]);
    expect(NAV_ALL).not.toContain('chat');
  });

  it('锚点助手已退役：nav.ts 的导出面里没有 navTestId / pageTestId', () => {
    const keys = Object.keys(navApi);
    expect(keys).not.toContain('navTestId');
    expect(keys).not.toContain('pageTestId');
    // 剩下的公开面只有层标识与副作用表。
    expect(keys.sort()).toEqual(['NAV_ALL', 'NAV_ENTER', 'entersSideEffect']);
  });
});
