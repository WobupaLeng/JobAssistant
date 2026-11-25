/**
 * QA 回归（缺陷 #3 中 + #5 低）：C9 的 `✔` 入库标记与事件流。
 *
 * 历史缺陷：`scrape_drain` 零调用，事件流整块缺失；而 `✔` 不是装饰——
 * E2E 用 `feed.count("✔")` 统计入库条数，丢了这个字符等于把断言弄瞎。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

import type { ScrapeUpdate } from '../src/api/types';
import {
  FEED_ACCEPTED_TAG,
  FEED_MAX,
  FEED_SKIPPED_TAG,
  toFeedLine,
  trimFeed,
} from '../src/lib_ui/scrape-panel-model';

const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', 'src');

function read(rel: string): string {
  return readFileSync(join(SRC, rel), 'utf8');
}

describe('toFeedLine：ScrapeUpdate → 事件流行', () => {
  it('入库：行首 ✔，带分值与摘要', () => {
    const u: ScrapeUpdate = { kind: 'job', summary: '后端开发 · 某公司', accepted: true, score: 86 };
    expect(toFeedLine(u)).toMatchObject({ tag: '✔', msg: '86 分 · 后端开发 · 某公司', tone: 'ok' });
    expect(FEED_ACCEPTED_TAG).toBe('✔');
  });

  it('分值缺失时不写「null 分」', () => {
    const u: ScrapeUpdate = { kind: 'job', summary: 'JD', accepted: true, score: null };
    expect(toFeedLine(u)?.msg).toBe('JD');
  });

  it('被规则跳过：行首「跳过」，tone=skip', () => {
    const u: ScrapeUpdate = { kind: 'job', summary: '外包公司', accepted: false, score: 0 };
    expect(toFeedLine(u)).toMatchObject({ tag: '跳过', msg: '外包公司', tone: 'skip' });
    expect(FEED_SKIPPED_TAG).toBe('跳过');
  });

  it('补上接收时刻与单调序号（与运行日志混排用）', () => {
    const u: ScrapeUpdate = { kind: 'job', summary: 'x', accepted: false, score: 0 };
    const line = toFeedLine(u, 1_700_000_000_000, 42);
    expect(line?.ts).toBe(1_700_000_000_000);
    expect(line?.n).toBe(42);
  });

  it('非岗位事件不进事件流（日志/登录/畸形/退出/重启各有去处）', () => {
    const others: ScrapeUpdate[] = [
      { kind: 'log', level: 'info', msg: 'x' },
      { kind: 'login', state: 'ok', msg: 'x' },
      { kind: 'malformed', line: 'x' },
      { kind: 'exited', code: 0 },
      { kind: 'restarted', pid: 1 },
    ];
    for (const u of others) expect(toFeedLine(u)).toBeNull();
  });
});

describe('trimFeed：只留最后 FEED_MAX 行', () => {
  it('未超限原样返回（且是副本）', () => {
    const lines = [{ ts: 1, n: 1, tag: '✔', msg: 'a', tone: 'ok' as const }];
    const out = trimFeed(lines);
    expect(out).toEqual(lines);
    expect(out).not.toBe(lines);
  });

  it('超限从头部丢', () => {
    const lines = Array.from({ length: FEED_MAX + 5 }, (_, i) => ({
      ts: i,
      n: i,
      tag: '✔',
      msg: String(i),
      tone: 'ok' as const,
    }));
    const out = trimFeed(lines);
    expect(out).toHaveLength(FEED_MAX);
    expect(out[0]?.msg).toBe('5');
    expect(out[out.length - 1]?.msg).toBe(String(FEED_MAX + 4));
  });
});

describe('接线：drain 真的被调用，渲染真的带 ✔', () => {
  const page = read('pages/DashboardPage.vue');
  const card = read('components/LogCard.vue');

  it('页面运行中拉 scrape_drain(50)，并把结果转行', () => {
    expect(page).toContain('await scrapeDrain(50)');
    expect(page).toContain('toFeedLine(u, Date.now(), feedN)');
    expect(page).toContain('if (progress.value.running) await drainFeed();');
  });

  it('LogCard 把事件行并进同一条时间线，并保留 scrape-feed 锚点', () => {
    expect(card).toContain('data-testid="scrape-feed"');
    expect(card).toContain('r.tag');
  });

  it('前端源码里不出现「已入库 0 条」这种伪造文案', () => {
    expect(page).not.toContain('已入库 0 条');
    expect(card).not.toContain('已入库 0 条');
  });
});
