/**
 * 筛选草稿：C12 的双写 + 只提交 diff。
 *
 * 三件事必须一起看，否则一定踩坑：
 *
 * 1. **双写**：`localStorage["aja.scrape_filter_draft"]` 只作兜底。
 *    真正的权威是后端 `app_config.scrape_defaults`——它跨机器、跨重装都在。
 *    顺序是「后端先、本地后」，`applyBackend()` 返回值 0 才回落到本地草稿
 *    （Leptos `filter.rs:970-989`）。
 * 2. **只发 diff**：`diffArgs()` 只输出变了的 14 个驼峰键。整份回传会把
 *    用户没动过的项也写一遍，后端 `null` 语义是「删除该键」，等于每次保存
 *    都把未设置的项清空（`filter.rs:922-950`）。
 * 3. **400ms 防抖**：改动频繁（点芯片、拖数字），立刻存会打爆命令层；
 *    防抖放在调用方（`debounce`），这里只提供纯函数。
 *
 * 选项表全部来自 `lib_ui/options.ts`（C15：常量表只有一份）。
 */
import type { ScrapeDefaultsArgs } from '../api/types';
import {
  CITIES,
  CITY_DEFAULT,
  DEFAULT_EXCLUDE,
  DEGREE_OPTIONS,
  DEGREE_VETO_OPTIONS,
  EXPERIENCE_OPTIONS,
  HR_ACTIVE_OPTIONS,
  INDUSTRY_OPTIONS,
  JOB_TYPE_DEFAULT,
  JOB_TYPE_OPTIONS,
  MAX_PAGES_DEFAULT,
  MAX_PAGES_MAX,
  MAX_PAGES_MIN,
  SALARY_OPTIONS,
  SALARY_OPEN_HIGH,
  SCALE_OPTIONS,
  STAGE_OPTIONS,
  WEEKEND_OPTIONS,
  isCustomSalary,
  salaryCode,
  salaryLabel,
} from './options';

/** 草稿存储键。**不要改**：老版本写下的草稿要能被新版本读出来。 */
export const DRAFT_KEY = 'aja.scrape_filter_draft';

/** 防抖窗口（毫秒）。`filter.rs:56`。 */
export const SAVE_DEBOUNCE_MS = 400;

/** 代码表：用于「按选项表顺序重排」与「过滤未知代码」。 */
export type CodeTable = readonly (readonly [string, string])[];

export interface FilterDraft {
  city: string;
  /** `''` 是合法值，表示不限（后端同样接受空串）。 */
  job_type: string;
  max_pages: number;
  salaries: string[];
  salary_low: string;
  salary_high: string;
  experiences: string[];
  degrees: string[];
  scales: string[];
  stages: string[];
  industries: string[];
  exclude: string;
  hr_active: string;
  weekend: string;
  veto_degrees: string[];
  founded: string;
}

/** 默认草稿：每次调用返回全新对象，避免共享可变更的数组。 */
export function createDraft(): FilterDraft {
  return {
    city: CITY_DEFAULT,
    job_type: JOB_TYPE_DEFAULT,
    max_pages: MAX_PAGES_DEFAULT,
    salaries: [],
    salary_low: '',
    salary_high: '',
    experiences: [],
    degrees: [],
    scales: [],
    stages: [],
    industries: [],
    exclude: DEFAULT_EXCLUDE,
    hr_active: 'any',
    weekend: 'any',
    veto_degrees: [],
    founded: '',
  };
}

// ---------------------------------------------------------------- 编解码

/** 16 个键的固定顺序。手改草稿字符串时顺序无所谓，但写出去时保持稳定便于 diff。 */
const KEYS: readonly (keyof FilterDraft)[] = [
  'city',
  'job_type',
  'max_pages',
  'salaries',
  'salary_low',
  'salary_high',
  'experiences',
  'degrees',
  'scales',
  'stages',
  'industries',
  'exclude',
  'hr_active',
  'weekend',
  'veto_degrees',
  'founded',
];

/** 反斜杠与换行要转义：草稿是 `key=value` 逐行的，值里带换行会把行结构冲垮。 */
function esc(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/\n/g, '\\n');
}

function unesc(s: string): string {
  let out = '';
  for (let i = 0; i < s.length; i += 1) {
    const ch = s[i]!;
    if (ch !== '\\') {
      out += ch;
      continue;
    }
    const next = s[i + 1];
    if (next === '\\') {
      out += '\\';
      i += 1;
    } else if (next === 'n') {
      out += '\n';
      i += 1;
    } else {
      out += ch;
    }
  }
  return out;
}

function joinIds(ids: readonly string[]): string {
  return ids.join(',');
}

function splitIds(raw: string): string[] {
  return raw
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** 草稿 → 单行文本。16 行，顺序见 `KEYS`。 */
export function encode(d: FilterDraft): string {
  const value = (k: keyof FilterDraft): string => {
    const v = d[k];
    if (Array.isArray(v)) return joinIds(v);
    if (typeof v === 'number') return String(v);
    return v;
  };
  return KEYS.map((k) => `${k}=${esc(value(k))}`).join('\n');
}

/** `keep_known` 的语义：只留表里有的代码，并**按表顺序**排好（`filter.rs:398-410`）。 */
function keepKnown(sel: readonly string[], table: CodeTable): string[] {
  const known = new Set(sel);
  const out: string[] = [];
  for (const [code] of table) {
    if (known.has(code)) out.push(code);
  }
  return out;
}

/** 薪资是多选且允许自定义区间，表里没有的只要形如 `sal:a-b` 就保留。 */
function keepSalaries(sel: readonly string[]): string[] {
  const known = new Set(sel);
  const out: string[] = [];
  for (const [code] of SALARY_OPTIONS) {
    if (known.has(code)) out.push(code);
  }
  for (const code of sel) {
    if (isCustomSalary(code) && !out.includes(code)) out.push(code);
  }
  return out;
}

function clampPages(v: unknown): number {
  const n = typeof v === 'number' ? v : Number.parseInt(String(v), 10);
  if (!Number.isFinite(n)) return MAX_PAGES_DEFAULT;
  return Math.min(MAX_PAGES_MAX, Math.max(MAX_PAGES_MIN, Math.trunc(n)));
}

/**
 * 单行文本 → 草稿。
 *
 * **任何脏数据都不能抛**：草稿是本地缓存，用户可能手改过、也可能是旧版本
 * 写的。未知键丢弃，非法值回落默认（`filter.rs:452-520`）。
 */
export function decode(raw: string | null | undefined): FilterDraft {
  const d = createDraft();
  if (!raw) return d;
  for (const line of raw.split('\n')) {
    const at = line.indexOf('=');
    if (at < 0) continue;
    const key = line.slice(0, at).trim();
    const value = unesc(line.slice(at + 1));
    switch (key) {
      case 'city': {
        const code = value.trim();
        if (CITIES.some(([c]) => c === code)) d.city = code;
        break;
      }
      case 'job_type': {
        const code = value.trim();
        // 空串本身是合法值（「不限」），必须显式放行，否则会被当成脏数据丢弃。
        if (code === '' || JOB_TYPE_OPTIONS.some(([c]) => c === code)) d.job_type = code;
        break;
      }
      case 'max_pages': {
        d.max_pages = clampPages(value.trim());
        break;
      }
      case 'salaries':
        d.salaries = keepSalaries(splitIds(value));
        break;
      case 'salary_low':
        d.salary_low = value.trim();
        break;
      case 'salary_high':
        d.salary_high = value.trim();
        break;
      case 'experiences':
        d.experiences = keepKnown(splitIds(value), EXPERIENCE_OPTIONS);
        break;
      case 'degrees':
        d.degrees = keepKnown(splitIds(value), DEGREE_OPTIONS);
        break;
      case 'scales':
        d.scales = keepKnown(splitIds(value), SCALE_OPTIONS);
        break;
      case 'stages':
        d.stages = keepKnown(splitIds(value), STAGE_OPTIONS);
        break;
      case 'industries':
        d.industries = keepKnown(splitIds(value), INDUSTRY_OPTIONS);
        break;
      case 'exclude':
        d.exclude = value;
        break;
      case 'hr_active': {
        const v = value.trim();
        if (HR_ACTIVE_OPTIONS.some((o) => o.value === v)) d.hr_active = v;
        break;
      }
      case 'weekend': {
        const v = value.trim();
        if (WEEKEND_OPTIONS.some((o) => o.value === v)) d.weekend = v;
        break;
      }
      case 'veto_degrees': {
        const sel = new Set(splitIds(value));
        // 学历否决表是纯字符串数组，没有代码列，直接按表顺序过滤。
        d.veto_degrees = [...DEGREE_VETO_OPTIONS.filter((name) => sel.has(name))];
        break;
      }
      case 'founded':
        d.founded = value.trim();
        break;
      default:
        break;
    }
  }
  return d;
}

// ---------------------------------------------------------------- 选项开关

/**
 * 多选开关：已在选里就移除，否则追加，然后**按表顺序**重排。
 *
 * 重排不是洁癖：`diffArgs` 用位置逐个比对，顺序不稳会让「只改了选择」
 * 变成一串假 diff。
 */
export function toggleCode(sel: readonly string[], code: string, table: CodeTable): string[] {
  if (sel.includes(code)) return sel.filter((c) => c !== code);
  return keepKnown([...sel, code], table);
}

// ---------------------------------------------------------------- 后端往返

interface BackendKey {
  readonly field: keyof FilterDraft;
  readonly name: keyof ScrapeDefaultsArgs;
}

/**
 * 14 条映射（`filter.rs:530-560`）。蛇形→驼峰在这里一次性定死，
 * 别在别处再写一遍——C3 的静默失效就是从这种重复开始的。
 */
export const BACKEND_KEYS: readonly BackendKey[] = [
  { field: 'city', name: 'city' },
  { field: 'salaries', name: 'salary' },
  { field: 'experiences', name: 'experience' },
  { field: 'degrees', name: 'degree' },
  { field: 'scales', name: 'companyScale' },
  { field: 'stages', name: 'financeStage' },
  { field: 'industries', name: 'industry' },
  { field: 'job_type', name: 'jobType' },
  { field: 'max_pages', name: 'maxPages' },
  { field: 'exclude', name: 'excludeKeywords' },
  { field: 'hr_active', name: 'hrActive' },
  { field: 'weekend', name: 'workWeekend' },
  { field: 'veto_degrees', name: 'jdDegree' },
  { field: 'founded', name: 'minYears' },
];

/** 单个字段 → 后端 JSON 值。数组给数组，数字给数字，空 `founded` 给 `null`。 */
function backendValue(d: FilterDraft, field: keyof FilterDraft): unknown {
  switch (field) {
    case 'max_pages':
      return d.max_pages;
    case 'salaries':
      return d.salaries.slice();
    case 'experiences':
      return d.experiences.slice();
    case 'degrees':
      return d.degrees.slice();
    case 'scales':
      return d.scales.slice();
    case 'stages':
      return d.stages.slice();
    case 'industries':
      return d.industries.slice();
    case 'veto_degrees':
      return d.veto_degrees.slice();
    case 'founded':
      // 空串 → null，后端认 null 为「不限年限」。
      return d.founded.trim() === '' ? null : d.founded.trim();
    default:
      return d[field];
  }
}

/** 全量载荷：14 个驼峰键。用于「第一次保存、没有 known 可比」的场景。 */
export function draftValues(d: FilterDraft): ScrapeDefaultsArgs {
  const out: Record<string, unknown> = {};
  for (const { field, name } of BACKEND_KEYS) {
    if (field === 'founded' && d.founded.trim() === '') continue;
    out[name] = backendValue(d, field);
  }
  return out as ScrapeDefaultsArgs;
}

function sameValue(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b)) return false;
    if (a.length !== b.length) return false;
    return a.every((x, i) => x === b[i]);
  }
  if (a === null || b === null) return a === b;
  if (typeof a === 'number' || typeof b === 'number') return String(a) === String(b);
  return a === b;
}

/**
 * 只发变化的键。
 *
 * - `known` 是「界面上次从后端读到的值」；第一次打开时传 `{}`。
 * - `{}` 表示没有任何变化，**不要发** `set_scrape_defaults`（发了也是空写）。
 * - `founded` 为空时该键不出现：`null` 与「没改」在语义上无法区分，
 *   发 `null` 会清掉用户已有的年限设置。
 */
export function diffArgs(known: Partial<ScrapeDefaultsArgs>, d: FilterDraft): ScrapeDefaultsArgs {
  const out: Record<string, unknown> = {};
  for (const { field, name } of BACKEND_KEYS) {
    const next = backendValue(d, field);
    if (field === 'founded' && next === null) continue;
    if (sameValue(known[name], next)) continue;
    if (known[name] === undefined && field === 'founded' && next === null) continue;
    out[name] = next;
  }
  return out as ScrapeDefaultsArgs;
}

/** 后端值 → 字符串。数字与字符串都接受（旧数据里 `maxPages` 可能是 `"3"`）。 */
function strOf(v: unknown): string | null {
  if (typeof v === 'string') return v;
  if (typeof v === 'number' && Number.isFinite(v)) return String(v);
  return null;
}

function strList(v: unknown): string[] | null {
  if (!Array.isArray(v)) return null;
  const out: string[] = [];
  for (const item of v) {
    const s = strOf(item);
    if (s !== null) out.push(s);
  }
  return out;
}

/**
 * 后端 `scrape_defaults` → 草稿，返回**采纳了多少个字段**。
 *
 * 返回 0 说明后端没有可用值（`null` / `{}` / 全脏），调用方应回落到本地草稿。
 * 逐个字段独立采纳：后端只有一半字段时，另一半保留默认（`filter.rs:970-1040`）。
 */
export function applyBackend(d: FilterDraft, backend: unknown): number {
  if (backend === null || typeof backend !== 'object' || Array.isArray(backend)) return 0;
  const src = backend as Record<string, unknown>;
  let n = 0;
  for (const { field, name } of BACKEND_KEYS) {
    const raw = src[name as string];
    if (raw === undefined || raw === null) continue;
    switch (field) {
      case 'city': {
        const v = strOf(raw);
        if (v !== null && CITIES.some(([c]) => c === v)) {
          d.city = v;
          n += 1;
        }
        break;
      }
      case 'job_type': {
        const v = strOf(raw);
        if (v !== null && (v === '' || JOB_TYPE_OPTIONS.some(([c]) => c === v))) {
          d.job_type = v;
          n += 1;
        }
        break;
      }
      case 'max_pages': {
        const v = strOf(raw);
        if (v !== null && Number.isFinite(Number(v))) {
          d.max_pages = clampPages(v);
          n += 1;
        }
        break;
      }
      case 'salaries': {
        const list = strList(raw);
        if (list !== null) {
          d.salaries = keepSalaries(list);
          n += 1;
        }
        break;
      }
      case 'experiences': {
        const list = strList(raw);
        if (list !== null) {
          d.experiences = keepKnown(list, EXPERIENCE_OPTIONS);
          n += 1;
        }
        break;
      }
      case 'degrees': {
        const list = strList(raw);
        if (list !== null) {
          d.degrees = keepKnown(list, DEGREE_OPTIONS);
          n += 1;
        }
        break;
      }
      case 'scales': {
        const list = strList(raw);
        if (list !== null) {
          d.scales = keepKnown(list, SCALE_OPTIONS);
          n += 1;
        }
        break;
      }
      case 'stages': {
        const list = strList(raw);
        if (list !== null) {
          d.stages = keepKnown(list, STAGE_OPTIONS);
          n += 1;
        }
        break;
      }
      case 'industries': {
        const list = strList(raw);
        if (list !== null) {
          d.industries = list.filter((c) => c.trim() !== '');
          n += 1;
        }
        break;
      }
      case 'veto_degrees': {
        const list = strList(raw);
        if (list !== null) {
          const sel = new Set(list);
          d.veto_degrees = [...DEGREE_VETO_OPTIONS.filter((name) => sel.has(name))];
          n += 1;
        }
        break;
      }
      case 'hr_active': {
        const v = strOf(raw);
        if (v !== null && HR_ACTIVE_OPTIONS.some((o) => o.value === v)) {
          d.hr_active = v;
          n += 1;
        }
        break;
      }
      case 'weekend': {
        const v = strOf(raw);
        if (v !== null && WEEKEND_OPTIONS.some((o) => o.value === v)) {
          d.weekend = v;
          n += 1;
        }
        break;
      }
      case 'exclude': {
        const v = strOf(raw);
        if (v !== null) {
          d.exclude = v;
          n += 1;
        }
        break;
      }
      case 'founded': {
        const v = strOf(raw);
        if (v !== null) {
          d.founded = v.trim();
          n += 1;
        }
        break;
      }
      default:
        break;
    }
  }
  return n;
}

// ---------------------------------------------------------------- 展示

/** 摘要文案（面板角标）：几项生效。「不限」的项不计入。 */
export function activeCount(d: FilterDraft): number {
  let n = 0;
  if (d.city !== CITY_DEFAULT) n += 1;
  if (d.job_type !== JOB_TYPE_DEFAULT && d.job_type !== '') n += 1;
  if (d.max_pages !== MAX_PAGES_DEFAULT) n += 1;
  n += d.salaries.length;
  n += d.experiences.length;
  n += d.degrees.length;
  n += d.scales.length;
  n += d.stages.length;
  n += d.industries.length;
  if (d.hr_active !== 'any') n += 1;
  if (d.weekend !== 'any') n += 1;
  n += d.veto_degrees.length;
  if (d.founded.trim() !== '') n += 1;
  if (d.exclude.trim() !== DEFAULT_EXCLUDE && d.exclude.trim() !== '') n += 1;
  return n;
}

/** 薪资区间串（两个输入框的回显）。两边都空时不显示。 */
export function customSalaryText(low: string, high: string): string {
  const code = salaryCode(low, high);
  return code === null ? '' : salaryLabel(code);
}

/** 薪资上限哨兵值：与 SALARY_OPEN_HIGH 同源，别另写 9999。 */
export const SALARY_OPEN = SALARY_OPEN_HIGH;
