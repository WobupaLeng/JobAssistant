/**
 * `lib_ui/options.ts`：选项表项数锚点 + 薪资档三种互逆变换（C15）。
 *
 * 项数写死在测试里是**故意的**：这些 code 是 Boss 的编码，少一项只会表现为
 * 「筛不出岗位」，不会报错。任何增删都必须让这里先红。
 */
import { describe, expect, it } from 'vitest';

import {
  CITIES,
  CITY_DEFAULT,
  DEGREE_OPTIONS,
  DEFAULT_EXCLUDE,
  EXCLUDE_PLACEHOLDER,
  EXPERIENCE_OPTIONS,
  HR_ACTIVE_OPTIONS,
  INDUSTRY_OPTIONS,
  JOB_TYPE_DEFAULT,
  JOB_TYPE_OPTIONS,
  MAX_PAGES_DEFAULT,
  MAX_PAGES_MAX,
  MAX_PAGES_MIN,
  PLATFORMS,
  RECOMMEND_KEYWORDS,
  SALARY_OPEN_HIGH,
  SALARY_OPTIONS,
  SCALE_OPTIONS,
  STAGE_OPTIONS,
  WEEKEND_OPTIONS,
  isCustomSalary,
  platformName,
  salaryCode,
  salaryLabel,
  tidSafe,
} from '../src/lib_ui/options';

describe('options/项数锚点', () => {
  it('常量表项数与 Rust 侧逐表一致', () => {
    expect(CITIES).toHaveLength(8);
    expect(SALARY_OPTIONS).toHaveLength(6);
    expect(EXPERIENCE_OPTIONS).toHaveLength(7);
    expect(DEGREE_OPTIONS).toHaveLength(7);
    expect(SCALE_OPTIONS).toHaveLength(6);
    expect(STAGE_OPTIONS).toHaveLength(8);
    expect(JOB_TYPE_OPTIONS).toHaveLength(3);
    expect(INDUSTRY_OPTIONS).toHaveLength(18);
    expect(WEEKEND_OPTIONS).toHaveLength(4);
    expect(HR_ACTIVE_OPTIONS).toHaveLength(4);
    expect(PLATFORMS).toHaveLength(1);
    expect(RECOMMEND_KEYWORDS).toHaveLength(6);
  });

  it('默认值与哨兵', () => {
    expect(CITY_DEFAULT).toBe('101280600');
    expect(JOB_TYPE_DEFAULT).toBe('1901');
    expect(SALARY_OPEN_HIGH).toBe(9999);
    expect([MAX_PAGES_MIN, MAX_PAGES_DEFAULT, MAX_PAGES_MAX]).toEqual([1, 3, 20]);
    expect(DEFAULT_EXCLUDE).toContain('外包');
    expect(EXCLUDE_PLACEHOLDER).toContain('驻场');
  });

  // SPEC-004 §一：work_weekend 走 JSON 透传无编译期枚举校验，错一个字母＝筛选静默丢失。
  // 字面量集前后端逐字同源（后端 rules.rs 三档枚举），这里把全集钉死。
  it('WEEKEND_OPTIONS 字面量集逐字同源 SPEC-004', () => {
    expect(WEEKEND_OPTIONS.map((o) => o.value)).toEqual(['any', 'double', 'no_bigweek', 'single']);
  });
});

describe('options/salaryLabel 四种形态', () => {
  it('表内档位用表里的名字', () => {
    expect(salaryLabel('sal:0-8')).toBe('8K以下');
    expect(salaryLabel('sal:30-9999')).toBe('30K以上');
  });

  it('表外自定义：上界 >= 9000 → {low}K以上', () => {
    expect(salaryLabel('sal:25-9999')).toBe('25K以上');
  });

  it('表外自定义：上界正常 → {low}-{high}K', () => {
    expect(salaryLabel('sal:25-40')).toBe('25-40K');
  });

  it('不是 sal: 前缀时原样回显', () => {
    expect(salaryLabel('面议')).toBe('面议');
  });
});

describe('options/salaryCode 四种形态', () => {
  it('正常区间', () => {
    expect(salaryCode('10', '20')).toBe('sal:10-20');
  });

  it('上界留空 → 哨兵 9999', () => {
    expect(salaryCode('10', '')).toBe(`sal:10-${SALARY_OPEN_HIGH}`);
  });

  it('上界 <= 下界 → 哨兵 9999', () => {
    expect(salaryCode('20', '10')).toBe(`sal:20-${SALARY_OPEN_HIGH}`);
  });

  it('两端都空 → null', () => {
    expect(salaryCode('', '')).toBeNull();
  });

  it('表内外判定', () => {
    expect(isCustomSalary('sal:25-40')).toBe(true);
    expect(isCustomSalary('sal:8-12')).toBe(false);
    expect(isCustomSalary('面议')).toBe(false);
  });
});

describe('options/tidSafe 与 platformName', () => {
  it('冒号等字符换成 -', () => {
    expect(tidSafe('sal:8-12')).toBe('sal-8-12');
    expect(tidSafe('a/b\\c d')).toBe('a-b-c-d');
  });

  it('平台名查表，查不到回显 id', () => {
    expect(platformName('boss')).toBe('Boss直聘');
    expect(platformName('unknown')).toBe('unknown');
  });
});
