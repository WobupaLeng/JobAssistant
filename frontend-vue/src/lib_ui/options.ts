/**
 * 抓取筛选的选项常量。code 是 Boss 直聘的编码，一个值都不许自己编、不许增删、不许改顺序：
 * 写错只会表现为「筛不出岗位」，不会报错。改这里必须同步后端与 sidecar 的选项表；
 * 视觉口径的真源是冻结原型 `prototypes/ui-v1/index.html`。
 */

/** 选项：`[编码, 显示名]`。`code` 一律是字符串——有的是数字编码，有的形如 `sal:8-12`。 */
export type Opt = readonly [code: string, name: string];

// ---------------------------------------------------------------- 城市
// 全量城市表在 `data/cities.ts`，`CITIES` 只是 8 个热门城市：城市面板先展示热门，
// 再按省级展开全量。两层数据必须互相包含，由 tests/options.test.ts 守着。
export { CITIES } from './data/cities';

/** 城市默认值（旧版 `useState('101280600')`）。 */
export const CITY_DEFAULT = '101280600';

// ---------------------------------------------------------------- 薪资（6 + 自定义）
export const SALARY_OPTIONS: readonly Opt[] = [
  ['sal:0-8', '8K以下'],
  ['sal:8-12', '8-12K'],
  ['sal:12-16', '12-16K'],
  ['sal:16-20', '16-20K'],
  ['sal:20-30', '20-30K'],
  ['sal:30-9999', '30K以上'],
];

/** 自定义薪资的上界哨兵：`high >= 9000` 显示成「{low}K以上」（旧版 `App.tsx:1678`）。 */
export const SALARY_OPEN_HIGH = 9999;

// ---------------------------------------------------------------- 经验（7）
export const EXPERIENCE_OPTIONS: readonly Opt[] = [
  ['101', '在校/应届'],
  ['102', '经验不限'],
  ['103', '1年以内'],
  ['104', '1-3年'],
  ['105', '3-5年'],
  ['106', '5-10年'],
  ['107', '10年以上'],
];

// ---------------------------------------------------------------- 学历（7）
export const DEGREE_OPTIONS: readonly Opt[] = [
  ['202', '大专'],
  ['203', '本科'],
  ['204', '硕士'],
  ['205', '博士'],
  ['206', '中专/中技'],
  ['208', '高中'],
  ['209', '初中及以下'],
];

// ---------------------------------------------------------------- 公司规模（6）
export const SCALE_OPTIONS: readonly Opt[] = [
  ['301', '0-20人'],
  ['302', '20-99人'],
  ['303', '100-499人'],
  ['304', '500-999人'],
  ['305', '1000-9999人'],
  ['306', '10000人以上'],
];

// ---------------------------------------------------------------- 融资阶段（8）
export const STAGE_OPTIONS: readonly Opt[] = [
  ['801', '未融资'],
  ['802', '天使轮'],
  ['803', 'A轮'],
  ['804', 'B轮'],
  ['805', 'C轮'],
  ['806', 'D轮及以上'],
  ['807', '已上市'],
  ['808', '不需要融资'],
];

// ---------------------------------------------------------------- 职位类型（3）
export const JOB_TYPE_OPTIONS: readonly Opt[] = [
  ['1901', '全职'],
  ['1902', '兼职'],
  ['1903', '实习'],
];

/** 职位类型默认值（旧版 `useState('1901')`）。 */
export const JOB_TYPE_DEFAULT = '1901';

// ---------------------------------------------------------------- 行业（18）
export const INDUSTRY_OPTIONS: readonly Opt[] = [
  ['100020', '互联网'],
  ['100021', '电子商务'],
  ['100022', '计算机软件'],
  ['100023', '计算机硬件'],
  ['100024', '通信/网络设备'],
  ['100025', '电子/半导体/集成电路'],
  ['100026', '人工智能'],
  ['100027', '云计算/大数据'],
  ['100028', '企业服务'],
  ['100029', '金融/银行/证券'],
  ['100030', '游戏'],
  ['100031', '医疗健康'],
  ['100032', '教育培训'],
  ['100033', '汽车/新能源'],
  ['100034', '智能制造/工业自动化'],
  ['100035', '物流/仓储'],
  ['100036', '房地产/建筑'],
  ['100037', '消费零售'],
];

// ---------------------------------------------------------------- 工作制（4）
//
// 字段名是 `value` 不是 `code`（SPEC 3.9 特意点出），这里用带名字的结构体，
// 免得调用方把 `value` 当成编码理解。
export interface ValueOpt {
  value: string;
  name: string;
  hint: string;
}

export const WEEKEND_OPTIONS: readonly ValueOpt[] = [
  { value: 'any', name: '不限', hint: '不限制休息制度' },
  {
    value: 'double',
    name: '只投双休',
    hint: '只要双休岗位；职位描述或岗位名出现单休 / 大小周 / 周六上班 / 做六休一 / 月休N天 就排除',
  },
  {
    value: 'no_bigweek',
    name: '大小周',
    hint: '找大小周+双休；排除单休/无休（未标注按双休含）',
  },
  {
    value: 'single',
    name: '单休',
    hint: '找单休+大小周+双休；只排除无休（未标注按双休含）',
  },
];

// ---------------------------------------------------------------- HR 活跃度（4 含不限）
export const HR_ACTIVE_OPTIONS: readonly ValueOpt[] = [
  { value: 'any', name: '不限', hint: '不按 HR 活跃度排除' },
  { value: '1', name: '今日活跃', hint: '只保留最近 1 天内活跃的 HR；活跃度未知的也会被排除' },
  { value: '3', name: '3天内', hint: '只保留最近 3 天内活跃的 HR；活跃度未知的也会被排除' },
  { value: '7', name: '一周内', hint: '只保留最近 7 天内活跃的 HR；活跃度未知的也会被排除' },
];

// ---------------------------------------------------------------- JD 学历要求
export const DEGREE_VETO_OPTIONS: readonly string[] = ['大专', '本科', '硕士', '博士'];

// ---------------------------------------------------------------- 平台 / 推荐词 / 排除词
export interface PlatformOpt {
  id: string;
  name: string;
  ready: boolean;
}

export const PLATFORMS: readonly PlatformOpt[] = [
  { id: 'boss', name: 'Boss直聘', ready: true },
];

/** 平台 id → 显示名。找不到时原样回显 id（旧版 `platformName`）。 */
export function platformName(id: string): string {
  const hit = PLATFORMS.find((p) => p.id === id);
  return hit ? hit.name : id;
}

export const RECOMMEND_KEYWORDS: readonly string[] = [
  'Java',
  '后端开发',
  '全栈工程师',
  'Node.js',
  '服务端开发',
  'Java开发工程师',
];

/** 排除关键词的 textarea 占位（旧版 `App.tsx:1802`）。 */
export const EXCLUDE_PLACEHOLDER =
  '外包 驻场 兼职 日薪 跨境 外贸 金融 银行 证券 区块链 Web3 985 211 硕士 博士 销售 产品经理 总监';

/** 排除关键词默认值（旧版 `DEFAULT_EXCLUDE`，`App.tsx:212`）。 */
export const DEFAULT_EXCLUDE =
  '外包,驻场,人力,兼职,跨境,金融,银行,证券,区块链,Web3,985,211,硕士,博士,销售';

/** 翻页次数的默认值与取值范围（旧版 `useState(3)`、`min={1} max={20}`）。 */
export const MAX_PAGES_DEFAULT = 3;
export const MAX_PAGES_MIN = 1;
export const MAX_PAGES_MAX = 20;

/**
 * 把选项值转成可安全放进 `data-testid` 的片段：`:` `/` `\` 空格 统一换成 `-`，
 * 例如 `sal:8-12` → `sal-8-12`。
 */
export function tidSafe(code: string): string {
  let out = '';
  for (const ch of code) {
    out += ch === ':' || ch === '/' || ch === '\\' || ch === ' ' ? '-' : ch;
  }
  return out;
}

/**
 * 薪资档位 → 显示名。常量表里查得到就用表里的，
 * 自定义档按旧版规则：`high >= 9000` 显示「{low}K以上」。
 */
export function salaryLabel(code: string): string {
  const hit = SALARY_OPTIONS.find(([c]) => c === code);
  if (hit) return hit[1];
  if (!code.startsWith('sal:')) return code;
  const rest = code.slice('sal:'.length);
  const dashAt = rest.indexOf('-');
  const low = dashAt < 0 ? rest : rest.slice(0, dashAt);
  const high = dashAt < 0 ? '0' : rest.slice(dashAt + 1);
  const h = Number(high);
  if (high.trim().length > 0 && Number.isFinite(h) && h >= 9000) return `${low}K以上`;
  return `${low}-${high}K`;
}

/** 是否为常量表之外的自定义薪资档（用于决定是否显示「×」删除）。 */
export function isCustomSalary(code: string): boolean {
  return code.startsWith('sal:') && !SALARY_OPTIONS.some(([c]) => c === code);
}

/**
 * 由「下限 / 上限」两个输入产出薪资编码：上限留空或 ≤ 下限 → 「{low}K以上」（哨兵 9999）；
 * 两个都空 → `null`。
 */
export function salaryCode(low: string, high: string): string | null {
  const lo = toU32(low);
  const hi = toU32(high);
  if (lo === 0 && hi === 0) return null;
  return hi > lo ? `sal:${lo}-${hi}` : `sal:${lo}-${SALARY_OPEN_HIGH}`;
}

/** 宽松解析非负整数：解析失败按 0（与 Rust 的 `.parse().ok().unwrap_or(0)` 一致）。 */
function toU32(raw: string): number {
  const t = raw.trim();
  if (!/^[0-9]+$/.test(t)) return 0;
  const v = Number(t);
  return Number.isFinite(v) ? v : 0;
}
