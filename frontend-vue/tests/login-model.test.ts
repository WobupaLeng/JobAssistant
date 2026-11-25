/**
 * `src/lib_ui/login-model.ts` 的纯函数契约。
 *
 * 三块：
 * 1. `mergeLoginState` —— 事件是旁路推送，**未知态不许覆盖已知态**（本轮最关键的一条）；
 * 2. 徽标 —— 口径必须与 `platform-model.loginDot` 完全一致，只多一个 title；
 * 3. 「保存登录 / 退出」按钮派生 + `resolve-scan-flag` 的显隐。
 */
import { describe, expect, it } from 'vitest';

import {
  LOGIN_STATES,
  isKnownState,
  isLoggedIn,
  isLoginError,
  isWaiting,
  loginBadge,
  loginStateLabel,
  loginStatusFromEvent,
  logoutAction,
  mergeLoginState,
  scanFlagVisible,
} from '../src/lib_ui/login-model';
import { loginDot } from '../src/lib_ui/platform-model';
import type { LoginStatePayload } from '../src/api/events';
import type { LoginStatus } from '../src/api/types';

function status(over: Partial<LoginStatus> = {}): LoginStatus {
  return {
    platform: 'boss',
    state: 'logged_in',
    running: true,
    message: '',
    saved: false,
    ...over,
  };
}

function event(over: Partial<LoginStatePayload> = {}): LoginStatePayload {
  return { platform: 'boss', state: 'logged_in', logged_in: true, ...over };
}

describe('login-model: mergeLoginState（未知态不覆盖已知态）', () => {
  it('没有权威状态时，用事件填一份最小状态（running=true，saved=false）', () => {
    const got = mergeLoginState(null, event({ state: 'waiting', logged_in: false }));
    expect(got).toEqual({
      platform: 'boss',
      state: 'waiting',
      running: true,
      message: '',
      saved: false,
    });
  });

  it('事件态为空/unknown 时原样返回权威状态（同一个对象，不新建）', () => {
    const cur = status({ state: 'logged_in' });
    expect(mergeLoginState(cur, event({ state: '' }))).toBe(cur);
    expect(mergeLoginState(cur, event({ state: 'unknown' }))).toBe(cur);
    expect(mergeLoginState(cur, null)).toBe(cur);
    expect(isKnownState('Unknown')).toBe(false);
    expect(isKnownState(' waiting ')).toBe(true);
  });

  it('权威 state 已被事件改过：迟到的 unknown 不会把它降级', () => {
    const cur = status({ state: 'logged_in', saved: true, message: '已保存' });
    const afterClosed = mergeLoginState(cur, event({ state: 'closed', logged_in: false }));
    expect(afterClosed?.state).toBe('closed');
    const afterUnknown = mergeLoginState(afterClosed, event({ state: 'unknown', logged_in: false }));
    expect(afterUnknown?.state).toBe('closed');
    expect(afterUnknown?.saved).toBe(true);
    expect(afterUnknown?.message).toBe('已保存');
  });

  it('保留权威的 running / saved / message，只换 state', () => {
    const cur = status({ state: 'waiting', running: true, saved: false, message: 'data:image/png' });
    const got = mergeLoginState(cur, event({ state: 'logged_in', logged_in: true }));
    expect(got).toEqual({
      platform: 'boss',
      state: 'logged_in',
      running: true,
      message: 'data:image/png',
      saved: false,
    });
    // 不就地改入参
    expect(cur.state).toBe('waiting');
  });

  it('事件平台与权威平台不同时忽略', () => {
    const cur = status({ platform: 'boss' });
    const other = event({ platform: 'zhilian', state: 'closed', logged_in: false });
    expect(mergeLoginState(cur, other)).toBe(cur);
  });

  it('state 相同时直接返回原对象（免触发无意义的重渲染）', () => {
    const cur = status({ state: 'logged_in' });
    expect(mergeLoginState(cur, event({ state: 'logged_in' }))).toBe(cur);
  });

  it('五个冻结 state 都能落到权威状态上', () => {
    for (const s of LOGIN_STATES) {
      const got = mergeLoginState(status({ state: 'no_login' }), event({ state: s }));
      expect(got?.state).toBe(s);
    }
  });

  it('loginStatusFromEvent 只带知道的字段（不带 running/saved/message）', () => {
    expect(loginStatusFromEvent(event({ state: 'waiting' }))).toEqual({
      platform: 'boss',
      state: 'waiting',
    });
  });

  it('三个状态谓词', () => {
    expect(isLoggedIn('logged_in')).toBe(true);
    expect(isLoggedIn('waiting')).toBe(false);
    expect(isWaiting('waiting')).toBe(true);
    expect(isLoginError('error')).toBe(true);
    expect(isLoginError('no_login')).toBe(false);
  });
});

describe('login-model: 徽标（与 platform-model.loginDot 同口径）', () => {
  const cases: (LoginStatus | null)[] = [
    null,
    status({ state: 'logged_in', running: true }),
    status({ state: 'logged_in', running: false }),
    status({ state: 'no_login', running: false }),
    status({ state: 'waiting', running: true }),
    status({ state: 'error', running: false, message: '扫码超时' }),
    status({ state: 'closed', running: false }),
    status({ state: 'no_login', running: false, saved: true }),
  ];

  it('八种输入的点与 platform-model.loginDot 完全一致', () => {
    for (const c of cases) {
      expect(loginBadge(c).dot).toBe(loginDot(c));
    }
  });

  it('等待扫码时点是 unknown（灰），不误报红点', () => {
    const b = loginBadge(status({ state: 'waiting', running: true }));
    expect(b.dot).toBe('unknown');
    expect(b.title).toBe('等待扫码');
  });

  it('登录失败时 title 带上后端原话，不吞错误', () => {
    const b = loginBadge(status({ state: 'error', running: false, message: '扫码超时' }));
    expect(b.dot).toBe('bad');
    expect(b.title).toBe('登录失败：扫码超时');
  });

  it('失败但没消息时 title 只说「登录失败」', () => {
    expect(loginBadge(status({ state: 'error', running: false, message: '  ' })).title).toBe(
      '登录失败',
    );
  });

  it('aria-label 文案与 platform-model.loginDotLabel 一致', () => {
    expect(loginBadge(null).dotLabel).toBe('登录状态未知');
    expect(loginBadge(status({ state: 'logged_in', running: false })).dotLabel).toBe('已登录');
    expect(loginBadge(status({ state: 'no_login', running: false })).dotLabel).toBe('未登录');
  });

  it('loginStateLabel 覆盖五个态 + 未知', () => {
    expect(loginStateLabel('logged_in')).toBe('已登录');
    expect(loginStateLabel('waiting')).toBe('等待扫码');
    expect(loginStateLabel('closed')).toBe('登录窗口已关闭');
    expect(loginStateLabel('error')).toBe('登录失败');
    expect(loginStateLabel('no_login')).toBe('尚未登录');
    expect(loginStateLabel('')).toBe('登录状态未知');
  });
});

describe('login-model: 退出登录 / 扫码', () => {
  // 登录格三态互斥：未登录是「登录」按钮，已登录才换成红色「退出」，扫码中贴二维码。
  // 登录标记由后端自动落库，界面不再有「保存登录」；`saved` 只剩徽标双态用途。
  it('退出：只有已登录才出现（红按钮），未登录不出，忙碌时禁用', () => {
    const a = logoutAction(status({ state: 'logged_in' }), false);
    expect(a.visible).toBe(true);
    expect(a.testId).toBe('platform-logout-btn');
    expect(a.label).toBe('退出');
    expect(a.btnClass).toContain('btn-danger');
    // 浏览器未连接（unknown）按未登录算：那一格要显示「登录 / 重新连接浏览器」，
    // 不能同时给出「退出」。
    expect(logoutAction(status({ state: 'unknown', saved: true }), false).visible).toBe(false);
    expect(logoutAction(status({ state: 'logged_in' }), true).disabled).toBe(true);
    expect(logoutAction(status({ state: 'no_login', running: false }), false).visible).toBe(false);
    expect(logoutAction(null, false).visible).toBe(false);
  });

  it('resolve-scan-flag：只在 waiting 态显示（登录窗已开、等扫码）', () => {
    expect(scanFlagVisible('waiting')).toBe(true);
    expect(scanFlagVisible('logged_in')).toBe(false);
    expect(scanFlagVisible('closed')).toBe(false);
    expect(scanFlagVisible(null)).toBe(false);
  });
});
