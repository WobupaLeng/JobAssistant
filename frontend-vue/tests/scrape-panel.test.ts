/**
 * 长按多选状态机（SPEC §16.3 裁决 S）与抓取条三态（§14）。
 *
 * 长按的 500ms / 8px 是契约常量，别改数字；这里不依赖 DOM 计时器，
 * 只测判定函数与状态迁移。
 */
import { describe, expect, it, vi } from 'vitest';
import {
  LONG_PRESS_MOVE_TOLERANCE,
  LONG_PRESS_MS,
  armLongPress,
  clickBlank,
  clickCard,
  emptySelection,
  enterSelecting,
  exitSelecting,
  ignorePointerTarget,
  interactiveSelector,
  movedBeyond,
  setChecked,
} from '../src/lib_ui/longpress';
import {
  LLM_PREFILTER_DEFAULT,
  MAX_JD_CEILING,
  buttonClass,
  buttonLabel,
  buttonState,
  clampPrefilter,
  lastTitleText,
  progressStats,
  progressText,
  scoreModeSegs,
  srSummary,
  startDisabled,
  mainDisabled,
  tokenText,
} from '../src/lib_ui/scrape-panel-model';

describe('长按多选', () => {
  it('时长与位移阈值是契约常量', () => {
    expect(LONG_PRESS_MS).toBe(500);
    expect(LONG_PRESS_MOVE_TOLERANCE).toBe(8);
  });

  it('位移 >8px 才算移动（正好 8px 不算）', () => {
    // 走欧氏距离的平方比较：8px 恰好不算越界，8.01px 算。
    expect(movedBeyond({ x: 0, y: 0 }, { x: 8, y: 0 })).toBe(false);
    expect(movedBeyond({ x: 0, y: 0 }, { x: 8.01, y: 0 })).toBe(true);
    expect(movedBeyond({ x: 0, y: 0 }, { x: 9, y: 0 })).toBe(true);
    expect(movedBeyond({ x: 0, y: 0 }, { x: 0, y: -12 })).toBe(true);
  });

  it('长按落在控件或打招呼方块上就不启动', () => {
    expect(ignorePointerTarget('BUTTON', true)).toBe(true);
    expect(ignorePointerTarget('INPUT', true)).toBe(true);
    expect(ignorePointerTarget('DIV', true)).toBe(true);
    expect(ignorePointerTarget('DIV', false)).toBe(false);
    expect(interactiveSelector()).toContain('[data-greet]');
  });

  it('armLongPress 到点才触发，cancel 后不触发', () => {
    vi.useFakeTimers();
    try {
      const fire = vi.fn();
      const h = armLongPress(fire);
      vi.advanceTimersByTime(LONG_PRESS_MS - 1);
      expect(fire).not.toHaveBeenCalled();
      vi.advanceTimersByTime(1);
      expect(fire).toHaveBeenCalledTimes(1);

      const fire2 = vi.fn();
      const h2 = armLongPress(fire2);
      h2.cancel();
      vi.advanceTimersByTime(LONG_PRESS_MS * 2);
      expect(fire2).not.toHaveBeenCalled();
      h.cancel();
    } finally {
      vi.useRealTimers();
    }
  });

  it('进入多选即勾中长按那张卡', () => {
    const s = enterSelecting('a');
    expect(s.selecting).toBe(true);
    expect(s.checked).toEqual(['a']);
  });

  it('多选态下点卡片是反选，勾到 0 个仍留在多选态', () => {
    let s = clickCard(enterSelecting('a'), 'b');
    expect(s.checked).toEqual(['a', 'b']);
    s = clickCard(s, 'a');
    s = clickCard(s, 'b');
    expect(s.checked).toEqual([]);
    // 退出多选只走显式出口（空白处单击 / 切页签 / 批量操作完成），
    // 逐张取消到 0 不再自动退出。
    expect(s.selecting).toBe(true);
  });

  it('点空白处退出多选', () => {
    expect(clickBlank(enterSelecting('a'))).toEqual(emptySelection());
  });

  it('退出后勾选清空', () => {
    expect(exitSelecting()).toEqual({ selecting: false, checked: [] });
    expect(setChecked(['x', 'y']).checked).toEqual(['x', 'y']);
  });
});

describe('抓取条三态', () => {
  it('空闲 → 开始抓取（主色）', () => {
    expect(buttonState(false, false)).toBe('idle');
    expect(buttonLabel('idle')).toBe('开始抓取');
    expect(buttonClass('idle')).toContain('btn-primary');
  });

  it('运行中 → 暂停抓取（琥珀色）', () => {
    expect(buttonState(true, false)).toBe('running');
    expect(buttonLabel('running')).toBe('暂停抓取');
    expect(buttonClass('running')).toContain('btn-pause');
  });

  it('用户暂停且进程已停 → 继续抓取', () => {
    expect(buttonState(false, true)).toBe('paused');
    expect(buttonLabel('paused')).toBe('继续抓取');
  });

  it('启动中与运行中都不允许再点', () => {
    expect(startDisabled(true, false)).toBe(true);
    expect(startDisabled(false, true)).toBe(true);
    expect(startDisabled(false, false)).toBe(false);
  });

  it('运行中主按钮「暂停抓取」必须可点（只有启动中禁用）', () => {
    expect(mainDisabled(false)).toBe(false);
    expect(mainDisabled(true)).toBe(true);
  });

  it('max JD 上限与后端 MAX_JD_CAP 一致（500）', () => {
    expect(MAX_JD_CEILING).toBe(500);
  });
});

describe('进度文案（C9 逐字）', () => {
  const p = {
    scraped: 120,
    inserted: 40,
    duplicates: 30,
    below_threshold: 20,
    rejected: 5,
    running: true,
  };

  it('顶条文案逐字', () => {
    // 已入库取进度里的 inserted（不是另开一个参数）。
    expect(progressText(p)).toBe('抓取中 · 已入库 40 条 · 已抓 120 · 低于门槛 20 · 规则否决 5');
    expect(progressText({ ...p, running: false })).toBe(
      '空闲 · 已入库 40 条 · 已抓 120 · 低于门槛 20 · 规则否决 5',
    );
  });

  it('sr-only 五段摘要逐字', () => {
    expect(srSummary(p)).toBe('已抓 120 · 入库 40 · 重复 30 · 低于门槛 20 · 规则否决 5');
  });

  it('五个 Stat 的标签与 testid 逐字', () => {
    expect(progressStats(p).map((s) => [s.testId, s.label, s.value])).toEqual([
      ['scrape-scraped', '已抓', 120],
      ['scrape-inserted', '入库', 40],
      ['scrape-duplicates', '重复', 30],
      ['scrape-below', '低于门槛', 20],
      ['scrape-rejected', '规则否决', 5],
    ]);
  });

  it('最近一条标题为空时给破折号', () => {
    expect(lastTitleText('后端工程师')).toBe('最近：后端工程师');
    expect(lastTitleText('  ')).toBe('最近：—');
  });
});

describe('计分方式分段（规则 / 大模型）', () => {
  it('两段都可选；选中谁谁 active', () => {
    const rule = scoreModeSegs('rule');
    expect(rule.find((s) => s.testId === 'scrape-mode-rule-seg')?.active).toBe(true);
    expect(rule.find((s) => s.testId === 'scrape-mode-llm-seg')?.active).toBe(false);
    const llm = scoreModeSegs('llm');
    expect(llm.find((s) => s.testId === 'scrape-mode-llm-seg')?.active).toBe(true);
    expect(llm.find((s) => s.testId === 'scrape-mode-rule-seg')?.active).toBe(false);
  });

  it('大模型段用中文标签且不再 disabled', () => {
    const llm = scoreModeSegs('rule').find((s) => s.testId === 'scrape-mode-llm-seg');
    expect(llm?.label).toBe('大模型打分');
    expect(llm?.disabled).toBe(false);
  });

  it('初筛线夹到 0-100，非数字回落默认 50', () => {
    expect(clampPrefilter(0)).toBe(0);
    expect(clampPrefilter(50)).toBe(50);
    expect(clampPrefilter(120)).toBe(100);
    expect(clampPrefilter(-3)).toBe(0);
    expect(clampPrefilter(70.9)).toBe(70);
    expect(clampPrefilter('60')).toBe(60);
    expect(clampPrefilter('')).toBe(LLM_PREFILTER_DEFAULT);
    expect(clampPrefilter(Number.NaN)).toBe(LLM_PREFILTER_DEFAULT);
  });
});

describe('token 读数', () => {
  it('千分位分隔（与原型读数一致）', () => {
    expect(tokenText(0)).toBe('0');
    expect(tokenText(999)).toBe('999');
    expect(tokenText(9999)).toBe('9,999');
    expect(tokenText(12345)).toBe('12,345');
  });
});
