/**
 * 自动投递的驱动泵（Leptos 版已退役，见 git 历史，提交 53d05cb 之前）。
 *
 * 每秒一次 tick：刷新后端状态 → 判断该不该发 → 最多发一条。
 *
 * 为什么是 `setTimeout` 链而不是 `setInterval`：每轮结束后再排下一次，
 * 这样一次慢 IPC 不会让两轮叠在一起；`stop()` 之后不再排队。
 * WebView 挂起时 `setTimeout` 不触发，循环自然停住——**已知且接受**（C16）。
 *
 * 铁律：间隔由后端给。这里只读 `next_at_ms`，从不自己算「20 秒 ± 5 秒」。
 */
import { autoGreetNext, autoGreetStatus } from '../../api/commands/auto';
import type { AutoStatusReply } from '../../api/types';
import { AUTO_TICK_MS, replyNote, shouldSend, statusText, type AutoPhase } from '../../lib_ui/auto-model';

export interface AutoQueueHooks {
  /** 每个 tick 用后端快照覆盖本地（含 idle 态，保证计数准确）。 */
  onStatus(st: AutoStatusReply): void;
  /** 一条正在路上：置 true/false，用于状态文案与按钮忙碌态。 */
  onSending(sending: boolean): void;
  /** 发出一条（后端回执 ok=true）后回调 —— 页面据此刷新列表与计数。 */
  onSent(): void;
  /** 人类可读的一行提示（成功也走这里，例如「连续失败 2 次，下一次重试」）。 */
  onNote(text: string): void;
}

export interface AutoQueue {
  /** 停止循环（幂等）。 */
  stop(): void;
  /** 立刻跑一拍（用户点开始 / 继续后不等 1 秒）。 */
  kick(): void;
}

/** 状态快照 → 「正在发送」时那句人类可读描述（后端没有「当前这条」字段）。 */
export function sendingLabelOf(): string {
  return '下一条';
}

export function startAutoQueue(hooks: AutoQueueHooks): AutoQueue {
  let alive = true;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inFlight = false;
  let status: AutoStatusReply | null = null;

  function schedule(delay: number): void {
    if (!alive) return;
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      void tick();
    }, delay);
  }

  async function tick(): Promise<void> {
    if (!alive) return;
    try {
      const st = await autoGreetStatus();
      if (!alive) return;
      status = st;
      hooks.onStatus(st);
    } catch (e) {
      if (!alive) return;
      // 状态拿不到时不要盲目发送：宁可这一拍跳过。
      hooks.onNote(`读取自动投递状态失败：${errText(e)}`);
      schedule(AUTO_TICK_MS);
      return;
    }

    if (status !== null && shouldSend(Date.now(), status, inFlight)) {
      await sendOne();
    }
    schedule(AUTO_TICK_MS);
  }

  async function sendOne(): Promise<void> {
    inFlight = true;
    hooks.onSending(true);
    try {
      const reply = await autoGreetNext(null);
      if (!alive) return;
      if (!reply.ok) {
        const msg = String(reply.error ?? '').trim() !== '' ? String(reply.error) : replyNote(reply);
        if (msg.trim() !== '') hooks.onNote(msg.trim());
      } else {
        hooks.onSent();
        const note = replyNote(reply);
        if (note.trim() !== '') hooks.onNote(note.trim());
      }
    } catch (e) {
      if (!alive) return;
      hooks.onNote(`发送招呼失败：${errText(e)}`);
    }

    // 不论成功与否都立刻重读状态：`next_at_ms` 只有后端知道，
    // 本次回执里的值不能直接拿去算下一次（跳过、失败、暂停的分支都不一样）。
    try {
      const st = await autoGreetStatus();
      if (alive) {
        status = st;
        hooks.onStatus(st);
      }
    } catch (e) {
      if (alive) hooks.onNote(`刷新自动投递状态失败：${errText(e)}`);
    }
    inFlight = false;
    hooks.onSending(false);
  }

  // 首拍立刻跑，不等 1 秒：进页面就能看到真实计数。
  schedule(0);

  return {
    stop(): void {
      alive = false;
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
    },
    kick(): void {
      if (!alive) return;
      schedule(0);
    },
  };
}

function errText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

/** 供页面渲染状态细节行时复用（避免两处各拼一遍）。 */
export function detailOf(st: AutoStatusReply, phase: AutoPhase, sending: boolean, label = ''): string {
  return statusText(st, phase, sending, label);
}
