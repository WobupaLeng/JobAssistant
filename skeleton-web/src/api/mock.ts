/**
 * 本地演示数据（骨架期专用）。
 *
 * 背景：本仓库开发态**不做 Mock 服务**（AGENTS.md 3.2）。这里的数组只在
 * `get_app_config` / `list_decisions` / `list_sensitive` 这些命令**尚未注册到后端**时，
 * 由界面按 `isCommandMissing()` 判定后兜底展示，好让版式与交互先跑起来。
 * 命令一旦在 Rust 侧落地，这些数据就不再会被读到——**不要**把它们接到业务判断上。
 */
import type { AppConfig, DecisionLog, SensitiveRow } from './types';

export const MOCK_CONFIG: AppConfig = {
  api_key: '',
  api_key_hint: 'sk-demo****7c3f',
  base_url: 'https://api.deepseek.com/v1',
  model: 'deepseek-chat',
  limit: 100,
  keywords: ['Java', 'Spring Cloud', '后端开发'],
  min_score: 70,
  use_llm: false,
  llm_prefilter: 50,
  greet_prompt:
    '你好，我是一名有 6 年经验的 Java 后端工程师，熟悉 Spring Cloud、MySQL、Redis 与高并发电商场景。\n' +
    '看到贵司在招「{岗位}」，与我的经历高度匹配，希望有机会进一步沟通。',
  skills: 'Java, Spring Boot, Spring Cloud, MySQL, Redis, RocketMQ, Docker, K8s',
  filters: {
    salary: '',
    experience: '',
    degree: '本科',
    scale: '',
    stage: '',
    industry: '',
  },
  excludes: {
    veto_words: ['外包', '驻场', '人力', '兼职', '跨境', '金融', '银行', '证券', '区块链', 'Web3', '985', '211', '硕士', '博士', '销售'],
    exclude_salary_min: null,
    exclude_salary_max: null,
    exclude_degree: ['硕士', '博士'],
    exclude_founded_below: null,
    exclude_resume_max_years: null,
    exclude_hr_active_days: 7,
    weekend: null,
  },
};

const NOW = Date.now();
const MIN = 60_000;

export const MOCK_DECISIONS: DecisionLog[] = [
  {
    id: 1041,
    created_at: NOW - 2 * MIN,
    platform: 'boss',
    job_id: 'b-88213',
    company: '某电商平台',
    title: '高级 Java 开发工程师',
    kind: 'stored',
    reason: '规则 82 分，达到门槛 70',
    score: 82,
    source: 'rule',
  },
  {
    id: 1040,
    created_at: NOW - 3 * MIN,
    platform: 'boss',
    job_id: 'b-88209',
    company: '某科技服务有限公司',
    title: 'Java 开发（外包驻场）',
    kind: 'rejected',
    reason: '命中排除词「外包」',
    score: null,
    source: 'rule',
  },
  {
    id: 1039,
    created_at: NOW - 9 * MIN,
    platform: 'boss',
    job_id: 'b-88196',
    company: '某金融科技',
    title: '后端工程师（支付方向）',
    kind: 'rejected',
    reason: '命中排除词「金融」',
    score: null,
    source: 'rule',
  },
  {
    id: 1038,
    created_at: NOW - 14 * MIN,
    platform: 'boss',
    job_id: 'b-88170',
    company: '某 SaaS 服务商',
    title: 'Java 后端开发工程师',
    kind: 'stored',
    reason: '规则 74 分，达到门槛 70',
    score: 74,
    source: 'rule',
  },
  {
    id: 1037,
    created_at: NOW - 21 * MIN,
    platform: 'boss',
    job_id: 'b-88155',
    company: '某互联网公司',
    title: '中间件研发工程师',
    kind: 'rejected',
    reason: '规则 61 分未达门槛 70：技能匹配一般，JD 偏底层',
    score: 61,
    source: 'rule',
  },
];

export const MOCK_SENSITIVE: SensitiveRow[] = [
  {
    id: 5021,
    created_at: NOW - 3 * MIN,
    platform: 'boss',
    job_id: 'b-88209',
    company: '某科技服务有限公司',
    title: 'Java 开发（外包驻场）',
    salary: '15-20K',
    kind: 'rejected',
    reason: '命中排除词「外包」',
  },
  {
    id: 5020,
    created_at: NOW - 9 * MIN,
    platform: 'boss',
    job_id: 'b-88196',
    company: '某金融科技',
    title: '后端工程师（支付方向）',
    salary: '25-40K',
    kind: 'rejected',
    reason: '命中排除词「金融」',
  },
  {
    id: 5019,
    created_at: NOW - 21 * MIN,
    platform: 'boss',
    job_id: 'b-88155',
    company: '某互联网公司',
    title: '中间件研发工程师',
    salary: '30-45K',
    kind: 'rejected',
    reason: '规则 61 分未达门槛 70',
  },
  {
    id: 5018,
    created_at: NOW - 2 * 60 * MIN,
    platform: 'boss',
    job_id: 'b-88170',
    company: '某 SaaS 服务商',
    title: 'Java 后端开发工程师',
    salary: '20-35K',
    kind: 'stored',
    reason: '已入库（抓取过，下一轮自动跳过）',
  },
  {
    id: 5017,
    created_at: NOW - 3 * 60 * MIN,
    platform: 'boss',
    job_id: 'b-88102',
    company: '某物流科技',
    title: 'Java 工程师（订单中台）',
    salary: '22-32K',
    kind: 'stored',
    reason: '已入库（抓取过，下一轮自动跳过）',
  },
];
