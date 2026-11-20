/**
 * 岗位详情抽屉：展示打分依据与 JD 原文。
 *
 * JD 在列表批次里可能被省略（体积原因），所以打开时用 `get_job` 补一次，
 * 只补当前这条，不改列表状态。
 */
import { useEffect, useState } from 'react';
import { api, type Job } from '../../api';
import { Chip } from '../../components/ui';
import { isRejected } from '../../lib/job';

export function JobDrawer(props: { job: Job; minScore: number | null; scoreMax: number | null; onClose: () => void }) {
  const { job, minScore, scoreMax } = props;
  const [detail, setDetail] = useState<Job>(job);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    // 列表里不带 JD 时才补拉；带上就省一次 IPC
    if (job.jd_text) {
      setDetail(job);
      return;
    }
    let alive = true;
    setLoading(true);
    api
      .getJob(job.job_id)
      .then((full) => {
        if (alive && full) setDetail(full);
      })
      .catch(() => {
        /* 单条补拉失败不影响已展示的列表字段 */
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [job]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') props.onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [props]);

  const j = detail;
  const rejected = isRejected(j);
  const pass = j.match_score !== null && minScore !== null ? j.match_score >= minScore : null;
  const tone = rejected || pass === false ? 'bad' : pass === true ? 'primary' : 'default';
  const when = j.updated_at ?? j.created_at;

  return (
    <>
      <div className="drawer-mask" onClick={props.onClose} />
      <aside className="drawer" data-testid="job-drawer" role="dialog" aria-label="岗位详情">
        <header className="drawer-head">
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontSize: 16, fontWeight: 650, lineHeight: 1.35 }}>{j.title ?? '（无标题）'}</div>
            <div className="row wrap" style={{ marginTop: 8 }}>
              <span style={{ color: 'var(--text-2)', fontSize: 12.5 }}>{j.company ?? '未知公司'}</span>
              {j.salary && <Chip>{j.salary}</Chip>}
              {j.location && <Chip>{j.location}</Chip>}
              {rejected && <Chip tone="bad">规则否决</Chip>}
            </div>
          </div>
          <div style={{ textAlign: 'right', flexShrink: 0 }}>
            <div
              className={`score-num ${pass === null ? 'is-none' : pass ? 'is-pass' : 'is-fail'}`}
              style={{ fontSize: 28, lineHeight: 1 }}
              data-testid="drawer-score"
            >
              {j.match_score ?? '—'}
            </div>
            <div className="card-sub" style={{ marginTop: 4 }}>
              / {scoreMax ?? '—'} 分
            </div>
          </div>
          <button className="btn btn-sm btn-ghost" data-testid="drawer-close" onClick={props.onClose} title="关闭（Esc）">
            ✕
          </button>
        </header>

        <div className="drawer-body">
          <div className="row wrap">
            {j.score_source && <Chip tone="accent">{j.score_source}</Chip>}
            {pass !== null && (
              <Chip tone={pass ? 'primary' : 'bad'}>
                {pass ? '达到门槛' : '低于门槛'} {minScore}
              </Chip>
            )}
            {j.score_scale && <Chip title="打分制">制式 {j.score_scale}</Chip>}
            {j.platform && <Chip tone="default">{j.platform}</Chip>}
          </div>

          <div>
            <div className="section-label">打分依据</div>
            <div className="jd-text" style={{ maxHeight: 160 }} data-testid="drawer-reason">
              {j.score_reason || (loading ? '读取中…' : '无打分说明')}
            </div>
          </div>

          <div>
            <div className="section-label">岗位信息</div>
            <dl className="kv">
              <dt>经验</dt>
              <dd>{j.experience ?? '—'}</dd>
              <dt>学历</dt>
              <dd>{j.degree ?? '—'}</dd>
              <dt>行业</dt>
              <dd>{j.company_industry ?? '—'}</dd>
              <dt>规模</dt>
              <dd>{j.company_scale ?? '—'}</dd>
              <dt>阶段</dt>
              <dd>{j.company_stage ?? '—'}</dd>
              <dt>在招</dt>
              <dd>{j.hiring_count !== null ? `${j.hiring_count} 个岗位` : '—'}</dd>
              <dt>HR</dt>
              <dd>
                {[j.hr_name, j.hr_position].filter(Boolean).join(' · ') || '—'}
                {j.hr_active_status && (
                  <span style={{ marginLeft: 8 }}>
                    <Chip tone="default">{j.hr_active_status}</Chip>
                  </span>
                )}
              </dd>
              <dt>状态</dt>
              <dd>
                {j.status}
                {rejected && <Chip tone={tone === 'bad' ? 'bad' : 'warn'}>已排除</Chip>}
              </dd>
              <dt>入库</dt>
              <dd>{when > 0 ? new Date(when * 1000).toLocaleString('zh-CN') : '—'}</dd>
              <dt>链接</dt>
              <dd>
                {j.job_url ? (
                  <a
                    href={j.job_url}
                    target="_blank"
                    rel="noreferrer"
                    style={{ color: 'var(--primary)', wordBreak: 'break-all' }}
                  >
                    在浏览器打开
                  </a>
                ) : (
                  '—'
                )}
              </dd>
            </dl>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', minHeight: 0 }}>
            <div className="section-label">职位描述</div>
            <div className="jd-text" data-testid="drawer-jd">
              {j.jd_text ? j.jd_text : loading ? '读取中…' : '（该岗位没有 JD 原文，抓取时通常按「无 JD」降权处理）'}
            </div>
          </div>
        </div>
      </aside>
    </>
  );
}
