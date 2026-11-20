/**
 * 概览页：统计 + 岗位列表 + 抓取 + 求职期望 + 打分试算。
 *
 * 这一页是整个应用的默认落地页。`tools/gui_e2e.py` 只按 testid 定位本页的控件，
 * 因此**任何控件都不得被挪到别的页签或折叠区**——那会让 E2E 静默失效
 * （它不会报「找不到」，而是报一堆看起来像业务失败的断言）。
 *
 * 状态归属（分层规则的具体体现）：
 * - 列表、排序、选中 → `useJobList`（只有本页用）
 * - 重算            → `useRescore`（只有本页用）
 * - 关键词、门槛     → 提到本层（`ScrapePanel` 与 `Expectations` 共享）
 */

import { useEffect, useMemo, useRef } from 'react';
import type { LlmStatus, ScoringSpec } from '../api';
import { Banner, Stat } from '../components/ui';
import { JobDrawer } from '../features/jobs/JobDrawer';
import { JobList } from '../features/jobs/JobList';
import { useJobList } from '../features/jobs/useJobList';
import { useRescore } from '../features/jobs/useRescore';
import { Expectations } from '../features/scrape/Expectations';
import { ScrapePanel } from '../features/scrape/ScrapePanel';
import { useScrapeForm } from '../features/scrape/useScrapeForm';
import { ScoreDemo } from '../features/score/ScoreDemo';

export function DashboardPage(props: {
  spec: ScoringSpec | null;
  llm: LlmStatus | null;
  notify: (msg: string) => void;
  fail: (e: unknown) => void;
}) {
  const list = useJobList();
  const rescore = useRescore();
  const form = useScrapeForm(props.spec?.score_min_default ?? null);
  const { refresh } = list;

  // 引用稳定化：refresh 在挂载后不变，但首帧的依赖顺序要保证只跑一次
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  useEffect(() => {
    void refreshRef.current();
  }, [list.tab, list.sort, list.ascending]);

  const excluded = useMemo(() => list.jobs.filter((j) => j.rejected === 1).length, [list.jobs]);

  const onMigrate = async () => {
    try {
      props.notify(await list.migrate());
    } catch (e) {
      props.fail(e);
    }
  };

  return (
    <>
      {list.hasLegacy && (
        <Banner
          testId="legacy-banner"
          action={
            <button className="btn btn-sm btn-warn" data-testid="migrate-btn" onClick={() => void onMigrate()}>
              迁移到百分制
            </button>
          }
        >
          检测到 <b>未迁移的十分制数据</b>，分数口径与当前规则不一致。迁移会把旧分数×10 并打上标记，只执行一次。
        </Banner>
      )}

      <section className="card" style={{ padding: '12px 16px' }}>
        <div className="row wrap" style={{ gap: 10 }}>
          <Stat label="岗位总数" value={list.total} testId="overview-total" />
          <Stat label="当前列表" value={list.jobs.length} testId="overview-listed" />
          <Stat label="已排除" value={excluded} tone="warn" testId="overview-rejected" />
          <span className="spacer" style={{ minWidth: 12 }} />
          <div
            className="row wrap"
            data-testid="rescore-panel"
            style={{
              gap: 10,
              padding: '8px 12px',
              borderRadius: 12,
              background: 'var(--surface-input)',
              border: '1px solid var(--hairline)',
            }}
          >
            <span style={{ fontSize: 12, color: 'var(--text-3)', maxWidth: 260 }}>
              按当前口径重算全库分数。规则重算不联网；开启大模型后每条都会真实计费。
            </span>
            <label className="switch" style={{ opacity: props.llm?.configured ? 1 : 0.45 }}>
              <input
                type="checkbox"
                data-testid="rescore-llm-toggle"
                checked={rescore.useLlm && props.llm?.configured === true}
                disabled={rescore.running || !props.llm?.configured}
                onChange={(e) => rescore.setUseLlm(e.target.checked)}
              />
              用大模型
            </label>
            <span className="card-sub" data-testid="rescore-note">
              {rescore.note || '尚未重算'}
            </span>
            <button
              className="btn btn-sm"
              data-testid="rescore-btn"
              onClick={() =>
                void rescore.run({
                  llmConfigured: props.llm?.configured === true,
                  onDone: () => void refreshRef.current(),
                })
              }
              disabled={rescore.running || list.total === 0}
            >
              {rescore.running ? '重算中…' : '全量重算'}
            </button>
          </div>
        </div>
      </section>

      <JobList
        jobs={list.jobs}
        tab={list.tab}
        onTab={list.setTab}
        sort={list.sort}
        ascending={list.ascending}
        onSort={list.onSort}
        minScore={props.spec?.score_min_default ?? null}
        total={list.total}
        loading={list.loading}
        selectedId={list.selected?.job_id ?? null}
        onSelect={list.select}
        onRefresh={() => void list.refresh()}
      />

      <ScrapePanel
        keywords={form.keywords}
        onKeywords={form.setKeywords}
        minScore={form.minScore}
        onMinScore={form.setMinScore}
        scoreMax={props.spec?.score_max ?? null}
        minScoreDefault={props.spec?.score_min_default ?? null}
        onInserted={() => void refreshRef.current()}
        notify={props.notify}
        fail={props.fail}
      />

      <Expectations
        onPick={(names) => {
          const added = form.applyExpectations(names);
          props.notify(
            added.length > 0 ? `已填入关键词：${added.join('、')}` : '这些期望职位已经在关键词里了',
          );
        }}
        fail={props.fail}
      />

      <ScoreDemo
        spec={props.spec}
        llm={props.llm}
        minScore={props.spec?.score_min_default ?? null}
        fail={props.fail}
      />

      {list.selected && (
        <JobDrawer
          job={list.selected}
          minScore={props.spec?.score_min_default ?? null}
          scoreMax={props.spec?.score_max ?? null}
          onClose={() => list.select(null)}
        />
      )}
    </>
  );
}
