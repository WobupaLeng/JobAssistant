/**
 * 筛选草稿的纯逻辑（M1 新增）。
 *
 * 每条用例对应 Leptos `features/scrape/filter.rs` 里的一个 `#[test]`，
 * 加上 C12 的两条铁律：只发 diff、本地只作兜底。
 */
import { describe, expect, it } from 'vitest';

import {
  BACKEND_KEYS,
  BACKEND_READ_KEYS,
  DRAFT_KEY,
  KEYWORDS_KEY,
  SAVE_DEBOUNCE_MS,
  activeCount,
  applyBackend,
  createDraft,
  decode,
  decodeKeywords,
  diffArgs,
  draftValues,
  encode,
  encodeKeywords,
  toggleCode,
  type FilterDraft,
} from '../src/lib_ui/scrape-filter-model';
import { loadDraft, loadKeywords, saveDraft, saveKeywords } from '../src/features/scrape/filter';
import {
  DEGREE_OPTIONS,
  JOB_TYPE_OPTIONS,
  MAX_PAGES_DEFAULT,
  MAX_PAGES_MAX,
  SCALE_OPTIONS,
} from '../src/lib_ui/options';

function withDraft(part: Partial<FilterDraft>): FilterDraft {
  return { ...createDraft(), ...part };
}

describe('encode / decode', () => {
  it('往返不丢字段', () => {
    const d = withDraft({
      city: '101020100',
      job_type: '',
      max_pages: 7,
      salaries: ['sal:10-20', 'sal:20-30'],
      salary_low: '10',
      salary_high: '25',
      experiences: ['103', '105'],
      degrees: ['203'],
      scales: ['303'],
      stages: ['805'],
      industries: ['100020', '100026'],
      exclude: '外包\n驻场\\测试',
      hr_active: '3',
      weekend: 'double',
      founded: '5',
    });
    // 薪资按 `keep_known` 语义落表，表里没有的自定义区间排在其后；断言时按集合比较。
    const back = decode(encode(d));
    expect({ ...back, salaries: [...back.salaries].sort() }).toEqual({
      ...d,
      salaries: [...d.salaries].sort(),
    });
  });

  it('脏数据一律回落默认，不抛', () => {
    const d = decode(
      [
        'city=999999',
        'job_type=abcd',
        'max_pages=999',
        'hr_active=oops',
        'weekend=oops',
        'garbage-line-without-equals',
        'unknown_key=1',
      ].join('\n'),
    );
    expect(d.city).toBe(createDraft().city);
    expect(d.job_type).toBe(createDraft().job_type);
    expect(d.max_pages).toBe(MAX_PAGES_MAX);
    expect(d.hr_active).toBe('any');
    expect(d.weekend).toBe('any');
  });

  it('未知代码丢弃，并按选项表顺序重排', () => {
    expect(decode('degrees=204,209,999').degrees).toEqual(['204', '209']);
    // 表顺序里 202 在 204 之前，输入反着写也要正过来
    expect(decode('degrees=204,202').degrees).toEqual(['202', '204']);
  });

  it('空串输入给出默认草稿', () => {
    expect(decode('')).toEqual(createDraft());
  });

  it('多行排除词经转义后仍是同一个值', () => {
    const raw = encode(withDraft({ exclude: 'a\nb\\c' }));
    expect(decode(raw).exclude).toBe('a\nb\\c');
  });
});

describe('toggleCode', () => {
  it('选中后按表顺序插入，再点取消', () => {
    let sel = toggleCode([], '204', DEGREE_OPTIONS);
    sel = toggleCode(sel, '202', DEGREE_OPTIONS);
    expect(sel).toEqual(['202', '204']);
    expect(toggleCode(sel, '202', DEGREE_OPTIONS)).toEqual(['204']);
  });

  it('未知代码不会被加进来', () => {
    expect(toggleCode([], '999', DEGREE_OPTIONS)).toEqual([]);
  });
});

describe('BACKEND_KEYS（C3）', () => {
  it('14 条映射，驼峰里没有下划线', () => {
    expect(BACKEND_KEYS).toHaveLength(14);
    for (const { name } of BACKEND_KEYS) expect(String(name)).not.toContain('_');
  });
});

describe('diffArgs（C12：只发变化）', () => {
  it('known 为空时把 13 个键全发（minYears 空值不发）', () => {
    const diff = diffArgs({}, createDraft());
    expect(Object.keys(diff)).toHaveLength(13);
    expect('minYears' in diff).toBe(false);
  });

  it('一模一样的已知值 → 空对象，一个 IPC 都不该发', () => {
    const d = createDraft();
    expect(diffArgs(draftValues(d) as never, d)).toEqual({});
  });

  it('只改城市 → 只有一个键', () => {
    const d = createDraft();
    const diff = diffArgs(draftValues(d) as never, { ...d, city: '101020100' });
    expect(Object.keys(diff)).toEqual(['city']);
    expect(diff.city).toBe('101020100');
  });

  it('空 founded 永远不进 diff（null 会清掉用户已有值）', () => {
    const known = { minYears: '3' } as never;
    expect('minYears' in diffArgs(known, createDraft())).toBe(false);
    expect(diffArgs(known, withDraft({ founded: '5' })).minYears).toBe('5');
  });

  it('maxPages 数字与字符串视为相同', () => {
    const known = { maxPages: '3' } as never;
    expect('maxPages' in diffArgs(known, createDraft())).toBe(false);
  });
});

describe('applyBackend（后端权威优先，C12）', () => {
  it('null / 空对象返回 0，调用方据此回落本地草稿', () => {
    const d = createDraft();
    expect(applyBackend(d, null)).toBe(0);
    expect(applyBackend(d, {})).toBe(0);
    expect(applyBackend(d, 42)).toBe(0);
  });

  it('合法字段被采纳，脏字段被挡下', () => {
    const d = createDraft();
    // `get_app_config.scrape_defaults` 回显的是**蛇形**键（collect_scrape_defaults
    // 去掉 `scrape_defaults.` 前缀后的相对键，见 app_config.rs:310 / KEY_SCRAPE_*）。
    // 写入走驼峰，读取走蛇形——这里刻意用蛇形，否则就等于没测到回读。
    const n = applyBackend(d, {
      city: '101020100',
      job_type: '',
      max_pages: 7,
      degree: ['202', '999'],
      hr_active: 'oops',
      min_years: '5',
    });
    expect(d.city).toBe('101020100');
    expect(d.job_type).toBe('');
    expect(d.max_pages).toBe(7);
    expect(d.degrees).toEqual(['202']);
    expect(d.hr_active).toBe('any');
    expect(d.founded).toBe('5');
    // city / job_type / max_pages / degree / min_years 五个被采纳
    expect(n).toBe(5);
  });

  it('数字型字符串也能被接受（旧数据写成 "10"）', () => {
    const d = createDraft();
    expect(applyBackend(d, { max_pages: '10' })).toBe(1);
    expect(d.max_pages).toBe(10);
  });

  // 回归：多词键曾经按驼峰名读（src[name]），而回显是蛇形 → 重启后全部回读不中。
  // 单字键（city/salary/experience/degree/industry）两种写法相同，把缺陷掩盖了很久。
  it('蛇形回显 → 9 个多词键全部命中（重启后筛选条件不丢）', () => {
    const d = createDraft();
    const n = applyBackend(d, {
      company_scale: ['303'],
      finance_stage: ['806'],
      job_type: '1902',
      max_pages: 8,
      exclude_keywords: '外包,驻场',
      hr_active: '3',
      work_weekend: 'double',
      min_years: '4',
      short_term_title: 'off',
    });
    expect(d.scales).toEqual(['303']);
    expect(d.stages).toEqual(['806']);
    expect(d.job_type).toBe('1902');
    expect(d.max_pages).toBe(8);
    expect(d.exclude).toBe('外包,驻场');
    expect(d.hr_active).toBe('3');
    expect(d.weekend).toBe('double');
    expect(d.founded).toBe('4');
    expect(d.short_term).toBe('off');
    expect(n).toBe(9);
  });

  it('短期项目开关：只认 on/off，脏值回落默认开', () => {
    expect(decode('short_term=off').short_term).toBe('off');
    expect(decode('short_term=oops').short_term).toBe('on');
    const d = createDraft();
    expect(applyBackend(d, { short_term_title: 'off' })).toBe(1);
    expect(d.short_term).toBe('off');
    expect(activeCount(d)).toBe(1);
  });

  // SPEC-004：single 档由 WEEKEND_OPTIONS 选项表驱动校验（decode L266 / applyBackend L546），
  // 无需改校验代码；这里证明新字面量经蛇形回读与本地解码两条路都被接受，不静默回落 any。
  it('single 档经选项表驱动被 decode/applyBackend 接受（SPEC-004 字面量同源）', () => {
    expect(decode('weekend=single').weekend).toBe('single');
    const d = createDraft();
    expect(applyBackend(d, { work_weekend: 'single' })).toBe(1);
    expect(d.weekend).toBe('single');
  });

  it('驼峰回显不再被当作有效输入（写入/读取两个方向不混用）', () => {
    const d = createDraft();
    // 驼峰是 set_scrape_defaults 的入参名，不是 get_app_config 的回显名。
    expect(applyBackend(d, { companyScale: ['303'], maxPages: 8, minYears: '4' })).toBe(0);
    expect(d.scales).toEqual([]);
    expect(d.max_pages).toBe(3);
    expect(d.founded).toBe('');
  });

  it('BACKEND_READ_KEYS 与 BACKEND_KEYS 一一对应，且多词键与后端 KEY_SCRAPE_* 逐字对齐', () => {
    expect(Object.keys(BACKEND_READ_KEYS).sort()).toEqual(BACKEND_KEYS.map((k) => k.name).sort());
    // 与 backend-tauri/src/core/domain/app_config.rs 的 14 条 `scrape_defaults.<key>` 常量逐字一致。
    expect(BACKEND_READ_KEYS).toEqual({
      city: 'city',
      salary: 'salary',
      experience: 'experience',
      degree: 'degree',
      companyScale: 'company_scale',
      financeStage: 'finance_stage',
      industry: 'industry',
      jobType: 'job_type',
      maxPages: 'max_pages',
      excludeKeywords: 'exclude_keywords',
      hrActive: 'hr_active',
      workWeekend: 'work_weekend',
      minYears: 'min_years',
      shortTermTitle: 'short_term_title',
    });
  });

  it('草稿里缺的字段保留默认值，不被清空', () => {
    const d = withDraft({ scales: ['303'] });
    applyBackend(d, { city: '101020100' });
    expect(d.scales).toEqual(['303']);
  });
});

describe('本地兜底存储', () => {
  it('存储键与防抖窗口是契约常量', () => {
    expect(DRAFT_KEY).toBe('aja.scrape_filter_draft');
    expect(SAVE_DEBOUNCE_MS).toBe(400);
  });

  it('node 环境没有 localStorage 时静默降级', () => {
    expect(() => saveDraft(createDraft())).not.toThrow();
    expect(loadDraft()).toBeNull();
  });

  it('有 localStorage 时能写能读', () => {
    const store = new Map<string, string>();
    const fake = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
    (globalThis as { localStorage?: unknown }).localStorage = fake;
    try {
      const d = withDraft({ city: '101020100', max_pages: 6 });
      saveDraft(d);
      expect(store.get(DRAFT_KEY)).toBeTruthy();
      expect(loadDraft()).toEqual(d);
    } finally {
      delete (globalThis as { localStorage?: unknown }).localStorage;
    }
  });
});

describe('搜索关键词本地持久化（F5 不丢）', () => {
  it('encode / decode 往返：去空、去重、保留顺序', () => {
    const raw = encodeKeywords(['前端', ' 前端 ', '', 'Go']);
    expect(raw).toBe('["前端","Go"]');
    expect(decodeKeywords(raw)).toEqual(['前端', 'Go']);
  });

  it('坏数据 / 旧版逗号串都能救回来', () => {
    expect(decodeKeywords(null)).toEqual([]);
    expect(decodeKeywords('')).toEqual([]);
    expect(decodeKeywords('{"a":1}')).toEqual([]);
    expect(decodeKeywords('前端, Go，测试')).toEqual(['前端', 'Go', '测试']);
    expect(decodeKeywords('["前端", 3]')).toEqual(['前端']);
  });

  it('有 localStorage 时写 KEYWORDS_KEY，读回来是同一份', () => {
    const store = new Map<string, string>();
    const fake = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
    (globalThis as { localStorage?: unknown }).localStorage = fake;
    try {
      saveKeywords(['前端', 'Go', '前端']);
      expect(store.get(KEYWORDS_KEY)).toBe('["前端","Go"]');
      expect(loadKeywords()).toEqual(['前端', 'Go']);
    } finally {
      delete (globalThis as { localStorage?: unknown }).localStorage;
    }
  });
});

describe('activeCount 摘要', () => {
  it('默认草稿为 0 项（默认值不算「已改」）', () => {
    expect(activeCount(createDraft())).toBe(0);
  });

  it('改城市 + 选两档薪资 = 3', () => {
    const d = withDraft({ city: '101020100', degrees: [], });
    d.salaries = ['sal:10-20', 'sal:20-30'];
    expect(activeCount(d)).toBe(3);
  });

  it('job_type 为空串（不限）不计入', () => {
    expect(activeCount(withDraft({ job_type: '' }))).toBe(0);
  });
});

describe('选项表自洽（C15）', () => {
  it('默认值与常量表里的首项一致', () => {
    expect(JOB_TYPE_OPTIONS.some(([code]) => code === createDraft().job_type)).toBe(true);
    expect(SCALE_OPTIONS.length).toBe(6);
    expect(MAX_PAGES_DEFAULT).toBe(3);
  });
});
