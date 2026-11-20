/**
 * 岗位列表：页签（全部 / 未排除 / 已排除）× 排序 × 低门槛着色 × 选中联动详情。
 *
 * 判定口径：`rejected === 1` 才是被规则否决；`match_score` 与门槛的比较只
 * 决定颜色（**门槛来自 Rust 下发的 `score_min_default`**，本文件不写死数字）。
 */
import { useMemo } from 'react';
import type { Job, SortField } from '../../api';
import { Chip, SkeletonRows } from '../../components/ui';
import { isRejected } from '../../lib/job';

export type ListTab = 'all' | 'active' | 'rejected';

const PAGE = 200;

const TAB_LABEL: Record<ListTab, string> = {
  all: '全部',
  active: '未排除',
  rejected: '已排除',
};

export function JobList(props: {
  jobs: Job[];
  tab: ListTab;
  onTab: (t: ListTab) => void;
  sort: SortField;
  ascending: boolean;
  onSort: (field: SortField) => void;
  minScore: number | null;
  total: number;
  loading: boolean;
  selectedId: string | null;
  onSelect: (job: Job) => void;
  onRefresh: () => void;
}) {
  const { jobs, tab, minScore, selectedId } = props;

  const visible = useMemo(() => {
    const list = jobs.filter((j) => {
      if (tab === 'active') return !isRejected(j);
      if (tab === 'rejected') return isRejected(j);
      return true;
    });
    return list.slice(0, PAGE);
  }, [jobs, tab]);

  const counts = useMemo(() => {
    const rejected = jobs.filter(isRejected).length;
    return { all: jobs.length, active: jobs.length - rejected, rejected };
  }, [jobs]);

  const scoreClass = (job: Job) => {
    if (job.match_score === null) return 'is-none';
    if (minScore === null) return '';
    return job.match_score >= minScore ? 'is-pass' : 'is-fail';
  };

  const arrow = (field: SortField) => (props.sort === field ? (props.ascending ? ' ↑' : ' ↓') : '');

  return (
    <section className="card" style={{ display: 'flex', flexDirection: 'column', minHeight: 320, flex: '1 1 auto' }}>
      <header className="card-head" style={{ paddingBottom: 10 }}>
        <h2 className="card-title">岗位列表</h2>
        <span className="card-sub" data-testid="job-count">
          共 {props.total} 条{visible.length < props.total ? `，显示前 ${visible.length} 条` : ''}
        </span>
        <span className="spacer" />
        <div className="tabs" role="tablist">
          {(Object.keys(TAB_LABEL) as ListTab[]).map((k) => (
            <button
              key={k}
              role="tab"
              aria-selected={tab === k}
              className={`tab ${tab === k ? 'is-active' : ''}`}
              data-testid={`job-tab-${k}`}
              onClick={() => props.onTab(k)}
            >
              {TAB_LABEL[k]}
              <span className="tab-count">{counts[k]}</span>
            </button>
          ))}
        </div>
        <button className="btn btn-sm btn-ghost" data-testid="job-refresh" onClick={props.onRefresh}>
          刷新
        </button>
      </header>

      <div className="card-body" style={{ paddingTop: 10, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
        <div className="table-wrap" style={{ maxHeight: 420 }}>
          <table className="table">
            <thead>
              <tr>
                <th className="col-score" onClick={() => props.onSort('score')} style={{ cursor: 'pointer' }}>
                  分数{arrow('score')}
                </th>
                <th>岗位</th>
                <th className="cell-ellipsis">公司</th>
                <th>薪资</th>
                <th>地点</th>
                <th className="cell-ellipsis">经验</th>
                <th className="col-source">来源</th>
              </tr>
            </thead>
            <tbody className="list-fade">
              {props.loading && jobs.length === 0 && <SkeletonRows cols={7} />}
              {!props.loading && visible.length === 0 && (
                <tr>
                  <td colSpan={7} className="empty" data-testid="job-empty">
                    {tab === 'rejected'
                      ? '没有命中排除规则的岗位。'
                      : '暂无岗位数据。先在「抓取控制」里扫码登录，再点「开始抓取」；' +
                        '抓取到的岗位会按当前规则打分后出现在这里。'}
                  </td>
                </tr>
              )}
              {visible.map((job) => {
                const rejected = isRejected(job);
                return (
                  <tr
                    key={job.job_id}
                    className={`${selectedId === job.job_id ? 'is-selected' : ''} ${rejected ? 'is-rejected' : ''}`}
                    data-testid={`job-row-${job.job_id}`}
                    onClick={() => props.onSelect(job)}
                  >
                    <td>
                      <span className={`score-num ${scoreClass(job)}`} data-testid={`score-${job.job_id}`}>
                        {job.match_score ?? '—'}
                      </span>
                    </td>
                    <td className="cell-title" title={job.title ?? ''}>
                      {job.title ?? '（无标题）'}
                      {rejected && (
                        <span style={{ marginLeft: 8 }}>
                          <Chip tone="bad">已排除</Chip>
                        </span>
                      )}
                    </td>
                    <td className="cell-ellipsis" title={job.company ?? ''}>
                      {job.company ?? '—'}
                    </td>
                    <td style={{ color: 'var(--text-2)' }}>{job.salary ?? '—'}</td>
                    <td style={{ color: 'var(--text-2)' }}>{job.location ?? '—'}</td>
                    <td style={{ color: 'var(--text-2)' }}>{job.experience ?? '—'}</td>
                    <td>
                      <span style={{ color: 'var(--text-3)', fontSize: 11.5 }}>{job.score_source ?? '—'}</span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
