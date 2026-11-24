/**
 * 自动投递条的三态 / 文案 / 动作分支（Leptos `features/auto/state.rs` 的等价语义）。
 *
 * 这里只测纯逻辑：相位推导、按钮文案与样式、`should_send` 判据、倒计时取整、
 * 状态细节行必须含「已发 N 条」、以及四个按钮分支。
 */
import { describe, expect, it } from 'vitest';

import type { AutoNextReply, AutoStatusReply } from '../src/api/types';
import { AUTO_TICK_MS } from '../src/lib_ui/auto-model';
import {
  autoPhase,
  barMode,
  emptyStatus,
  replyNote,
  secondsLeft,
  shouldSend,
  stateClass,
  stateLabel,
  statusDot,
  statusText,
  toggleAction,
  toggleClass,
  toggleLabel,
  togglePressed,
} from '../src/lib_ui/auto-model';

function status(patch: Partial<AutoStatusReply> = {}): AutoStatusReply {
  return { ...emptyStatus(), ...patch };
}

function reply(patch: Partial<AutoNextReply> = {}): AutoNextReply {
  return {
    ok: true,
    stage: 'sent',
    error: '',
    skip: false,
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
    ...patch,
  };
}

describe('相位推导：一律以后端为准', () => {
  it('idle：没在跑、没暂停、没有停止原因', () => {
    expect(autoPhase(status(), false)).toBe('idle');
  });

  it('running：后端说在跑', () => {
    expect(autoPhase(status({ running: true, next_at_ms: 1_000 }), false)).toBe('running');
  });

  it('paused：后端说已暂停（即使 running 仍为 true，暂停优先）', () => {
    expect(autoPhase(status({ paused: true, running: true }), false)).toBe('paused');
  });

  it('stopped：有停止原因、且没在跑', () => {
    expect(autoPhase(status({ stop_reason: '待投递列表已发完' }), false)).toBe('stopped');
  });

  it('sending 压过一切：一条正在路上就显示投递中', () => {
    expect(autoPhase(status({ paused: true }), true)).toBe('running');
    expect(autoPhase(status({ stop_reason: '已停止' }), true)).toBe('running');
  });

  it('本地不能靠「我点了开始」假定已开始：后端没 running 就是 idle', () => {
    expect(autoPhase(status({ running: false }), false)).toBe('idle');
  });

  it('barMode 只有三态：stopped 视觉上回到 idle', () => {
    expect(barMode('idle')).toBe('idle');
    expect(barMode('running')).toBe('running');
    expect(barMode('paused')).toBe('paused');
    expect(barMode('stopped')).toBe('idle');
  });
});

describe('三态按钮（SPEC §10.2）', () => {
  it('文案：未开始投递 / 投递中 / 已暂停投递', () => {
    expect(stateLabel('idle')).toBe('未开始投递');
    expect(stateLabel('running')).toBe('投递中');
    expect(stateLabel('paused')).toBe('已暂停投递');
    // 本轮已停：回到「未开始投递」，但按钮仍是「开始自动投递」。
    expect(stateLabel('stopped')).toBe('未开始投递');
  });

  it('主按钮文案：开始自动投递 / 暂停 / 继续', () => {
    expect(toggleLabel('idle')).toBe('开始自动投递');
    expect(toggleLabel('running')).toBe('暂停');
    expect(toggleLabel('paused')).toBe('继续');
    expect(toggleLabel('stopped')).toBe('开始自动投递');
  });

  it('主按钮样式：只有 running 用 btn-pause 警示色', () => {
    expect(toggleClass('running')).toBe('btn btn-sm btn-pause');
    expect(toggleClass('idle')).toBe('btn btn-sm btn-primary');
    expect(toggleClass('paused')).toBe('btn btn-sm btn-primary');
    expect(toggleClass('stopped')).toBe('btn btn-sm btn-primary');
  });

  it('aria-pressed：开始后（含暂停）为 true', () => {
    expect(togglePressed('idle')).toBe(false);
    expect(togglePressed('stopped')).toBe(false);
    expect(togglePressed('running')).toBe(true);
    expect(togglePressed('paused')).toBe(true);
  });

  it('状态标签 class：is-idle / is-running / is-paused 三选一', () => {
    expect(stateClass('idle')).toBe('auto-apply-label is-idle');
    expect(stateClass('stopped')).toBe('auto-apply-label is-idle');
    expect(stateClass('running')).toBe('auto-apply-label is-running');
    expect(stateClass('paused')).toBe('auto-apply-label is-paused');
  });

  it('点主按钮的四个分支：开始（先 reset 再 resume）/ 暂停 / 继续', () => {
    expect(toggleAction('idle')).toEqual({ kind: 'start' });
    expect(toggleAction('stopped')).toEqual({ kind: 'start' });
    expect(toggleAction('running')).toEqual({ kind: 'pause', reason: '用户已暂停' });
    expect(toggleAction('paused')).toEqual({ kind: 'resume' });
  });
});

describe('should_send：发不发只看后端到期没', () => {
  const now = 1_000_000;

  it('在跑且已到期 ⇒ 发', () => {
    expect(shouldSend(now, status({ running: true, next_at_ms: now - 1 }), false)).toBe(true);
    expect(shouldSend(now, status({ running: true, next_at_ms: now }), false)).toBe(true);
  });

  it('还没到点 ⇒ 不发', () => {
    expect(shouldSend(now, status({ running: true, next_at_ms: now + 1 }), false)).toBe(false);
  });

  it('next_at_ms 为 0（不在倒计时）⇒ 一律不发', () => {
    expect(shouldSend(now, status({ running: true, next_at_ms: 0 }), false)).toBe(false);
  });

  it('暂停 / 未运行 / 正在发送 ⇒ 不发', () => {
    expect(shouldSend(now, status({ running: true, paused: true, next_at_ms: now - 1 }), false)).toBe(false);
    expect(shouldSend(now, status({ running: false, next_at_ms: now - 1 }), false)).toBe(false);
    expect(shouldSend(now, status({ running: true, next_at_ms: now - 1 }), true)).toBe(false);
  });

  it('每拍最多一条：sending 期间不会再发', () => {
    const st = status({ running: true, next_at_ms: now - 1 });
    expect(shouldSend(now, st, false)).toBe(true);
    expect(shouldSend(now, st, true)).toBe(false);
  });

  it('前端不再有轮询间隔常量（状态请求全由心跳 60~90 秒与用户操作触发）', () => {
    expect(AUTO_TICK_MS).toBeUndefined();
  });
});

describe('倒计时取整', () => {
  it('向上取整，避免先跳 0 再发出', () => {
    expect(secondsLeft(0)).toBe(0);
    expect(secondsLeft(1)).toBe(1);
    expect(secondsLeft(999)).toBe(1);
    expect(secondsLeft(1_000)).toBe(1);
    expect(secondsLeft(1_001)).toBe(2);
    expect(secondsLeft(12_400)).toBe(13);
  });

  it('负数与非法值夹到 0', () => {
    expect(secondsLeft(-5)).toBe(0);
    expect(secondsLeft(Number.NaN)).toBe(0);
    expect(secondsLeft(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe('状态细节行：必须始终含「已发 N 条」', () => {
  it('idle', () => {
    expect(statusText(status({ sent: 0 }), 'idle', false)).toBe('未开始 · 已发 0 条');
  });

  it('running：计数 + 倒计时 + 自动停止提示 + 失败次数 + 待投递', () => {
    const text = statusText(
      status({ sent: 3, remaining_ms: 12_400, failures: 2, pending: 7 }),
      'running',
      false,
    );
    expect(text).toContain('已发 3 条');
    expect(text).toContain('下次 13 秒');
    expect(text).toContain('待投递列表发完自动停止');
    expect(text).toContain('连续失败 2 次');
    expect(text).toContain('待投递 7');
  });

  it('running 且 failures=0 时不出现「连续失败」', () => {
    const text = statusText(status({ sent: 1, remaining_ms: 5_000 }), 'running', false);
    expect(text).not.toContain('连续失败');
  });

  it('paused：停止原因 + 计数（无原因时用「已暂停」）', () => {
    expect(statusText(status({ sent: 4, stop_reason: '用户已暂停' }), 'paused', false)).toBe(
      '用户已暂停 · 已发 4 条',
    );
    expect(statusText(status({ sent: 4, stop_reason: '' }), 'paused', false)).toBe('已暂停 · 已发 4 条');
  });

  it('stopped：停止原因 + 计数（无原因时用「已停止」）', () => {
    expect(statusText(status({ sent: 9, stop_reason: '待投递列表已发完' }), 'stopped', false)).toBe(
      '待投递列表已发完 · 已发 9 条',
    );
    expect(statusText(status({ sent: 9 }), 'stopped', false)).toBe('已停止 · 已发 9 条');
  });

  it('sending：正在发送这条…（带人选时带上人选）', () => {
    expect(statusText(status({ sent: 2 }), 'running', true, '')).toBe('正在发送这条…下一条 · 已发 2 条');
    expect(statusText(status({ sent: 2 }), 'running', true, '张三')).toBe('正在发送这条…张三 · 已发 2 条');
  });
});

describe('状态点语气', () => {
  it('running / paused / idle / 连续失败停', () => {
    expect(statusDot('running', 0)).toBe('running');
    expect(statusDot('paused', 0)).toBe('warn');
    expect(statusDot('idle', 0)).toBe('idle');
    expect(statusDot('stopped', 2)).toBe('idle');
    expect(statusDot('stopped', 3)).toBe('bad');
  });
});

describe('回执行文案', () => {
  it('ok ⇒ 已发送：公司 · 岗位', () => {
    expect(replyNote(reply())).toBe('已发送：星瀚科技 · 后端工程师');
  });

  it('ok 但没公司岗位 ⇒ 只说「已发送」', () => {
    expect(replyNote(reply({ company: '', title: '' }))).toBe('已发送');
  });

  it('stage=done 不是错误：返回空串（待投递列表发完）', () => {
    expect(replyNote(reply({ ok: false, stage: 'done', error: '' }))).toBe('');
    expect(replyNote(reply({ ok: false, stage: 'done', error: '列表已空' }))).toBe('');
  });

  it('真失败：发送失败（阶段）：原因', () => {
    expect(replyNote(reply({ ok: false, stage: 'send', error: '会话已失效' }))).toBe(
      '发送失败（send）：会话已失效',
    );
  });

  it('失败但没原因 ⇒ 空串（不要弹一条空提示）', () => {
    expect(replyNote(reply({ ok: false, stage: 'send', error: '   ' }))).toBe('');
  });
});
