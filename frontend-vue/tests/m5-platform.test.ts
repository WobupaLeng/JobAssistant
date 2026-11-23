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
    expect(platformIds()).toEqual(['boss', 'zhilian', '51job']);
  });

  it('主平台是 Boss', () => {
    expect(PRIMARY_PLATFORM).toBe('boss');
    expect(PLATFORMS.find((p) => p.id === PRIMARY_PLATFORM)?.name).toBe('Boss直聘');
  });

  it('testid 前缀照原型', () => {
    expect(platformPrefix('boss')).toBe('platform-boss');
    expect(platformPrefix('51job')).toBe('platform-51job');
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

  it('未接入的平台永远 is-off、禁选、未被选中', () => {
    for (const id of ['zhilian', '51job']) {
      const c = platformCards('boss', false).find((x) => x.id === id)!;
      expect(c.mode).toBe('off');
      expect(c.checked).toBe(false);
      expect(c.loginVisible).toBe(false);
      expect(c.cardClass).toContain('is-off');
      expect(c.title).toBe('未接入');
    }
  });

  it('登录进行中时按钮禁用', () => {
    const boss = platformCards('boss', true).find((c) => c.id === 'boss')!;
    expect(boss.loginDisabled).toBe(true);
    const idle = platformCards('boss', false).find((c) => c.id === 'boss')!;
    expect(idle.loginDisabled).toBe(false);
  });

  it('选中项换到别的 id 时 checked 跟着走', () => {
    const cards = platformCards('zhilian', false);
    expect(cards.find((c) => c.id === 'boss')!.checked).toBe(false);
  });
});

describe('platform-model / pickPlatform', () => {
  it('可以切到已接入的平台', () => {
    expect(pickPlatform('boss', 'boss')).toBe('boss');
  });

  it('未接入的平台不生效', () => {
    expect(pickPlatform('boss', 'zhilian')).toBe('boss');
    expect(pickPlatform('boss', '51job')).toBe('boss');
    expect(pickPlatform('boss', 'nope')).toBe('boss');
  });

  it('不允许把选中清空', () => {
    expect(pickPlatform('boss', '')).toBe('boss');
  });
});

describe('platform-model / 登录状态点', () => {
  it('运行中算未知（还在扫码）', () => {
    expect(loginDot({ running: true, state: 'ok' })).toBe('unknown');
    expect(loginDotClass('unknown')).toBe('status-dot');
  });

  it('已登录/已保存算 ok', () => {
    expect(loginDot({ state: 'ok' })).toBe('ok');
    expect(loginDot({ state: 'logged_in' })).toBe('ok');
    expect(loginDot({ saved: true })).toBe('ok');
    expect(loginDotClass('ok')).toBe('status-dot is-ok');
    expect(loginDotLabel('ok')).toBe('已登录');
  });

  it('其余算未登录；完全没状态时是未知', () => {
    expect(loginDot({ state: 'idle' })).toBe('bad');
    expect(loginDot({})).toBe('bad');
    expect(loginDot(null)).toBe('unknown');
    expect(loginDot(undefined)).toBe('unknown');
    expect(loginDotClass('bad')).toBe('status-dot is-bad');
    expect(loginDotLabel('bad')).toBe('未登录');
  });

  it('未知态也有标签', () => {
    expect(loginDotLabel('unknown')).toBe('登录状态未知');
  });
});
