/**
 * 打分试算：手动填一份岗位，看规则分怎么算出来的。
 *
 * 大模型开关只有在 `llm_status.configured` 时才可点——未配置密钥是正常状态，
 * 不是故障，所以整块面板照常可用，只是大模型不可选。
 * 初筛线默认值来自 `llm_status.prefilter_default`，前端不写死。
 */
import { useState } from 'react';
import { api, type LlmStatus, type ScoreReply, type ScoringSpec } from '../../api';
import { Card, Chip } from '../../components/ui';

export function ScoreDemo(props: {
  spec: ScoringSpec | null;
  llm: LlmStatus | null;
  minScore: number | null;
  fail: (e: unknown) => void;
}) {
  const [title, setTitle] = useState('Java 后端开发工程师');
  const [salary, setSalary] = useState('20-40K');
  const [experience, setExperience] = useState('5-10年');
  const [jd, setJd] = useState(
    '负责后端服务设计与开发，熟悉 Java、Spring Boot、Spring Cloud、MySQL、Redis、RabbitMQ，有高并发电商经验优先。',
  );
  const [useLlm, setUseLlm] = useState(false);
  const [prefilter, setPrefilter] = useState<number | ''>('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ScoreReply | null>(null);

  const configured = props.llm?.configured === true;
  const prefilterValue = prefilter === '' ? (props.llm?.prefilter_default ?? undefined) : Number(prefilter);

  const run = async () => {
    setBusy(true);
    try {
      const r = await api.scoreJob({
        company: '演示公司',
        title,
        salary,
        experience,
        jd,
        useLlm: useLlm && configured,
        llmPrefilter: useLlm && configured ? prefilterValue : undefined,
      });
      setResult(r);
    } catch (e) {
      props.fail(e);
    } finally {
      setBusy(false);
    }
  };

  const pass = result && props.minScore !== null ? result.score >= props.minScore : null;

  return (
    <Card
      testId="demo-panel"
      title="打分试算"
      subtitle={useLlm && configured ? '规则 → 初筛 → 大模型' : '纯规则，不调大模型、不联网'}
    >
      <div className="demo-grid">
        <label className="field">
          <span className="field-label">岗位名称</span>
          <input className="input" data-testid="demo-title" value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label className="field">
          <span className="field-label">薪资</span>
          <input className="input" data-testid="demo-salary" value={salary} onChange={(e) => setSalary(e.target.value)} />
        </label>
        <label className="field">
          <span className="field-label">经验</span>
          <input
            className="input"
            data-testid="demo-exp"
            value={experience}
            onChange={(e) => setExperience(e.target.value)}
          />
        </label>
      </div>

      <label className="field">
        <span className="field-label">职位描述（留空会走「无 JD」判定，直接判不入库）</span>
        <textarea
          className="textarea"
          data-testid="demo-jd"
          value={jd}
          rows={3}
          onChange={(e) => setJd(e.target.value)}
        />
      </label>

      <div className="row wrap" style={{ marginTop: 12 }}>
        <button className="btn btn-primary" data-testid="demo-score-btn" onClick={run} disabled={busy}>
          {busy ? '试算中…' : '试算'}
        </button>
        <label
          className="switch"
          title={configured ? `已配置 ${props.llm?.key_hint ?? ''}${props.llm?.model ? `（${props.llm.model}）` : ''}` : '未配置 API 密钥，只能用规则打分'}
          style={{ opacity: configured ? 1 : 0.45 }}
        >
          <input
            type="checkbox"
            data-testid="demo-llm-toggle"
            checked={useLlm && configured}
            disabled={!configured}
            onChange={(e) => setUseLlm(e.target.checked)}
          />
          大模型打分
        </label>
        <label className="row" style={{ gap: 6, fontSize: 12, color: 'var(--text-3)' }}>
          初筛线
          <input
            className="input input-num"
            data-testid="demo-prefilter"
            type="number"
            min={0}
            max={props.spec?.score_max ?? undefined}
            value={prefilter}
            placeholder={props.llm?.prefilter_default !== undefined ? String(props.llm.prefilter_default) : ''}
            disabled={!useLlm || !configured}
            onChange={(e) => setPrefilter(e.target.value === '' ? '' : Number(e.target.value))}
          />
        </label>
        {!configured && <span className="card-sub">未配置密钥 · 仅规则可用</span>}
      </div>

      {result && (
        <div className="demo-result" data-testid="demo-result">
          <div className="demo-score">
            <span
              className={`big ${pass === null ? 'is-none' : pass ? 'is-pass' : 'is-fail'}`}
              style={{ color: pass === null ? undefined : pass ? 'var(--good)' : 'var(--bad)' }}
            >
              {result.score}
            </span>
            <span style={{ color: 'var(--text-3)', fontSize: 12.5 }}>/ {props.spec?.score_max ?? '—'} 分</span>
            <Chip tone={result.source === 'llm' ? 'accent' : 'default'}>{result.source === 'llm' ? '大模型' : '规则'}</Chip>
            {pass !== null && <Chip tone={pass ? 'primary' : 'bad'}>{pass ? '达到门槛' : '低于门槛'}</Chip>}
            {result.tokens > 0 && <Chip tone="accent">{result.tokens} tok</Chip>}
            {result.rejected_no_jd && <Chip tone="warn">无 JD · 判不入库</Chip>}
          </div>
          <div className="demo-reason">{result.reason || '无说明'}</div>
        </div>
      )}
    </Card>
  );
}
