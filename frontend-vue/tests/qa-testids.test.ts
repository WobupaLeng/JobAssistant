/**
 * QA 回归（缺陷 #4 中）：testid 必须与冻结原型逐字一致。
 *
 * 历史缺陷成串出现：列表页签写成 `job-tab-pending` / `job-tab-count-pending`，
 * 而原型是 `job-tab-pending-tab` / `job-tab-pending-count`；排序缺
 * `job-sort-arrow`；敏感库入口写成 `sensitive-open-btn`；`chat-detach-head`
 * 只有 class 没有 testid；右栏 `chat-rail` 重复挂了两处。
 * 每一条都会让 E2E 选择器落空，且**不会报错**——只是永远选不到元素。
 *
 * 真源是 `prototypes/ui-v1/index.html`（UI_READY 冻结版）。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

function read(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf8');
}

const SRC_FILES = [
  'frontend-vue/src/components/JobListView.vue',
  'frontend-vue/src/components/LogCard.vue',
  'frontend-vue/src/components/FilterPanel.vue',
  'frontend-vue/src/components/TopbarTail.vue',
  'frontend-vue/src/components/chat/ChatDetach.vue',
  'frontend-vue/src/components/chat/FriendList.vue',
  'frontend-vue/src/pages/ChatPage.vue',
  'frontend-vue/src/pages/DashboardPage.vue',
];

const SRC = SRC_FILES.map((f) => [f, read(f)] as const);

describe('列表页签 / 排序（原型 1684 · 1692-1694）', () => {
  const list = read('frontend-vue/src/components/JobListView.vue');

  it('页签是 job-tab-<slug>-tab，计数是 job-tab-<slug>-count', () => {
    expect(list).toContain(':data-testid="`job-tab-${t.slug}-tab`"');
    expect(list).toContain(':data-testid="`job-tab-${t.slug}-count`"');
    expect(list).not.toContain('`job-tab-${t.slug}`');
    expect(list).not.toContain('`job-tab-count-${t.slug}`');
  });

  it('排序按钮带 -btn，箭头带 job-sort-arrow（全站唯一）', () => {
    // 原型 :1692-1694 是 job-sort-time-btn / -salary-btn / -score-btn，
    // 断言必须查 -btn 后缀（此前只查前缀，改名成原型口径也照样绿，等于没守）。
    expect(list).toContain(':data-testid="`job-sort-${sortSlug(s.key)}-btn`"');
    expect(list).not.toContain(':data-testid="`job-sort-${sortSlug(s.key)}`"');
    expect(list).toContain('data-testid="job-sort-arrow"');
    const holders = SRC.filter(([, s]) => s.includes('data-testid="job-sort-arrow"'));
    expect(holders).toHaveLength(1);
  });

  it('空态、批量条、错误态锚点齐全', () => {
    for (const id of ['job-batch-bar', 'job-empty-title', 'job-empty-sub', 'job-empty-filter-btn', 'job-empty-start-btn']) {
      expect(list).toContain(`data-testid="${id}"`);
    }
  });
});

describe('顶栏与对话框', () => {
  it('敏感库入口是 sensitive-open（不是 sensitive-open-btn）', () => {
    const tail = read('frontend-vue/src/components/TopbarTail.vue');
    expect(tail).toContain('data-testid="sensitive-open"');
    for (const [, s] of SRC) expect(s).not.toContain('sensitive-open-btn');
  });

  it('chat-detach-head 有 testid', () => {
    const detach = read('frontend-vue/src/components/chat/ChatDetach.vue');
    expect(detach).toContain('class="chat-detach-head" data-testid="chat-detach-head"');
  });

  it('chat-rail 这个 testid 全站恰好一处（页面 aside），FriendList 不重复挂', () => {
    const hits = SRC.flatMap(([file, s]) =>
      s
        .split('\n')
        .map((line, i) => ({ file, line: line.trim(), no: i + 1 }))
        // 只看**属性**，不看注释正文（注释里提到这个选择器是正常的说明）。
        .filter((x) => x.line.includes('class="chat-rail" data-testid="chat-rail"')),
    );
    expect(hits).toHaveLength(1);
    expect(hits[0]?.file).toBe('frontend-vue/src/pages/ChatPage.vue');
    expect(read('frontend-vue/src/components/chat/FriendList.vue')).not.toContain(':data-testid="ids.root"');
  });
});

describe('日志行（原型 2526 用固定 log-row）', () => {
  it('每行都有 data-testid="log-row"', () => {
    const card = read('frontend-vue/src/components/LogCard.vue');
    expect(card).toContain('class="log-row" data-testid="log-row"');
  });
});

describe('禁用 testid 全部为 0', () => {
  const banned = [
    'chat-rail-expand-btn',
    'chat-detach-back-btn',
    'chat-detach-side',
    'chat-detach-l2',
    'job-batch-hint',
    'config-token-section',
    'config-advanced-section',
  ];

  it('源码里一个都不出现', () => {
    const all = SRC.map(([, s]) => s).join('\n');
    for (const id of banned) expect(all).not.toContain(id);
  });
});
