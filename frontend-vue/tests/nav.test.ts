/**
 * `lib_ui/nav.ts`：导航副作用规则（ARCH-003 §5.6 的那条不变量）。
 *
 * 概览页**恒为** `false`：它自己按 5 秒轮询页签条数，再叠一层「进入即重拉」
 * 会让每次切页都打两条重复请求。这条规则写错不会报错，只会静默多一倍请求。
 */
import { describe, expect, it } from 'vitest';

import { NAV, NAV_ALL, NAV_ENTER, entersSideEffect, navTestId, pageTestId } from '../src/lib_ui/nav';

describe('nav/entersSideEffect', () => {
  it('只有概览页没有副作用', () => {
    expect(entersSideEffect('dashboard')).toBe(false);
    expect(entersSideEffect('chat')).toBe(true);
    expect(entersSideEffect('config')).toBe(true);
    expect(entersSideEffect('logs')).toBe(true);
  });

  it('有副作用的页面集合恰好是除概览外的三个（顺序不变）', () => {
    expect(NAV_ALL).toEqual(['dashboard', 'chat', 'config', 'logs']);
    expect(NAV_ENTER).toEqual(['chat', 'config', 'logs']);
    expect(NAV_ALL.filter(entersSideEffect)).toEqual([...NAV_ENTER]);
  });

  it('页签文案与顺序（E2E 依赖的可见文案）', () => {
    expect(NAV.map((n) => n.name)).toEqual(['概览', 'Boss 消息', '配置', '决策记录']);
  });

  it('data-testid 前缀：nav-<slug> / page-<slug>', () => {
    expect(navTestId('dashboard')).toBe('nav-dashboard');
    expect(pageTestId('logs')).toBe('page-logs');
    expect(NAV_ALL.map(navTestId)).toEqual(['nav-dashboard', 'nav-chat', 'nav-config', 'nav-logs']);
  });
});
