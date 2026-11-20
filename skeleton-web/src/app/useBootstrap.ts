/**
 * 一次性的应用引导：基本信息、打分口径、大模型状态、关窗收尾。
 *
 * 从 `App.tsx` 抽出的理由：这三件事（+ 关窗副作用）在组件生命周期里只发生一次，
 * 却和「页面切换」「列表刷新」混在同一个组件里，导致任何一次改动都要通读 400 行。
 * 抽成 hook 后，`App.tsx` 只剩组装。
 */

import { useCallback, useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, errText, type AppInfo, type LlmStatus, type ScoringSpec } from '../api';

export type Bootstrap = {
  info: AppInfo | null;
  spec: ScoringSpec | null;
  llm: LlmStatus | null;
  /** 引导阶段失败的文本；空串表示正常 */
  error: string;
  dismissError: () => void;
  /** 大模型密钥变更后重拉，让顶栏徽标与试算面板立刻反映「已配置」 */
  refreshLlm: () => void;
  setError: (msg: string) => void;
};

export function useBootstrap(): Bootstrap {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [spec, setSpec] = useState<ScoringSpec | null>(null);
  const [llm, setLlm] = useState<LlmStatus | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    let alive = true;
    Promise.all([api.appInfo(), api.scoringSpec(), api.llmStatus()])
      .then(([i, s, l]) => {
        if (!alive) return;
        setInfo(i);
        setSpec(s);
        setLlm(l);
      })
      .catch((e) => {
        if (alive) setError(errText(e));
      });
    return () => {
      alive = false;
    };
  }, []);

  // 关窗时收尾 sidecar：只 stop 不关 stdin 的话，Python 主循环仍在等输入，
  // 进程会残留到父进程被杀为止。`scrape_close` 是唯一会关 stdin 的命令。
  // 不 preventDefault——收尾失败也不能让用户关不掉窗口。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested(() => {
        void api.scrapeClose().catch(() => {
          /* 进程早就没了也算正常，静默即可 */
        });
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        /* 非 Tauri 环境（浏览器调试）没有窗口事件，忽略 */
      });
    return () => unlisten?.();
  }, []);

  const refreshLlm = useCallback(() => {
    api.llmStatus().then(setLlm).catch((e) => setError(errText(e)));
  }, []);

  const dismissError = useCallback(() => setError(''), []);

  return { info, spec, llm, error, dismissError, refreshLlm, setError };
}
