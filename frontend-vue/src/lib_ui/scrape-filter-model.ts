/**
 * 筛选草稿：双写 + 只提交 diff。权威是后端 `scrape_defaults`，`localStorage` 只作兜底；
 * 整份回传会把后端 `null` 语义当成「删除该键」；400ms 防抖在调用方。**搜索关键词不在草稿里**：
 * `exclude` 是排除词，搜索词直接进 `scrape_start` 的 `keywords`，混成一条会发错。
 */
import type { ScrapeDefaultsArgs } from '../api/types';
import { splitList } from './text';
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

/**
 * 搜索关键词的本地存储键。关键词进 `scrape_start.keywords`，**不进** 后端
 * `scrape_defaults`（那是筛选条件），所以只能由界面自己落盘，否则 F5 一次就清了。
 */
export const KEYWORDS_KEY = 'aja.scrape_keywords';

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
 * 单行文本 → 草稿。**任何脏数据都不能抛**：未知键丢弃，非法值回落默认。
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
 * 多选开关：已在选里就移除，否则追加，然后**按表顺序**重排——
 * `diffArgs` 用位置逐个比对，顺序不稳会产生假 diff。
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

/**
 * 读取用的键：写入走驼峰 `name`，读回是蛇形（后端按 `scrape_defaults.<snake>` 拼出）。
 * 多词键两种写法不同，单字键恰好相同会掩盖这个缺陷。本表与 `BACKEND_KEYS` 一一对应。
 */
export const BACKEND_READ_KEYS: Readonly<Record<keyof ScrapeDefaultsArgs, string>> = {
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
  jdDegree: 'jd_degree',
  minYears: 'min_years',
};

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
 * 只发变化的键。`known` 是上次从后端读到的值，首次传 `{}`；返回 `{}` 时**不要发**
 * `set_scrape_defaults`。`founded` 为空时该键不出现——`null` 会被当成「删除该键」。
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

/** 后端 `scrape_defaults` → 草稿，返回**采纳了多少个字段**：0 表示无可用值，应回落本地草稿。 */
export function applyBackend(d: FilterDraft, backend: unknown): number {
  if (backend === null || typeof backend !== 'object' || Array.isArray(backend)) return 0;
  const src = backend as Record<string, unknown>;
  let n = 0;
  for (const { field, name } of BACKEND_KEYS) {
    // 回显是蛇形键（见 BACKEND_READ_KEYS 的注释）；驼峰名只用于写入。
    // 单字键两种写法相同，所以这里必须走映射表，不能想当然用 name。
    const raw = src[BACKEND_READ_KEYS[name]];
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

// ---------------------------------------------------------------- 搜索关键词

/** 关键词输入框的提示（`filter.rs` 的 KeywordBox 逐字）。 */
export const KW_PLACEHOLDER = '输入后回车添加（可空格/逗号分隔多个）';

/** 关键词重复时的提示（`filter.rs` commit() 的 fail 文案）。 */
export const KW_DUPLICATE_NOTE = '这个关键词已经在列表里了';

/** 关键词为空时的提示（`panel.rs` on_start 的 fail 文案）。 */
export const KW_REQUIRED_NOTE = '请先填关键词，多个关键词用逗号或顿号分隔';

/**
 * 把一段自由文本并进关键词列表（切开、trim、去空、保留原顺序去重），
 * 并报告追加了几个——界面靠 `added.length === 0` 判断要不要提示重复。
 */
export function mergeKeywordList(
  current: readonly string[],
  incoming: string,
): { list: string[]; added: string[] } {
  const list = current.slice();
  const added: string[] = [];
  for (const kw of splitList(incoming)) {
    if (list.includes(kw)) continue;
    list.push(kw);
    added.push(kw);
  }
  return { list, added };
}

/** 关键词整理：去空、去重、保留顺序。界面上的任何入口都先过这一层。 */
export function normalizeKeywords(raw: readonly string[]): string[] {
  return mergeKeywordList([], raw.join(',')).list;
}

/**
 * `scrape_start` 的 `keywords` 载荷，**只来自搜索关键词列表**：
 * 严禁读 `draft.exclude`，排除词是「命中即否决」的过滤条件。
 */
export function searchKeywords(keywords: readonly string[]): string[] {
  return normalizeKeywords(keywords);
}

/** 关键词落盘：JSON 数组，读回时统一过 `normalizeKeywords`。 */
export function encodeKeywords(list: readonly string[]): string {
  return JSON.stringify(normalizeKeywords(list));
}

/** 读本地关键词。坏数据 / 旧版逗号串都尽量救回来，救不回给空表。 */
export function decodeKeywords(raw: string | null | undefined): string[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return normalizeKeywords(parsed.filter((v): v is string => typeof v === 'string'));
    }
    if (typeof parsed === 'string') return normalizeKeywords([parsed]);
    return []; // 是合法 JSON 但形状不对：不拿它去猜逗号
  } catch {
    /* 不是 JSON：按旧版逗号串读 */
  }
  return normalizeKeywords(raw.split(/[,，\n]/));
}
