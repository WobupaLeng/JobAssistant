/**
 * 自动投递的纯逻辑。
 *
 * 行为对照的是 Leptos 版的自动投递状态机（Leptos 版已退役，见 git 历史，提交 53d05cb 之前）。
 *
 * ## 节奏规则归属（不可改动）
 * 「下一条什么时候发」**完全由后端决定**（`core/domain/auto_greet.rs`：
 * 20 秒基准 ± 5 秒随机，下限 15 秒）。前端只做两件事：
 *
 * 1. 每秒读一次 `auto_greet_status()` 刷新倒计时文字；
 * 2. 当后端回执的 `next_at_ms` **已到期**时才调 `auto_greet_next()` 发一条。
 *
 * 前端**不得**自己算间隔、不得自己排下一次时刻——那会把反爬红线搬到
 * 一个可被最小化 / 刷新打断的定时器上。WebView 挂起时 `setTimeout` 不触发，
 * 循环自然停在 await 上，这是已知且接受的行为（C16 同族）。
 */
import type { AutoNextReply, AutoStatusReply } from '../api/types';
import { AUTO_TICK_MS } from './chat-model';

export { AUTO_TICK_MS };

/**
 * 界面态。`running` / `paused` **一律以后端为准**：
 * 用户点了开始，前端不能自行假定已开始（后端可能因连续失败立刻暂停）。
 */
export type AutoPhase = 'idle' | 'running' | 'paused' | 'stopped';

/** 由后端状态推导界面态。`sending` 是前端自己的瞬时标志（一条正在路上）。 */
export function autoPhase(st: AutoStatusReply, sending: boolean): AutoPhase {
  if (sending) return 'running';
  if (st.paused) return 'paused';
  if (st.running) return 'running';
  if (String(st.stop_reason ?? '').trim() === '') return 'idle';
  return 'stopped';
}

/**
 * 原型口径的三态（冻结原型只有 idle / running / paused 三个 `data-mode`）：
 * 「本轮已停」在视觉上回到 idle —— 主按钮仍是「开始自动投递」。
 */
export function barMode(phase: AutoPhase): 'idle' | 'running' | 'paused' {
  if (phase === 'running') return 'running';
  if (phase === 'paused') return 'paused';
  return 'idle';
}

/** 主按钮文案（原型逐字：开始自动投递 / 暂停 / 继续）。 */
export function toggleLabel(phase: AutoPhase): string {
  if (phase === 'running') return '暂停';
  if (phase === 'paused') return '继续';
  return '开始自动投递';
}

/** 主按钮 class：running 用 `.btn-pause` 警示色，其余主色。 */
export function toggleClass(phase: AutoPhase): string {
  return phase === 'running' ? 'btn btn-sm btn-pause' : 'btn btn-sm btn-primary';
}

/** `aria-pressed`：开始后为 true（含暂停态），idle / stopped 为 false。 */
export function togglePressed(phase: AutoPhase): boolean {
  return phase === 'running' || phase === 'paused';
}

/** 状态标签文案（裁决 N：先读「现在处于什么状态」）。 */
export function stateLabel(phase: AutoPhase): string {
  if (phase === 'running') return '投递中';
  if (phase === 'paused') return '已暂停投递';
  return '未开始投递';
}

/** 状态标签 class：三态点色 灰 / 主色绿 / 警示琥珀。 */
export function stateClass(phase: AutoPhase): string {
  if (phase === 'running') return 'auto-apply-label is-running';
  if (phase === 'paused') return 'auto-apply-label is-paused';
  return 'auto-apply-label is-idle';
}

/**
 * 现在是否该调 `auto_greet_next`。
 *
 * 判据只有一条：后端说「在跑、没暂停」且 `next_at_ms` 已到期。
 * `next_at_ms === 0`（不在倒计时）一律不发——那是暂停 / 停止态。
 */
export function shouldSend(nowMs: number, st: AutoStatusReply, sending: boolean): boolean {
  if (sending || st.paused || !st.running) return false;
  const next = Number(st.next_at_ms ?? 0);
  return next !== 0 && nowMs >= next;
}

/**
 * 「下次 12 秒」里的秒数。向上取整：12.4 秒显示「13 秒」而不是「12 秒」，
 * 否则会先跳 0 再真的发出去，看起来像卡了一下。
 */
export function secondsLeft(remainingMs: number): number {
  const v = Number.isFinite(remainingMs) ? Math.max(0, remainingMs) : 0;
  return Math.floor((v + 999) / 1000);
}

/**
 * 状态细节行（原型的 `title` 提示，不占版面）。
 *
 * **必须始终含「已发 N 条」**：E2E 用正则认这一段，少了它
 * 「自动投递到底发了几条」在界面上就无从核对。
 */
export function statusText(
  st: AutoStatusReply,
  phase: AutoPhase,
  sending: boolean,
  sendingLabel = '',
): string {
  const count = `已发 ${st.sent} 条`;
  if (sending) {
    const who = sendingLabel.trim() === '' ? '下一条' : sendingLabel.trim();
    return `正在发送这条…${who} · ${count}`;
  }
  if (phase === 'paused') {
    const reason = String(st.stop_reason ?? '').trim() === '' ? '已暂停' : String(st.stop_reason);
    return `${reason} · ${count}`;
  }
  if (phase === 'stopped') {
    const reason = String(st.stop_reason ?? '').trim() === '' ? '已停止' : String(st.stop_reason);
    return `${reason} · ${count}`;
  }
  if (phase === 'idle') return `未开始 · ${count}`;
  const parts = [count, `下次 ${secondsLeft(Number(st.remaining_ms ?? 0))} 秒`, '待投递列表发完自动停止'];
  if (st.failures > 0) parts.push(`连续失败 ${st.failures} 次`);
  if (st.pending >= 0) parts.push(`待投递 ${st.pending}`);
  return parts.join(' · ');
}

/** 状态点语气：running 呼吸 / warn 暂停 / idle 未开始 / bad 连续失败停。 */
export function statusDot(phase: AutoPhase, failures: number): 'running' | 'warn' | 'idle' | 'bad' {
  if (phase === 'running') return 'running';
  if (phase === 'paused') return 'warn';
  if (phase === 'idle') return 'idle';
  return failures >= 3 ? 'bad' : 'idle';
}

/**
 * 一次 `auto_greet_next` 回执 → 一条给人看的结果行。空串表示没什么可说的。
 * `stage === 'done'`（待投递列表发完）**不是错误**，不要染成红色。
 */
export function replyNote(r: AutoNextReply): string {
  if (r.ok) {
    const who = [r.company, r.title].map((x) => String(x ?? '').trim()).filter((x) => x !== '');
    return who.length === 0 ? '已发送' : `已发送：${who.join(' · ')}`;
  }
  if (String(r.stage ?? '') === 'done') return '';
  const err = String(r.error ?? '').trim();
  if (err === '') return '';
  return `发送失败（${r.stage}）：${err}`;
}

/**
 * 点主按钮该做什么（Leptos `on_toggle` 的四分支）。
 *
 * - idle / stopped → 先 `reset` 清零上一轮计数，再 `resume`；
 * - running → `pause('用户已暂停')`；
 * - paused → `resume`。
 */
export type AutoAction = { kind: 'start' } | { kind: 'pause'; reason: string } | { kind: 'resume' };

export function toggleAction(phase: AutoPhase): AutoAction {
  if (phase === 'running') return { kind: 'pause', reason: '用户已暂停' };
  if (phase === 'paused') return { kind: 'resume' };
  return { kind: 'start' };
}

/** 一个空的状态快照：首帧还没有后端回执时用它渲染（计数全 0）。 */
export function emptyStatus(): AutoStatusReply {
  return {
    running: false,
    paused: false,
    sent: 0,
    failures: 0,
    next_at_ms: 0,
    remaining_ms: 0,
    stop_reason: '',
    pending: 0,
  };
}
