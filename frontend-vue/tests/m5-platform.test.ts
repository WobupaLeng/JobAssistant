import { describe, expect, it } from 'vitest';

import {
  PRIMARY_PLATFORM,
  loginDot,
  loginDotClass,
  loginDotLabel,
  pickPlatform,
  platformCards,
  platformIds,
  platformPrefix,
} from '../src/lib_ui/platform-model';
import { PLATFORMS } from '../src/lib_ui/options';

describe('platform-model / 平台常量（C15：只从 options 常量表取）', () => {
  it('三平台与 options.ts 完全一致', () => {
    expect(platformIds()).toEqual(PLATFORMS.map((p) => p.id));
    expect(platformIds()).toEqual(['boss']);
  });

  it('主平台是 Boss', () => {
    expect(PRIMARY_PLATFORM).toBe('boss');
    expect(PLATFORMS.find((p) => p.id === PRIMARY_PLATFORM)?.name).toBe('Boss直聘');
  });

  it('testid 前缀照原型', () => {
    expect(platformPrefix('boss')).toBe('platform-boss');
    expect(platformPrefix('boss')).toBe('platform-boss');
  });
});

describe('platform-model / 卡片视图', () => {
  it('选中的平台出状态点与登录按钮，其余只出后缀', () => {
    const cards = platformCards('boss', false);
    const boss = cards.find((c) => c.id === 'boss')!;
    expect(boss.checked).toBe(true);
    expect(boss.loginVisible).toBe(true);
    expect(boss.loginLabel).toBe('登录');
    expect(boss.testId).toBe('platform-boss-card');
    expect(boss.checkTestId).toBe('platform-boss-check');
    expect(boss.statusTestId).toBe('platform-boss-status');
    expect(boss.loginTestId).toBe('platform-boss-login-btn');
  });

  it('登录进行中时按钮禁用', () => {
    const boss = platformCards('boss', true).find((c) => c.id === 'boss')!;
    expect(boss.loginDisabled).toBe(true);
    const idle = platformCards('boss', false).find((c) => c.id === 'boss')!;
    expect(idle.loginDisabled).toBe(false);
  });

  // 已登录后这一格不再给「登录」（会诱导重复扫码），改由红色「退出」接手；
  // `loggedIn` 是模板判据，按钮文案本身只剩「登录 / 登录中…」两态。
  it('已登录时标记 loggedIn，登录文案保持「登录」由模板让位给「退出」', () => {
    for (const busy of [false, true]) {
      const boss = platformCards('boss', busy, true).find((c) => c.id === 'boss')!;
      expect(boss.loggedIn).toBe(true);
      expect(boss.loginLabel).toBe(busy ? '登录中…' : '登录');
      expect(boss.loginTitle).toBe('打开 Boss 登录窗口扫码');
    }
    expect(platformCards('boss', false, false).find((c) => c.id === 'boss')!.loggedIn).toBe(false);
  });

  it('未登录仍显示「登录」且只在 busy 时禁用（不被本轮改动波及）', () => {
    const idle = platformCards('boss', false, false).find((c) => c.id === 'boss')!;
    expect(idle.loginLabel).toBe('登录');
    expect(idle.loginDisabled).toBe(false);
    expect(idle.loginTitle).toBe('打开 Boss 登录窗口扫码');
  });

});

describe('platform-model / pickPlatform', () => {
  it('未接入的平台不生效', () => {
    expect(pickPlatform('boss', 'nope')).toBe('boss');
  });

  it('不允许把选中清空', () => {
    expect(pickPlatform('boss', '')).toBe('boss');
  });
});

describe('platform-model / 登录状态点', () => {
  it('颜色与 running 无关：运行中且已登录仍为 ok', () => {
    expect(loginDot({ running: true, state: 'logged_in' })).toBe('ok');
    expect(loginDot({ running: true, state: 'ok' })).toBe('ok');
    expect(loginDot({ running: true, state: 'no_login' })).toBe('bad');
    expect(loginDotClass('unknown')).toBe('status-dot');
  });

  it('已登录/已保存算 ok', () => {
    expect(loginDot({ state: 'ok' })).toBe('ok');
    expect(loginDot({ state: 'logged_in' })).toBe('ok');
    expect(loginDot({ saved: true })).toBe('ok');
    expect(loginDotClass('ok')).toBe('status-dot is-ok');
    expect(loginDotLabel('ok')).toBe('已登录');
  });

  it('明确未登录为 bad；state 缺失/unknown 或没查过为 unknown', () => {
    expect(loginDot({ state: 'no_login' })).toBe('bad');
    expect(loginDot({ state: 'idle' })).toBe('bad');
    expect(loginDot({})).toBe('unknown');
    expect(loginDot({ state: 'unknown' })).toBe('unknown');
    expect(loginDot({ state: 'waiting', running: true })).toBe('unknown');
    expect(loginDot(null)).toBe('unknown');
    expect(loginDot(undefined)).toBe('unknown');
    expect(loginDotClass('bad')).toBe('status-dot is-bad');
    expect(loginDotLabel('bad')).toBe('未登录');
  });

  it('未知态也有标签', () => {
    expect(loginDotLabel('unknown')).toBe('登录状态未知');
  });
});
