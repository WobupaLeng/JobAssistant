/**
 * QA 回归（缺陷 #2 中）：限额输入框必须真的落到后端。
 *
 * 历史缺陷：`ScrapeBar` 的「最多 JD」「最低分」两个输入框只改了本地 ref，
 * 从没调用 `set_limit`——用户以为改了门槛，实际抓取仍按旧值跑。
 * C11 还有一条：这两个值**只能**走 `set_limit`，`min_score` / `max_jd`
 * 都不在 `set_app_config` 的 `WRITABLE_KEYS` 里，塞进 patch 会被静默跳过。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { MAX_JD_CEILING } from '../src/lib_ui/scrape-panel-model';

const SRC = join(dirname(fileURLToPath(import.meta.url)), '..', 'src');

function read(rel: string): string {
  return readFileSync(join(SRC, rel), 'utf8');
}

/** 与 DashboardPage 里 `onMaxJd` 完全同口径的夹值（纯函数复核）。 */
function clampMaxJd(v: number): number {
  return Math.min(MAX_JD_CEILING, Math.max(1, Math.trunc(v)));
}

/** 与 `onMinScore` 同口径：夹到满分与 0 之间。 */
function clampMinScore(v: number, scoreMax: number): number {
  return Math.min(scoreMax, Math.max(0, Math.trunc(v)));
}

describe('限额夹值区间（后端 MAX_JD_CAP / 满分）', () => {
  it('max_jd 夹在 [1, 500]', () => {
    expect(clampMaxJd(0)).toBe(1);
    expect(clampMaxJd(-9)).toBe(1);
    expect(clampMaxJd(3)).toBe(3);
    expect(clampMaxJd(9999)).toBe(MAX_JD_CEILING);
    expect(clampMaxJd(12.7)).toBe(12);
  });

  it('min_score 夹在 [0, 满分]（满分来自 get_scoring_spec，不写字面量）', () => {
    const max = 100; // 测试里当参数给，代码里来自 get_scoring_spec
    expect(clampMinScore(-1, max)).toBe(0);
    expect(clampMinScore(70, max)).toBe(70);
    expect(clampMinScore(120, max)).toBe(max);
    expect(clampMinScore(70, 85)).toBe(70);
    expect(clampMinScore(90, 85)).toBe(85);
  });
});

describe('DashboardPage 的接线', () => {
  const page = read('pages/DashboardPage.vue');

  it('两个输入框的处理函数都调用 set_limit，并回写后端返回值', () => {
    expect(page).toContain('await setLimit(next, null);');
    expect(page).toContain('await setLimit(null, next);');
    expect(page).toContain('maxJd.value = reply.max_jd;');
    expect(page).toContain('minScore.value = reply.min_score;');
  });

  it('模板把两个 update 事件接到处理函数，而不是直接赋值 ref', () => {
    expect(page).toContain('@update:max-jd="onMaxJd"');
    expect(page).toContain('@update:min-score="onMinScore"');
    expect(page).not.toContain('@update:max-jd="maxJd = $event"');
    expect(page).not.toContain('@update:min-score="minScore = $event"');
  });

  it('满分未加载时拒绝写门槛（不猜上限）', () => {
    expect(page).toContain('打分口径还没加载完，稍后再试');
  });

  it('min_score / max_jd 从不进 set_app_config 的 patch（C11）', () => {
    // 打分方式 / 初筛线走 setAppConfig 是合法的（两者都在 WRITABLE_KEYS 里），
    // 但门槛与上限那两把锁只能走 set_limit——任何 patch 里都不许出现它们。
    const patches = [...page.matchAll(/setAppConfig\(\{([^}]*)\}\)/g)].map((m) => m[1]);
    expect(patches.length).toBeGreaterThan(0);
    for (const body of patches) {
      expect(body).not.toContain('min_score');
      expect(body).not.toContain('max_jd');
    }
    expect(page).not.toContain('min_score:');
  });
});
