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
import { existsSync, readFileSync, readdirSync } from 'node:fs';
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

describe('撤导航后的锚点净口径（用户裁决 5 / SPEC-001 AC9）', () => {
  /** AC9 的「命中 0」必须覆盖整个 src，抽六个文件不够。 */
  function readTree(dir: string): string {
    const out: string[] = [];
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) out.push(readTree(p));
      else if (/\.(vue|ts)$/.test(e.name)) out.push(readFileSync(p, 'utf8'));
    }
    return out.join('\n');
  }
  const src = readTree(join(ROOT, 'frontend-vue', 'src'));

  it('nav-<slug> 与 page-<slug> 锚点、锚点助手、页签 class 全部命中 0', () => {
    expect(src).not.toMatch(/data-testid="(nav|page)-/);
    // 助手名只允许出现在注释里（说明它退役了），调用点必须为 0。
    expect(src).not.toMatch(/\bnavTestId\s*\(/);
    expect(src).not.toMatch(/\bpageTestId\s*\(/);
    expect(src).not.toMatch(/\bNAV_BLOCK_CLASS\b/);
    expect(src).not.toMatch(/class="[^"]*nav-tabs/);
  });

  it('撤导航后三个入口都还在：右栏 Boss 行 / 顶栏配置 / 日志卡头部决策记录', () => {
    expect(src).toContain('data-testid="shell-config-btn"');
    expect(src).toContain('data-testid="log-decisions-btn"');
    expect(read('frontend-vue/src/components/chat/FriendList.vue')).toContain('`friend-${');
  });

  it('概览容器锚点照原型是 shell-dashboard（page-dashboard 已退役）', () => {
    expect(src).toContain('data-testid="shell-dashboard"');
  });

  it('求职期望行退役：chat-expect-* 与 .expect-row 命中 0（SPEC-001 Rev3/Rev4）', () => {
    expect(src).not.toContain('chat-expect');
    expect(src).not.toContain('ExpectRow');
    expect(existsSync(join(ROOT, 'frontend-vue', 'src', 'components', 'chat', 'ExpectRow.vue'))).toBe(false);
    // 契约面不动：api 封装与后端命令仍在（期望改由筛选面板「读取期望职位」承载）。
    expect(read('frontend-vue/src/api/commands/chat.ts')).toContain('list_expectations');
    expect(src).toContain('scrape-filter-expect-btn');
    const css = readFileSync(join(ROOT, 'frontend-vue', 'styles', 'vue-chat.css'), 'utf8');
    expect(css).not.toMatch(/\.expect-row\s*\{/);
  });

  it('token 读数两份 DOM 各唯一一份，文案是「累计 N token」（原型 L1400-1406 / A32）', () => {
    const tail = read('frontend-vue/src/components/TopbarTail.vue');
    expect(tail).toContain('data-testid="token-total"');
    expect(tail).toContain('data-testid="token-total-chip"');
    expect(tail).toContain('累计 <b');
    // 旧的「N tokens」口径（读数在后面加复数 s）不得残留；类名 tb-tokens 不算。
    expect(tail).not.toMatch(/\}\}\s*tokens/);
  });

  it('分项 token 读数接线（原型 L1402-1403/L1406）：模型打分/打招呼 + chip 全量 title + 1360–1599 隐藏分项', () => {
    const tail = read('frontend-vue/src/components/TopbarTail.vue');
    expect(tail).toContain('模型打分 <b>{{ tokenText(tokensScore) }}</b>');
    expect(tail).toContain('打招呼 <b>{{ tokenText(tokensGreet) }}</b>');
    // 折叠 chip 的 title 给全量三值（原型 L1406 逐字格式）。
    expect(tail).toContain('token · 模型打分');
    expect(tail).toContain('· 打招呼');
    const css = readFileSync(join(ROOT, 'frontend-vue', 'styles', 'vue-dashboard.css'), 'utf8');
    expect(css).toContain('.tb-tokens .rail-stat');
    expect(css).toContain('@media (min-width: 1360px) and (max-width: 1599px)');
  });

  it('主题开关全站恰好一处（顶栏合并后壳不再自带）', () => {
    const holders = SRC.filter(([, text]) => text.includes('data-testid="theme-toggle"'));
    expect(holders).toHaveLength(1);
    expect(holders[0]?.[0]).toBe('frontend-vue/src/components/TopbarTail.vue');
  });
});

describe('Liquid Glass L3 透镜锚点（SPEC-003 裁定 3/5）', () => {
  function readTree(dir: string): string {
    const out: string[] = [];
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) out.push(readTree(p));
      else if (/\.(vue|ts)$/.test(e.name)) out.push(readFileSync(p, 'utf8'));
    }
    return out.join('\n');
  }

  it('glass-l3-canvas 锚点在 GlassCanvas.vue 且全站唯一', () => {
    const canvas = read('frontend-vue/src/components/GlassCanvas.vue');
    expect(canvas).toContain('data-testid="glass-l3-canvas"');
    const src = readTree(join(ROOT, 'frontend-vue', 'src'));
    const hits = src.split('data-testid="glass-l3-canvas"').length - 1;
    expect(hits).toBe(1);
  });

  it('透镜画布不吞事件（pointer-events:none 在 vue-glass.css）', () => {
    const css = readFileSync(join(ROOT, 'frontend-vue', 'styles', 'vue-glass.css'), 'utf8');
    expect(css).toContain('.glass-l3-canvas');
    expect(css).toMatch(/\.glass-l3-canvas\s*\{[^}]*pointer-events:\s*none/);
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
