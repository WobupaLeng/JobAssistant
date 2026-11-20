/**
 * 配置：大模型 / 抓取 / 打分 / 打招呼 / 筛选与排除，分组成可折叠卡片。
 *
 * 约束：
 * - **API Key 只进不回**：加载时后端只给 `api_key_hint`（尾 4 位），输入框用
 *   `type=password` 且保持空，用户不填就等于「不改密钥」（AGENTS.md 2.4 安全红线）；
 * - 分数上限与门槛取自 `ScoringSpec`，本文件不写 100 / 70；
 * - 后端命令未就绪时不白屏：显示「该功能后端尚未就绪」，编辑区只读并叠一层本地演示数据。
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  api,
  isCommandMissing,
  errText,
  keyHint,
  NOT_READY_TEXT,
  type AppConfig,
  type AppConfigPatch,
  type AppExcludes,
  type AppFilters,
  type ScoringSpec,
} from '../../api';
import { MOCK_CONFIG } from '../../api/mock';
import { Card, Chip } from '../../components/ui';
import { joinList, splitList } from '../../lib/text';

type SectionKey = 'llm' | 'scrape' | 'score' | 'greet' | 'filter';

const SECTIONS: { key: SectionKey; title: string; sub: string }[] = [
  { key: 'llm', title: '大模型', sub: '打分与生成打招呼文案都用这一份' },
  { key: 'scrape', title: '抓取', sub: '关键词与每轮上限' },
  { key: 'score', title: '打分', sub: '入库门槛与大模型通道' },
  { key: 'greet', title: '打招呼', sub: '提示词与技能，生成话术时生效' },
  { key: 'filter', title: '筛选与排除', sub: '下单条件 + 硬性排除规则' },
];

const WEEKEND_OPTIONS = [
  { value: '', name: '不限' },
  { value: 'double', name: '只投双休' },
  { value: 'no_bigweek', name: '大小周' },
];

const HR_OPTIONS = [
  { value: '', name: '不限制' },
  { value: '1', name: '今日活跃' },
  { value: '3', name: '3 天内' },
  { value: '7', name: '一周内' },
];

const DEGREE_OPTIONS = ['大专', '本科', '硕士', '博士'];

/** 拒绝单值清空：`'5'` → 5；`''` → null（= 不排除）。 */
function numOrNull(v: string): number | null {
  const t = v.trim();
  if (t === '') return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

function str(num: number | null): string {
  return num === null ? '' : String(num);
}

/**
 * 算出真正需要提交的差异键。
 * 只有改动过的顶层键 / 子键才进 patch：后端按「键存在就写」处理，
 * 每次都全量提交会把用户没见过的默认值（如后端补的 `use_llm`）一并写回。
 */
function diffConfig(base: AppConfig, next: AppConfig): AppConfigPatch {
  const out: AppConfigPatch = {};
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

  const top: (keyof AppConfigPatch)[] = [
    'base_url',
    'model',
    'limit',
    'keywords',
    'min_score',
    'use_llm',
    'llm_prefilter',
    'greet_prompt',
    'skills',
  ];
  for (const k of top) {
    if (same(base[k], next[k])) continue;
    (out as Record<string, unknown>)[k] = next[k];
  }

  const filters: Partial<AppFilters> = {};
  for (const k of Object.keys(next.filters) as (keyof AppFilters)[]) {
    if (!same(base.filters[k], next.filters[k])) filters[k] = next.filters[k];
  }
  if (Object.keys(filters).length > 0) out.filters = filters;

  const excludes: Partial<AppExcludes> = {};
  for (const k of Object.keys(next.excludes) as (keyof AppExcludes)[]) {
    if (!same(base.excludes[k], next.excludes[k])) (excludes as Record<string, unknown>)[k] = next.excludes[k];
  }
  if (Object.keys(excludes).length > 0) out.excludes = excludes;

  return out;
}

export function ConfigPanel(props: { spec: ScoringSpec | null; onSaved?: () => void; fail: (e: unknown) => void }) {
  const [draft, setDraft] = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [mockMode, setMockMode] = useState(false);
  const [note, setNote] = useState('');
  // 首次加载已经用到的「未就绪」判定：命令可能因未注册而直接抛同步异常，
  // 这种情况不走 catch 分支，所以在加载结果里一并记下。
  const [loadNotReady, setLoadNotReady] = useState(false);
  const [open, setOpen] = useState<Record<SectionKey, boolean>>({
    llm: true,
    scrape: true,
    score: true,
    greet: true,
    filter: false,
  });
  // 成功加载过的真实配置（未经演示数据污染）；首次加载降级时为空，演示数据不可写回
  const realRef = useRef<AppConfig | null>(null);
  const fail = props.fail;

  const scoreMax = props.spec?.score_max ?? null;
  const minDefault = props.spec?.score_min_default ?? null;

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const cfg = await api.getAppConfig();
      // 读取时后端可能没给全字段，这里显式归一，免得任何一处 undefined 崩掉渲染
      const full: AppConfig = {
        ...cfg,
        keywords: cfg.keywords ?? [],
        filters: { ...MOCK_CONFIG.filters, ...(cfg.filters ?? {}) },
        excludes: { ...MOCK_CONFIG.excludes, ...(cfg.excludes ?? {}) },
      };
      realRef.current = full;
      setDraft(full);
      setMockMode(false);
      setLoadNotReady(false);
      setNote('');
    } catch (e) {
      if (isCommandMissing(e) || e instanceof TypeError) {
        setMockMode(true);
        setLoadNotReady(true);
        setDraft(MOCK_CONFIG);
        setNote(`${NOT_READY_TEXT}，下面展示的是本地演示数据`);
      } else {
        setNote(errText(e));
        setLoadNotReady(false);
        fail(e);
      }
    } finally {
      setLoading(false);
    }
  }, [fail]);

  useEffect(() => {
    void load();
    // 只在挂载时拉一次；重载由「重置」按钮显式触发
  }, [load]);

  const patch = (p: Partial<AppConfig>) => setDraft((d) => (d ? { ...d, ...p } : d));
  const patchFilters = (p: Partial<AppFilters>) =>
    setDraft((d) => (d ? { ...d, filters: { ...d.filters, ...p } } : d));
  const patchExcludes = (p: Partial<AppExcludes>) =>
    setDraft((d) => (d ? { ...d, excludes: { ...d.excludes, ...p } } : d));

  const toggleSection = (k: SectionKey) => setOpen((o) => ({ ...o, [k]: !o[k] }));

  /** 只提交真正改动的键：`api_key` 为空串就是「不改」，避免手滑把密钥清掉。 */
  const save = async () => {
    if (!draft) return;
    if (mockMode || !realRef.current) {
      setNote(NOT_READY_TEXT);
      return;
    }
    setSaving(true);
    try {
      const body = diffConfig(realRef.current, draft);
      const key = draft.api_key.trim();
      if (key) body.api_key = key;
      if (Object.keys(body).length === 0) {
        setNote('没有改动');
        return;
      }
      await api.setAppConfig(body);
      setNote(key ? '已保存（密钥已更新）' : '已保存');
      // 保存成功后回读一次：脱敏提示、后端归一后的值都以库里的为准
      await load();
      props.onSaved?.();
    } catch (e) {
      if (isCommandMissing(e) || e instanceof TypeError) {
        setMockMode(true);
        setNote(NOT_READY_TEXT);
      } else {
        setNote(`保存失败：${errText(e)}`);
        fail(e);
      }
    } finally {
      setSaving(false);
    }
  };

  const excludeText = useMemo(() => joinList(draft?.excludes.veto_words), [draft?.excludes.veto_words]);
  const excludesDegree = useMemo(() => new Set(draft?.excludes.exclude_degree ?? []), [draft?.excludes.exclude_degree]);

  if (loading || !draft) {
    return (
      <Card testId="config-panel" title="配置" subtitle="加载中…">
        <div className="empty">正在读取配置…</div>
      </Card>
    );
  }

  // 首屏还没拿到真实配置时只读：避免在演示数据上编辑后被「保存」写回真库
  const ro = mockMode || loadNotReady;

  return (
    <div className="config-wrap" data-testid="config-panel">
      <div className="card config-tools">
        <div className="row wrap" style={{ gap: 10 }}>
          <span style={{ fontSize: 12.5, color: 'var(--text-2)' }}>配置写入本地数据库，重启后保留。</span>
          <Chip tone={mockMode ? 'warn' : 'primary'}>{mockMode ? '后端尚未就绪' : '已就绪'}</Chip>
          <span className="spacer" />
          <span className="card-sub" data-testid="config-note">
            {note || '尚未修改'}
          </span>
          <button className="btn btn-sm btn-ghost" data-testid="config-reset-btn" onClick={() => void load()} disabled={saving}>
            重新加载
          </button>
          <button className="btn btn-sm btn-primary" data-testid="config-save-btn" onClick={() => void save()} disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>

      <div className="config-groups">
        {/* ---------- 大模型 ---------- */}
        <Section sec="llm" open={open} onToggle={toggleSection} counts={draft.model || '未填模型'}>
          <div className="cfg-grid">
            <label className="field cfg-span-2">
              <span className="field-label">
                API Key
                <span className="cfg-hint">
                  {draft.api_key_hint ? `已配置 ${keyHint(draft.api_key_hint)}，留空则不修改` : '当前未配置'}
                </span>
              </span>
              <input
                className="input"
                data-testid="config-llm-key"
                type="password"
                autoComplete="off"
                placeholder={keyHint(draft.api_key_hint)}
                value={draft.api_key}
                disabled={ro}
                onChange={(e) => patch({ api_key: e.target.value })}
              />
            </label>
            <label className="field">
              <span className="field-label">接口地址</span>
              <input
                className="input"
                data-testid="config-llm-base-url"
                placeholder="https://api.deepseek.com/v1"
                value={draft.base_url}
                disabled={ro}
                onChange={(e) => patch({ base_url: e.target.value })}
              />
            </label>
            <label className="field">
              <span className="field-label">模型</span>
              <input
                className="input"
                data-testid="config-llm-model"
                placeholder="deepseek-chat"
                value={draft.model}
                disabled={ro}
                onChange={(e) => patch({ model: e.target.value })}
              />
            </label>
          </div>
        </Section>

        {/* ---------- 抓取 ---------- */}
        <Section sec="scrape" open={open} onToggle={toggleSection} counts={`上限 ${draft.limit}`}>
          <div className="cfg-grid">
            <label className="field cfg-span-3">
              <span className="field-label">关键词（逗号分隔，逐个提交给站点搜索）</span>
              <input
                className="input"
                data-testid="config-keywords"
                placeholder="Java, Spring Cloud, 后端开发"
                value={joinList(draft.keywords)}
                disabled={ro}
                onChange={(e) => patch({ keywords: splitList(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">每轮抓取上限</span>
              <input
                className="input input-num"
                data-testid="config-limit"
                type="number"
                min={1}
                value={draft.limit}
                disabled={ro}
                onChange={(e) => patch({ limit: Number(e.target.value) || 0 })}
              />
            </label>
          </div>
        </Section>

        {/* ---------- 打分 ---------- */}
        <Section sec="score" open={open} onToggle={toggleSection} counts={draft.use_llm ? '大模型通道' : '规则通道'}>
          <div className="cfg-grid">
            <label className="field">
              <span className="field-label">
                入库门槛
                <span className="cfg-hint">默认 {minDefault ?? '—'}</span>
              </span>
              <input
                className="input input-num"
                data-testid="config-min-score"
                type="number"
                min={0}
                max={scoreMax ?? undefined}
                value={draft.min_score}
                disabled={ro}
                onChange={(e) => patch({ min_score: Number(e.target.value) || 0 })}
              />
            </label>
            <label className="field">
              <span className="field-label">
                初筛线
                <span className="cfg-hint">规则分低于它就不调大模型</span>
              </span>
              <input
                className="input input-num"
                data-testid="config-llm-prefilter"
                type="number"
                min={0}
                max={scoreMax ?? undefined}
                value={draft.llm_prefilter}
                disabled={ro || !draft.use_llm}
                onChange={(e) => patch({ llm_prefilter: Number(e.target.value) || 0 })}
              />
            </label>
            <label className="switch" style={{ alignSelf: 'end', paddingBottom: 8 }}>
              <input
                type="checkbox"
                data-testid="config-use-llm"
                checked={draft.use_llm}
                disabled={ro}
                onChange={(e) => patch({ use_llm: e.target.checked })}
              />
              用大模型打分
            </label>
          </div>
        </Section>

        {/* ---------- 打招呼 ---------- */}
        <Section sec="greet" open={open} onToggle={toggleSection} counts={draft.skills ? '已填技能' : '用画像技能'}>
          <div className="cfg-stack">
            <label className="field">
              <span className="field-label">提示词（`{'{岗位}'}` 会被替换成岗位名）</span>
              <textarea
                className="textarea"
                data-testid="config-greet-prompt"
                rows={5}
                value={draft.greet_prompt}
                disabled={ro}
                onChange={(e) => patch({ greet_prompt: e.target.value })}
              />
            </label>
            <label className="field">
              <span className="field-label">技能（逗号分隔；留空则用简历画像里的技能）</span>
              <textarea
                className="textarea"
                data-testid="config-skills"
                rows={3}
                value={draft.skills}
                disabled={ro}
                onChange={(e) => patch({ skills: e.target.value })}
              />
            </label>
          </div>
        </Section>

        {/* ---------- 筛选与排除 ---------- */}
        <Section sec="filter" open={open} onToggle={toggleSection} counts={`排除词 ${draft.excludes.veto_words.length}`}>
          <div className="section-label">下单条件（留空 = 不限）</div>
          <div className="cfg-grid cfg-grid-3">
            <FilterField name="salary" label="薪资范围" value={draft.filters.salary} ro={ro} onChange={(v) => patchFilters({ salary: v })} placeholder="如 20-40K" />
            <FilterField name="experience" label="经验要求" value={draft.filters.experience} ro={ro} onChange={(v) => patchFilters({ experience: v })} placeholder="如 3-5年" />
            <FilterField name="degree" label="学历要求" value={draft.filters.degree} ro={ro} onChange={(v) => patchFilters({ degree: v })} placeholder="如 本科" />
            <FilterField name="scale" label="公司规模" value={draft.filters.scale} ro={ro} onChange={(v) => patchFilters({ scale: v })} placeholder="如 100-499人" />
            <FilterField name="stage" label="融资阶段" value={draft.filters.stage} ro={ro} onChange={(v) => patchFilters({ stage: v })} placeholder="如 已上市" />
            <FilterField name="industry" label="行业" value={draft.filters.industry} ro={ro} onChange={(v) => patchFilters({ industry: v })} placeholder="如 互联网" />
          </div>

          <div className="section-label" style={{ marginTop: 14 }}>
            排除项（命中即不入库，留空 = 不排除）
          </div>
          <div className="cfg-grid cfg-grid-3">
            <label className="field cfg-span-3">
              <span className="field-label">排除词（公司名 / 岗位名里命中任一即排除）</span>
              <input
                className="input"
                data-testid="config-filter-vetoWords"
                placeholder="外包, 驻场, 人力, 兼职"
                value={excludeText}
                disabled={ro}
                onChange={(e) => patchExcludes({ veto_words: splitList(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">薪资下限（低于此值排除，元/月）</span>
              <input
                className="input input-num"
                data-testid="config-filter-exclSalaryMin"
                type="number"
                min={0}
                value={str(draft.excludes.exclude_salary_min)}
                disabled={ro}
                onChange={(e) => patchExcludes({ exclude_salary_min: numOrNull(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">薪资上限（高于此值排除，元/月）</span>
              <input
                className="input input-num"
                data-testid="config-filter-exclSalaryMax"
                type="number"
                min={0}
                value={str(draft.excludes.exclude_salary_max)}
                disabled={ro}
                onChange={(e) => patchExcludes({ exclude_salary_max: numOrNull(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">成立年份早于（年份）</span>
              <input
                className="input input-num"
                data-testid="config-filter-exclFoundedBelow"
                type="number"
                min={1900}
                max={2100}
                placeholder="如 2015"
                value={str(draft.excludes.exclude_founded_below)}
                disabled={ro}
                onChange={(e) => patchExcludes({ exclude_founded_below: numOrNull(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">岗位要求年限超过（年）</span>
              <input
                className="input input-num"
                data-testid="config-filter-exclMaxYears"
                type="number"
                min={0}
                value={str(draft.excludes.exclude_resume_max_years)}
                disabled={ro}
                onChange={(e) => patchExcludes({ exclude_resume_max_years: numOrNull(e.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field-label">HR 活跃度</span>
              <select
                className="select"
                data-testid="config-filter-exclHrDays"
                value={str(draft.excludes.exclude_hr_active_days)}
                disabled={ro}
                onChange={(e) => patchExcludes({ exclude_hr_active_days: numOrNull(e.target.value) })}
              >
                {HR_OPTIONS.map((o) => (
                  <option key={o.value || 'none'} value={o.value}>
                    {o.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="field-label">周末制度</span>
              <select
                className="select"
                data-testid="config-filter-weekendRule"
                value={draft.excludes.weekend ?? ''}
                disabled={ro}
                onChange={(e) => patchExcludes({ weekend: e.target.value || null })}
              >
                {WEEKEND_OPTIONS.map((o) => (
                  <option key={o.value || 'none'} value={o.value}>
                    {o.name}
                  </option>
                ))}
              </select>
            </label>
            <div className="field cfg-span-3">
              <span className="field-label">排除学历要求（多选，命中即排除）</span>
              <div className="row wrap" data-testid="config-filter-exclDegree" style={{ gap: 6 }}>
                {DEGREE_OPTIONS.map((d) => {
                  const on = excludesDegree.has(d);
                  return (
                    <button
                      key={d}
                      type="button"
                      className={`chip ${on ? 'chip-bad' : ''}`}
                      data-testid={`config-filter-degree-${d}`}
                      disabled={ro}
                      onClick={() =>
                        patchExcludes({
                          exclude_degree: on
                            ? draft.excludes.exclude_degree.filter((x) => x !== d)
                            : [...draft.excludes.exclude_degree, d],
                        })
                      }
                    >
                      {d}
                    </button>
                  );
                })}
              </div>
            </div>
          </div>
        </Section>
      </div>
    </div>
  );
}

/** 一个可折叠分组：标题行本身就是按钮，收起时只留标题。 */
function Section(props: {
  sec: SectionKey;
  open: Record<SectionKey, boolean>;
  onToggle: (k: SectionKey) => void;
  counts: string;
  children: ReactNode;
}) {
  const meta = SECTIONS.find((s) => s.key === props.sec);
  const open = props.open[props.sec];
  return (
    <section className="card config-card">
      <button
        type="button"
        className="config-head"
        data-testid={`config-section-${props.sec}`}
        aria-expanded={open}
        onClick={() => props.onToggle(props.sec)}
      >
        <span className={`chevron ${open ? 'is-open' : ''}`}>▸</span>
        <span className="config-title">{meta?.title}</span>
        <span className="card-sub">{meta?.sub}</span>
        <span className="spacer" />
        <Chip>{props.counts}</Chip>
      </button>
      {open && <div className="card-body config-body">{props.children}</div>}
    </section>
  );
}

function FilterField(props: {
  name: string;
  label: string;
  value: string;
  placeholder: string;
  ro: boolean;
  onChange: (v: string) => void;
}) {
  return (
    <label className="field">
      <span className="field-label">{props.label}</span>
      <input
        className="input"
        data-testid={`config-filter-${props.name}`}
        placeholder={props.placeholder}
        value={props.value}
        disabled={props.ro}
        onChange={(e) => props.onChange(e.target.value)}
      />
    </label>
  );
}
