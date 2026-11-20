/**
 * 全量重算的状态机。
 *
 * 从 `App.tsx` 抽出：重算要**分批驱动游标直到走完**，`limit` 交给 Rust 默认 50，
 * 界面不暴露这个数字（免得用户以为「只重算了 50 条」）。单条写失败只累加计数、
 * 不中断整批。这是一段有循环与累计变量的流程控制，混在渲染函数里极易被改错。
 */

import { useCallback, useState } from 'react';
import { api, errText } from '../../api';

export type RescoreState = {
  running: boolean;
  useLlm: boolean;
  note: string;
  setUseLlm: (v: boolean) => void;
  run: (opts: { llmConfigured: boolean; onDone: () => void }) => Promise<void>;
};

export function useRescore(): RescoreState {
  const [running, setRunning] = useState(false);
  const [useLlm, setUseLlm] = useState(false);
  const [note, setNote] = useState('');

  const run = useCallback(
    async (opts: { llmConfigured: boolean; onDone: () => void }) => {
      setRunning(true);
      setNote('重算中…');
      let cursor: string | undefined;
      let scanned = 0;
      let updated = 0;
      let failed = 0;
      let batches = 0;
      try {
        for (;;) {
          const r = await api.rescoreJobs({ afterJobId: cursor, useLlm: useLlm && opts.llmConfigured });
          scanned += r.scanned;
          updated += r.updated;
          failed += r.failed;
          batches += 1;
          setNote(`重算中… 已扫 ${scanned} 条，改写 ${updated} 条（第 ${batches} 批）`);
          if (!r.has_more || !r.next_cursor) break;
          cursor = r.next_cursor;
        }
        setNote(`重算完成：扫描 ${scanned} 条，改写 ${updated} 条${failed > 0 ? `，失败 ${failed} 条` : ''}`);
        opts.onDone();
      } catch (e) {
        setNote(`重算中断：${errText(e)}`);
      } finally {
        setRunning(false);
      }
    },
    [useLlm],
  );

  return { running, useLlm, note, setUseLlm, run };
}
