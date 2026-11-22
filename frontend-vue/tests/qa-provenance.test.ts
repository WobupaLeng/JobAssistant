/**
 * QA 回归（缺陷 #5 低）：源码注释里的溯源坐标与选项常量文案。
 *
 * 历史缺陷：
 * - `lib_ui/options.ts` 注释把常量表真源指到 `docs/ui/UI-SPEC-web-align.md`，
 *   该文件**不存在**（真源是冻结原型 + 本仓库的选项表）；
 * - 平台名写「前程无忧」，原型写的是「51job」——按名字找到的元素会落空。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { PLATFORMS, platformName } from '../src/lib_ui/options';
import { platformCards } from '../src/lib_ui/platform-model';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

function read(rel: string): string {
  return readFileSync(join(ROOT, rel), 'utf8');
}

describe('选项表注释不再指向不存在的规格文件', () => {
  const options = read('frontend-vue/src/lib_ui/options.ts');

  it('不出现 UI-SPEC-web-align.md', () => {
    expect(options).not.toContain('UI-SPEC-web-align.md');
  });

  it('指向真实存在的真源（冻结原型）', () => {
    expect(options).toContain('prototypes/ui-v1/index.html');
  });
});

describe('平台名与原型逐字一致（原型 1379 用「51job」）', () => {
  it('PLATFORMS 三张卡的展示名', () => {
    expect(PLATFORMS.map((p) => p.name)).toEqual(['Boss直聘', '智联招聘', '51job']);
    expect(platformName('51job')).toBe('51job');
  });

  it('「前程无忧」这个别名不再出现在选项表里', () => {
    expect(read('frontend-vue/src/lib_ui/options.ts')).not.toContain('前程无忧');
  });

  it('platformCards() 的卡片视图沿用同一批名字', () => {
    expect(platformCards('boss', false).map((c) => c.name)).toEqual(['Boss直聘', '智联招聘', '51job']);
  });
});
