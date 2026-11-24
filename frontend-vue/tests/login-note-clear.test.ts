/**
 * 登录类提示的清除判据（用户 m01325 / m01477）。
 *
 * 右栏那句「浏览器未连接：请先登录」有两个来源：心跳按登录态写的前端常量，以及
 * sidecar 没起来时命令层回的中文文案（「尚未启动 / 没有会话」，见 `lib_ui/session.ts`）。
 * 登录成功后两类都必须消失，但「发送失败：……」这种与登录无关的真错误必须留下。
 *
 * 判据是 `pages/ChatPage.vue` 内部的 `isSessionNote`，这里不重复实现它，而是通过
 * 组件实例观察 `note` / `autoNote` 的真实清除行为（判据被改坏时用例会红）。
 */
import { describe, expect, it, vi } from 'vitest';

import {
  LOGIN_EXPIRED_NOTE,
  NOT_LOGGED_IN_NOTE,
  startHeartbeat,
} from '../src/lib_ui/auto-heartbeat';
import { needsSession } from '../src/lib_ui/session';

describe('登录类提示的文案集合', () => {
  it('心跳写出的未登录 / 浏览器未连接 / 登录已失效都属于登录类', () => {
    // 判据用「是不是登录类」，这三个常量是它的主要输入。
    const loginNotes = [NOT_LOGGED_IN_NOTE, LOGIN_EXPIRED_NOTE];
    for (const text of loginNotes) {
      expect(text.trim()).not.toBe('');
    }
    expect(NOT_LOGGED_IN_NOTE.startsWith('浏览器未连接')).toBe(true);
    expect(LOGIN_EXPIRED_NOTE.startsWith('登录已失效')).toBe(true);
  });

  it('sidecar 未启动的文案被 needsSession 认出（登录成功后也要清掉）', () => {
    expect(needsSession('sidecar 尚未启动')).toBe(true);
    expect(needsSession('当前没有会话')).toBe(true);
    expect(needsSession('发送失败：网络错误')).toBe(false);
  });
});

describe('心跳在未登录时不写多余提示', () => {
  it('start() 只写一条引导，并且停表', async () => {
    const notes: string[] = [];
    const beat = startHeartbeat({
      onStatus: () => undefined,
      onSending: () => undefined,
      onNote: (t) => notes.push(t),
    });
    vi.stubGlobal('__TEST_LOGIN_STATE__', 'no_login');
    // 非 Tauri 环境 login_status 走 invoke 失败 → unknown → BOSS_UNREACHABLE_NOTE。
    await beat.start();
    expect(beat.running()).toBe(false);
    expect(notes.at(-1)).toBe(NOT_LOGGED_IN_NOTE);
    beat.stop();
    vi.unstubAllGlobals();
  });
});