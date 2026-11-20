/**
 * 抓取关键词与门槛的**共享**状态。
 *
 * 为什么不是一个组件内的 `useState`：这两个值被两个 feature 同时编辑——
 * `ScrapePanel`（用户手敲）与 `Expectations`（一键填入）。二者在概览页上并存，
 * 所以状态必须提在它们的公共父层（`DashboardPage`），而不是任一方内部。
 *
 * 下沉与共享的界线就在这里：**只有一个 feature 用的状态留在 feature 内**，
 * 跨 feature 的才提到页面层。这也解释了为什么 `useJobList` / `useRescore`
 * 可以自成一体，而这个不行。
 */

import { useCallback, useState } from 'react';
import { mergeKeywords } from '../../lib/text';

export type ScrapeForm = {
  keywords: string;
  minScore: number | null;
  setKeywords: (v: string) => void;
  setMinScore: (v: number | null) => void;
  /** 把一组期望职位并进关键词（追加不覆盖）。返回真正新增的词。 */
  applyExpectations: (names: string[]) => string[];
};

const DEFAULT_KEYWORDS = 'Java, Spring Cloud, 后端开发';

export function useScrapeForm(minScoreDefault: number | null): ScrapeForm {
  const [keywords, setKeywords] = useState(DEFAULT_KEYWORDS);
  // 门槛初值来自 Rust 下发的口径；用户改过之后不再覆盖
  const [minScore, setMinScoreRaw] = useState<number | null>(minScoreDefault);
  const [touched, setTouched] = useState(false);

  const setMinScore = useCallback((v: number | null) => {
    setTouched(true);
    setMinScoreRaw(v);
  }, []);

  // 口径晚到时补初值（get_scoring_spec 是异步的）
  if (!touched && minScoreDefault !== null && minScore === null) {
    setMinScoreRaw(minScoreDefault);
  }

  const applyExpectations = useCallback((names: string[]) => {
    let added: string[] = [];
    setKeywords((cur) => {
      const r = mergeKeywords(cur, names);
      added = r.added;
      return r.text;
    });
    return added;
  }, []);

  return { keywords, minScore, setKeywords, setMinScore, applyExpectations };
}
