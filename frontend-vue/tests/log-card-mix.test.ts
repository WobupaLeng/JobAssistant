// @vitest-environment jsdom
/**
 * 日志卡「一条流 + 五项页签」（用户 m00182 / m00374）：
 * - 运行日志与岗位事件流（✔ / 跳过）混排在同一条时间线上，事件行带接收时刻；
 * - 页签摆在「决策记录」按钮左边（卡头），单选，默认「全部」；
 * - 点「事件」只剩事件行，点「错误」只剩错误级日志；
 * - 「决策记录」按钮仍然发 `open-decisions`（覆盖页不归这里管，决策也不混进这条线）。
 */
import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import LogCard from '../src/components/LogCard.vue';
import { fmtTimeSecs } from '../src/lib_ui/time';
import type { ScrapeProgressLike } from '../src/lib_ui/scrape-panel-model';

const progress: ScrapeProgressLike = {
  running: false,
  scraped: 3,
  inserted: 1,
  duplicates: 0,
  below_threshold: 1,
  rejected: 1,
  last_title: '光蕴科技',
};

const lines = [
  { ts: 1_700_000_000_000, seq: 1, level: 'info' as const, message: '抓取开始' },
  { ts: 1_700_000_001_000, seq: 2, level: 'warn' as const, message: '低于门槛：3 条' },
  { ts: 1_700_000_002_000, seq: 3, level: 'error' as const, message: '浏览器断线' },
];

const feed = [
  { ts: 1_700_000_000_500, n: 1, tag: '跳过', msg: '外包公司', tone: 'skip' as const },
  { ts: 1_700_000_003_000, n: 2, tag: '✔', msg: '86 分 · 后端开发 · 某公司', tone: 'ok' as const },
];

function render(extra: Record<string, unknown> = {}) {
  return mount(LogCard, { props: { progress, lines, feed, ...extra } });
}

describe('日志卡：运行日志与岗位事件混排', () => {
  it('默认「全部」：两类行按时间正序排成一条线', () => {
    const w = render();
    const rows = w.findAll('[data-testid="log-row"]');
    expect(rows).toHaveLength(5);
    expect(rows.map((r) => r.attributes('data-level'))).toEqual(['info', 'event', 'warn', 'error', 'event']);
  });

  it('事件行同样带时间列（补的是 drain 接收时刻）', () => {
    const w = render();
    const rows = w.findAll('[data-testid="log-row"]');
    expect(rows[1]?.find('.log-time').text()).toBe(fmtTimeSecs(1_700_000_000_500));
    expect(rows[1]?.text()).toContain('外包公司');
    expect(rows[4]?.text()).toContain('✔');
    expect(rows[4]?.text()).toContain('86 分 · 后端开发');
  });

  it('没传事件时只剩日志行（老调用方不受影响）', () => {
    const w = mount(LogCard, { props: { progress, lines } });
    expect(w.findAll('[data-testid="log-row"]')).toHaveLength(3);
  });

  it('事件行仍在 scrape-feed 容器里（C9 靠它数 ✔）', () => {
    const w = render();
    const box = w.find('[data-testid="scrape-feed"]');
    expect(box.exists()).toBe(true);
    expect(box.find('[data-testid="log-rows"]').exists()).toBe(true);
  });
});

describe('日志卡：五项单选页签', () => {
  it('五个页签，顺序「全部 / 错误 / 警告 / 信息 / 事件」', () => {
    const w = render();
    const labels = w.findAll('[data-testid^="log-tab-"]').map((b) => b.text());
    expect(labels).toEqual(['全部', '错误', '警告', '信息', '事件']);
  });

  it('页签在卡头，且紧挨在「决策记录」按钮左边', () => {
    const w = render();
    const head = w.find('[data-testid="log-card"] > .card-head');
    const tabs = w.find('[data-testid="log-tabs"]');
    expect(head.element.contains(tabs.element)).toBe(true);
    const kids = Array.from(head.element.children);
    const tabsAt = kids.indexOf(tabs.element as Element);
    const btnAt = kids.findIndex((el) => el.getAttribute('data-testid') === 'log-decisions-btn');
    expect(tabsAt).toBe(btnAt - 1);
  });

  it('默认停在「全部」', () => {
    const w = render();
    expect(w.find('[data-testid="log-tab-all"]').attributes('aria-selected')).toBe('true');
  });

  it('点「事件」只剩 ✔ / 跳过', async () => {
    const w = render();
    await w.find('[data-testid="log-tab-event"]').trigger('click');
    const rows = w.findAll('[data-testid="log-row"]');
    expect(rows).toHaveLength(2);
    expect(rows.every((r) => r.attributes('data-level') === 'event')).toBe(true);
  });

  it('点「错误」只剩错误级日志，事件行也一起被筛掉', async () => {
    const w = render();
    await w.find('[data-testid="log-tab-error"]').trigger('click');
    const rows = w.findAll('[data-testid="log-row"]');
    expect(rows).toHaveLength(1);
    expect(rows[0]?.text()).toContain('浏览器断线');
  });

  it('该类一条都没有时给这一类的空态，而不是「尚无事件」', async () => {
    const w = render({ feed: [] });
    await w.find('[data-testid="log-tab-event"]').trigger('click');
    expect(w.find('[data-testid="log-empty"]').text()).toContain('还没有岗位事件');
  });

  it('「决策记录」按钮照旧发 open-decisions（覆盖页不改）', async () => {
    const w = render();
    await w.find('[data-testid="log-decisions-btn"]').trigger('click');
    expect(w.emitted('open-decisions')).toHaveLength(1);
  });
});

describe('行内的列对齐（用户 m01419）', () => {
  it('时间 / 级别 / 正文三列定宽，级别词长短不同也不会让正文起点错开', () => {
    // 级别词有 `WARN`（四字母）也有「跳过」（两汉字），flex + auto 宽度会让
    // 正文起点逐行错开。这里钉住三列定宽（时间 14ch = MM-DD HH:mm:ss 的字符位，
    // 级别 3.6em，正文吃剩下）。时间列写窄了会压到级别列上（用户报过重叠）。
    const here = dirname(fileURLToPath(import.meta.url));
    const css = readFileSync(join(here, '..', 'styles', 'vue-dashboard.css'), 'utf8');
    const at = css.indexOf('.log-row {');
    expect(at).toBeGreaterThan(-1);
    const block = css.slice(at, css.indexOf('}', at));
    expect(block).toContain('grid-template-columns: 14ch 3.6em minmax(0, 1fr)');
    expect(block).toContain('column-gap: 8px');
    // 时间戳实际长度（`fmtTimeSecs` 输出 MM-DD HH:mm:ss）必须装得进 14ch，
    // 否则 nowrap 的文本会压到级别列上。
    expect(fmtTimeSecs(Date.now())).toHaveLength(14);
    // 两列都不许换行：换行会把下一行的列挤回上一行的高度里。
    for (const sel of ['.log-time {', '.log-level {']) {
      const start = css.indexOf(sel);
      expect(start).toBeGreaterThan(-1);
      expect(css.slice(start, css.indexOf('}', start))).toContain('white-space: nowrap');
    }
  });
});