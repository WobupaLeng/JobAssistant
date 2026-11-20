/**
 * 岗位列表的状态与取数。
 *
 * 从 `App.tsx` 下沉而来：列表的页签、排序、选中项原先与迁移横幅、重算、
 * 抓取关键词等**互不相关**的状态挤在同一个组件里。下沉后，
 * 列表相关的改动只影响本文件。
 */

import { useCallback, useRef, useState } from 'react';
import { api, errText, type Job, type SortField } from '../../api';

/** 列表页签：`active` 排除已否决，`rejected` 只看否决，`all` 全看。 */
export type ListTab = 'all' | 'active' | 'rejected';

export type JobListState = {
  jobs: Job[];
  total: number;
  hasLegacy: boolean;
  loading: boolean;
  error: string;
  tab: ListTab;
  sort: SortField;
  ascending: boolean;
  selected: Job | null;
  setTab: (t: ListTab) => void;
  onSort: (f: SortField) => void;
  select: (j: Job | null) => void;
  refresh: () => Promise<void>;
  migrate: () => Promise<string>;
};

const PAGE_LIMIT = 500;

export function useJobList(): JobListState {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [total, setTotal] = useState(0);
  const [hasLegacy, setHasLegacy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [tab, setTab] = useState<ListTab>('active');
  const [sort, setSort] = useState<SortField>('score');
  const [ascending, setAscending] = useState(false);
  const [selected, setSelected] = useState<Job | null>(null);

  // 用 ref 保存最新查询条件，避免 refresh 的 identity 随 tab/sort 变化而打断调用方
  const query = useRef({ tab, sort, ascending });
  query.current = { tab, sort, ascending };

  /** 列表与总数一起刷新：总数来自 `db_overview`（含已排除），列表按页签条件拉取。 */
  const refresh = useCallback(async () => {
    const q = query.current;
    try {
      const [overview, list] = await Promise.all([
        api.dbOverview(),
        api.listJobs({
          limit: PAGE_LIMIT,
          include_rejected: q.tab !== 'active',
          sort: q.sort,
          ascending: q.ascending,
        }),
      ]);
      setTotal(overview.total);
      setHasLegacy(overview.has_legacy);
      setJobs(list);
      setError('');
    } catch (e) {
      setError(errText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const onSort = useCallback((field: SortField) => {
    setSort((cur) => {
      if (field === cur) {
        setAscending((v) => !v);
        return cur;
      }
      // 分数默认从高到低，时间默认从新到旧
      setAscending(false);
      return field;
    });
  }, []);

  /** 迁移旧十分制数据。返回一句给用户看的结果文案。 */
  const migrate = useCallback(async () => {
    const changed = await api.migrateScores();
    await refresh();
    return changed ? '迁移完成：旧分数已换算为百分制' : '没有需要迁移的数据';
  }, [refresh]);

  return {
    jobs,
    total,
    hasLegacy,
    loading,
    error,
    tab,
    sort,
    ascending,
    selected,
    setTab,
    onSort,
    select: setSelected,
    refresh,
    migrate,
  };
}
