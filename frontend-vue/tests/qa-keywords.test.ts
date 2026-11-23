/**
 * QA 回归（缺陷 #1 严重 / #2 中）：搜索关键词 ≠ 排除关键词。
 *
 * 历史缺陷：`FilterPanel` 的「推荐词」与手填词都写进了 `draft.exclude`，
 * 页面再把 `draft.exclude` 切碎当 `scrape_start.keywords` 发出去——
 * 结果是**排除词被当成搜索词**，且 `DEFAULT_EXCLUDE` 那 15 个词（外包/驻场/
 * 金融…）成了默认搜索词，一个岗位都抓不回来，还完全不报错。
 *
 * 这组用例把边界钉死在三处：
 * 1. `searchKeywords()` 只吃关键词列表，永远不碰 `draft.exclude`；
 * 2. `diffArgs()` 的 14 个键里**没有** keywords（它只进 `scrape_start`）；
 * 3. 界面层的两个入口（手填、推荐 chip）都走 `mergeKeywordList()`，不写 exclude。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { DEFAULT_EXCLUDE, RECOMMEND_KEYWORDS } from '../src/lib_ui/options';
import {
  BACKEND_KEYS,
  KW_DUPLICATE_NOTE,
  KW_REQUIRED_NOTE,
  createDraft,
  diffArgs,
  draftValues,
  mergeKeywordList,
  normalizeKeywords,
  searchKeywords,
} from '../src/lib_ui/scrape-filter-model';

const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', 'src');

function read(rel: string): string {
  return readFileSync(join(SRC, rel), 'utf8');
}

describe('searchKeywords：只来自关键词列表', () => {
  it('空列表 → 空数组（不回落 DEFAULT_KEYWORDS，也不拿排除词顶替）', () => {
    expect(searchKeywords([])).toEqual([]);
  });

  it('保留顺序、去空、去重', () => {
    expect(searchKeywords([' Java ', 'Java', '', '后端开发', 'java'])).toEqual(['Java', '后端开发', 'java']);
  });

  it('一路带上分隔符的整串也能切开（手填框回车那一瞬）', () => {
    expect(normalizeKeywords(['Java, 后端开发、Node.js；全栈'])).toEqual([
      'Java',
      '后端开发',
      'Node.js',
      '全栈',
    ]);
  });

  it('DEFAULT_EXCLUDE 的 15 个排除词一个都不会出现在搜索载荷里', () => {
    const excludes = DEFAULT_EXCLUDE.split(',');
    const payload = searchKeywords(['Java']);
    for (const word of excludes) expect(payload).not.toContain(word);
    // 反向确认测试本身有效：这两个词确实在默认排除表里。
    expect(excludes).toContain('外包');
    expect(excludes).toContain('驻场');
  });
});

describe('mergeKeywordList：推荐 chip 进关键词，不进 exclude', () => {
  it('追加去重，报告新增了几个', () => {
    const first = mergeKeywordList([], 'Java');
    expect(first.list).toEqual(['Java']);
    expect(first.added).toEqual(['Java']);

    const again = mergeKeywordList(first.list, RECOMMEND_KEYWORDS[0] ?? '');
    expect(again.added).toEqual([]);
    expect(again.list).toEqual(['Java']);
  });

  it('一次填多个（空格/逗号混合）', () => {
    const { list, added } = mergeKeywordList(['Java'], '后端开发 Node.js,全栈');
    expect(list).toEqual(['Java', '后端开发', 'Node.js', '全栈']);
    expect(added).toEqual(['后端开发', 'Node.js', '全栈']);
  });

  it('重复词有专门文案，不与「必填」文案混淆', () => {
    expect(KW_DUPLICATE_NOTE).toBe('这个关键词已经在列表里了');
    expect(KW_REQUIRED_NOTE).toBe('请先填关键词，多个关键词用逗号或顿号分隔');
  });

  it('推荐词表原样进关键词列表（不经过 exclude 的 15 词默认表）', () => {
    let list: string[] = [];
    for (const kw of RECOMMEND_KEYWORDS) list = mergeKeywordList(list, kw).list;
    expect(list).toEqual([...RECOMMEND_KEYWORDS]);
    expect(list).not.toContain('外包');
  });
});

describe('关键词不进 set_scrape_defaults 的 14 键', () => {
  it('BACKEND_KEYS 里没有 keywords（排除词才走 excludeKeywords）', () => {
    const names = BACKEND_KEYS.map((k) => k.name as string);
    expect(names).toHaveLength(14);
    expect(names).toContain('excludeKeywords');
    expect(names.some((n) => n.toLowerCase().includes('keyword') && n !== 'excludeKeywords')).toBe(false);
    expect(names).not.toContain('keywords');
  });

  it('draftValues / diffArgs 的键集合都不含 keywords', () => {
    const d = createDraft();
    expect(Object.keys(draftValues(d))).not.toContain('keywords');
    expect(Object.keys(diffArgs({}, d))).not.toContain('keywords');
  });
});

describe('界面层：两个入口都写 keywords，不写 exclude', () => {
  const panel = read('components/FilterPanel.vue');
  const page = read('pages/DashboardPage.vue');

  it('FilterPanel 的 addKeywords / addRecommend 走 mergeKeywordList + emit keywords', () => {
    expect(panel).toContain("emit('update:keywords'");
    expect(panel).toContain('mergeKeywordList(props.keywords');
    // 历史缺陷的写法：把词拼进 exclude。出现即回归。
    expect(panel).not.toContain('patch({ exclude: props.draft.exclude.trim()');
  });

  it('FilterPanel 不再声明 filter-pos-input / filter-read-expect-btn 这两个错位 testid', () => {
    expect(panel).not.toContain('filter-pos-input');
    expect(panel).not.toContain('filter-read-expect-btn');
    expect(panel).toContain('data-testid="scrape-keywords"');
    // 契约别名：同一个输入框同时挂 testid 与 alias（Leptos KeywordBox 的双名口径）。
    expect(panel).toContain('data-alias="scrape-filter-kw-input"');
    expect(panel).toContain('data-testid="scrape-filter-kw-add-btn"');
    expect(panel).toContain('data-testid="scrape-filter-expect-btn"');
  });

  it('DashboardPage 发 scrape_start 时 keywords 来自 searchKeywords(keywords)，不切 exclude', () => {
    expect(page).toContain('const kws = searchKeywords(keywords.value);');
    expect(page).toContain('keywords: kws,');
    expect(page).not.toContain('draft.value.exclude');
    expect(page).not.toContain('已入库 0 条');
  });
});
