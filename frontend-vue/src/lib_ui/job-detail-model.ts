/**
 * 岗位详情抽屉的纯视图模型。
 *
 * 真源是 Leptos `features/jobs/drawer.rs`（字段口径与逐字文案）+ 冻结原型
 * `prototypes/ui-v1/index.html` 的 `job-detail-*` 骨架（可测锚点）。
 * 两边冲突时**以原型为准的只有外壳**（testid / 头部 / 底部动作条），
 * 正文内容仍按 .rs：原型正文是占位串，.rs 才是真实字段。
 *
 * 这里只做"该显示什么"，不碰 DOM、不发命令。
 */
import type { Job } from '../api/types';
import type { ScoreTone } from './job';
import { scoreTone } from './job';
import { fmtTime } from './time';

/** 空字段统一破折号：空串会让抽屉出现"空洞"，看起来像加载失败。 */
export const DASH = '—';

/** 取非空值，否则破折号。 */
export function dash(s: string | null | undefined): string {
  const v = (s ?? '').trim();
  return v === '' ? DASH : v;
}

/** 抽屉标题：空标题补「（无标题）」（`drawer.rs:157-161`）。 */
export function detailTitle(job: Job): string {
  const t = job.title.trim();
  return t === '' ? '（无标题）' : t;
}

/** 头部公司行。 */
export function detailCompany(job: Job): string {
  return dash(job.company);
}

/** 头部两枚 chip：薪资 / 城市。 */
export function detailHeadChips(job: Job): string[] {
  return [dash(job.salary), dash(job.location)];
}

/**
 * 分数位：未打分（0）显示破折号，否则原样数字。
 * **不夹取、不换算**——满分只用于旁边那行 `/ N 分`。
 */
export function detailScoreText(score: number): string {
  return score === 0 ? DASH : String(score);
}

/** `/ N 分`；满分未知时 `/ — 分`（C10：满分只能来自 `get_scoring_spec`）。 */
export function detailScoreMaxText(scoreMax: number | null | undefined): string {
  const m = typeof scoreMax === 'number' && scoreMax > 0 ? scoreMax : null;
  return m === null ? '/ — 分' : `/ ${m} 分`;
}

/** 分数色阶：与列表卡同一套（`scoreTone`），门槛来自 `get_scoring_spec`。 */
export function detailScoreTone(score: number, minScore: number | null): ScoreTone {
  return scoreTone(score, minScore);
}

/** 分数格类名：`score-num drawer-score is-<tone>`。 */
export function detailScoreClass(score: number, minScore: number | null): string {
  return `score-num drawer-score is-${detailScoreTone(score, minScore)}`;
}

/**
 * 是否达门槛：只有「门槛已知**且**已打分」才给结论，否则 `null`（不渲染这枚 chip）。
 * 宁可不说，也不说错（`drawer.rs:102-111`）。
 */
export function passesThreshold(score: number, minScore: number | null): boolean | null {
  if (minScore === null) return null;
  if (score === 0) return null;
  return score >= minScore;
}

/** 第二行 chips：来源 / 达门槛 / 制式 / 平台。`null` 位不渲染。 */
export interface DetailChip {
  readonly tone: 'primary' | 'bad' | 'accent' | 'default';
  readonly text: string;
  readonly title?: string;
}

export function detailChips(
  job: Job,
  minScore: number | null,
  scoreMax: number | null,
): DetailChip[] {
  const out: DetailChip[] = [];
  out.push({ tone: 'accent', text: dash(job.score_source) });
  const pass = passesThreshold(job.match_score, minScore);
  if (pass !== null && minScore !== null) {
    out.push(
      pass
        ? { tone: 'primary', text: `达到门槛 ${minScore}`, title: '分数达到当前入库门槛' }
        : { tone: 'bad', text: `低于门槛 ${minScore}`, title: '分数低于当前入库门槛' },
    );
  }
  if (typeof scoreMax === 'number' && scoreMax > 0) {
    out.push({ tone: 'default', text: `制式 ${scoreMax}` });
  }
  out.push({ tone: 'default', text: dash(job.platform) });
  return out;
}

/** 打分依据。空理由时按"读取中 / 无打分说明"降级（`drawer.rs:113-122`）。 */
export function detailReason(job: Job, jdLoading: boolean): string {
  const r = job.score_reason.trim();
  if (r !== '') return r;
  return jdLoading ? '读取中…' : '无打分说明';
}

/** JD 正文的四种降级形态（`drawer.rs:124-135`），顺序不能换。 */
export function detailJdText(job: Job, jdLoading: boolean, jdError: string): string {
  const t = job.jd_text.trim();
  if (t !== '') return job.jd_text;
  if (jdError.trim() !== '') return `（读取职位描述失败：${jdError.trim()}）`;
  if (jdLoading) return '读取中…';
  return '（该岗位没有 JD 原文，抓取时通常按「无 JD」降权处理）';
}

/** 岗位信息 kv 表的一行。 */
export interface DetailKvRow {
  readonly label: string;
  readonly value: string;
  /** 该行尾随的小 chip（HR 活跃度 / 已排除）。 */
  readonly chip?: { readonly tone: 'default' | 'bad'; readonly text: string };
}

/** 「在招」：>0 显示 `N 个岗位`，否则破折号。 */
export function hiringText(hiringCount: number): string {
  return hiringCount > 0 ? `${hiringCount} 个岗位` : DASH;
}

/** HR 列：姓名 · 职位，任意一边缺失就让位（`drawer.rs:181-185`）。 */
export function hrText(job: Job): string {
  const n = dash(job.hr_name);
  const p = dash(job.hr_position);
  if (n === DASH) return p;
  if (p === DASH) return n;
  return `${n} · ${p}`;
}

/**
 * 入库时间：优先 `updated_at`（重算会刷新它），没有再用 `created_at`；
 * 两者都 ≤0 → 破折号。C14：`fmtTime` 同时吃秒与毫秒，UTC+8。
 */
export function detailWhen(job: Job): string {
  const raw = job.updated_at > 0 ? job.updated_at : job.created_at;
  return raw > 0 ? fmtTime(raw) : DASH;
}

/** kv 表十行，顺序照 `drawer.rs:270-317`。 */
export function detailKv(job: Job): DetailKvRow[] {
  const active = (job.hr_active_status ?? '').trim();
  return [
    { label: '经验', value: dash(job.experience) },
    { label: '学历', value: dash(job.degree) },
    { label: '行业', value: dash(job.company_industry) },
    { label: '规模', value: dash(job.company_scale) },
    { label: '阶段', value: dash(job.company_stage) },
    { label: '在招', value: hiringText(job.hiring_count) },
    {
      label: 'HR',
      value: hrText(job),
      ...(active === '' ? {} : { chip: { tone: 'default' as const, text: active } }),
    },
    {
      label: '状态',
      value: dash(job.status),
      ...(job.rejected !== 0 ? { chip: { tone: 'bad' as const, text: '已排除' } } : {}),
    },
    { label: '入库', value: detailWhen(job) },
    { label: '链接', value: '' },
  ];
}

/** 外链：空则 `null`（渲染破折号）。 */
export function detailLink(job: Job): string | null {
  const u = (job.job_url ?? '').trim();
  return u === '' ? null : u;
}

/**
 * 是否需要补拉完整 JD：列表接口不返回 `jd_text`（体积太大），
 * 打开时为空就 `get_job` 补一次。**补拉失败不能把抽屉干掉**。
 */
export function needsJdFetch(job: Job): boolean {
  return job.jd_text.trim() === '';
}

// ---------------------------------------------------------------- 单条动作

/**
 * 详情抽屉底部的两个单条动作（原型 `job-detail-reject-btn` /
 * `job-detail-deliver-btn`，§16.4 裁决保留）。
 *
 * 「不投递」对已排除的岗位反向变成「允许投递」——否则抽屉里会出现一个
 * 点了没反应的按钮（Leptos 卡片上的 `mark_label` 同口径）。
 */
export type DetailAction = 'reject' | 'allow' | 'deliver';
/**
 * 追溯型动作：`undo-deliver` 把「已投递」放回待投递（`unmark_delivered`）。
 *
 * 它与 `deliver` 成对出现在「已投递」页签上，语义是撤回而不是新增一种状态，
 * 所以单独列进 `DetailAction`，让按钮与页签一一对应。
 */
export type DetailAct = DetailAction | 'undo-deliver';

export interface DetailActionMeta {
  readonly action: DetailAct;
  readonly testId: string;
  readonly label: string;
  readonly btnClass: string;
  readonly disabled: boolean;
}

/**
 * 底部两颗动作按钮：**页签决定可做哪些事**（与列表批量条同一口径
 * `job-list-model.batchOps`）。
 *
 * - 未投递（pending）：不投递 / 标记已投递
 * - 不投递（rejected）：撤销否决（不出现投递类按钮）
 * - 已投递（delivered）：取消投递 /（不出现不投递）
 *
 * 之所以要按页签裁，是因为抽屉里原本两张按钮恒在：在不投递页签点「标记已投递」
 * 会把一个被否决的岗位标成已投递，在已投递页签点「不投递」会让同一行既是
 * rejected 又是 delivered，列表与抽屉各显示一套状态。按钮跟随页签后，
 * 抽屉里能点的动作与当前列表能做的动作永远一致。
 *
 * 页签是第一判据；只有没给页签（单测直接调）时才退回看 `job.status`。
 */
export function detailActions(job: Job, tab?: string): readonly DetailActionMeta[] {
  const delivered = job.status === 'delivered';
  if (tab === 'rejected') {
    return [
      {
        action: 'allow',
        testId: 'job-detail-allow-btn',
        // 与列表批量条同口径：这一页签上是「撤销否决」回未投递，
        // 写「允许投递」会让人以为是给这个岗位再投一次。
        label: '撤销否决',
        btnClass: 'btn btn-sm btn-ghost',
        disabled: false,
      },
    ];
  }
  if (tab === 'delivered' || (tab === undefined && delivered && job.rejected === 0)) {
    return [
      {
        action: 'undo-deliver',
        testId: 'job-detail-undeliver-btn',
        label: '取消投递',
        btnClass: 'btn btn-sm btn-ghost',
        disabled: false,
      },
    ];
  }
  return [rejectAction(job), deliverAction(job)];
}

export function rejectAction(job: Job): DetailActionMeta {
  const rejected = job.rejected !== 0;
  return rejected
    ? {
        action: 'allow',
        testId: 'job-detail-allow-btn',
        label: '允许投递',
        btnClass: 'btn btn-sm btn-ghost',
        disabled: false,
      }
    : {
        action: 'reject',
        testId: 'job-detail-reject-btn',
        label: '不投递',
        btnClass: 'btn btn-sm btn-danger',
        disabled: false,
      };
}

/** 已投递则按钮禁用（重复标记没有意义，也不该再发一次 `mark_delivered`）。 */
export function deliverAction(job: Job): DetailActionMeta {
  const done = job.status === 'delivered' && job.rejected === 0;
  return {
    action: 'deliver',
    testId: 'job-detail-deliver-btn',
    label: done ? '已标记投递' : '标记已投递',
    btnClass: 'btn btn-sm btn-primary',
    disabled: done,
  };
}

/** 动作执行后回给用户的提示（逐字，与列表侧同一口径）。 */
export function actionNote(action: DetailAct, hit: number): string {
  switch (action) {
    case 'reject':
      return `已标为不投递：影响 ${hit} 条`;
    case 'allow':
      return `已恢复投递：影响 ${hit} 条`;
    case 'deliver':
      return '已标记投递';
    case 'undo-deliver':
      return `已取消投递：影响 ${hit} 条`;
  }
}

/** 动作失败前缀。 */
export function actionFailPrefix(action: DetailAct): string {
  switch (action) {
    case 'reject':
      return '标记不投递失败：';
    case 'allow':
      return '恢复投递失败：';
    case 'deliver':
      return '标记投递失败：';
    case 'undo-deliver':
      return '取消投递失败：';
  }
}

/** 抽屉开合：同一岗位再点一次不重复打开（避免重复 `get_job`）。 */
export interface DetailState {
  readonly jobId: string | null;
}

export const EMPTY_DETAIL: DetailState = { jobId: null };

export function openDetail(state: DetailState, jobId: string): DetailState {
  return state.jobId === jobId ? state : { jobId };
}

export function closeDetail(): DetailState {
  return EMPTY_DETAIL;
}

export function isOpen(state: DetailState, jobId: string): boolean {
  return state.jobId === jobId;
}
