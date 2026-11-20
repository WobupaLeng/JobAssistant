/**
 * 页面层的统一出口：每个 nav 一个页面组件，各自编排本页的 feature。
 *
 * `App.tsx` 只从这里取一个页面渲染，不再知道任何 feature 的存在——
 * 这就是「禁止反向 import」在代码上的落地：
 * 组装层依赖页面层，页面层依赖领域层，方向永远朝下。
 */

import type { LlmStatus, ScoringSpec } from '../api';
import type { Nav } from '../lib/nav';
import { ChatPanel } from '../features/chat/ChatPanel';
import { ConfigPanel } from '../features/config/ConfigPanel';
import { LogPanel } from '../features/logs/LogPanel';
import { DashboardPage } from './DashboardPage';

export type PageProps = {
  nav: Nav;
  spec: ScoringSpec | null;
  llm: LlmStatus | null;
  /** 每次进入页面递增，驱动该页重拉数据 */
  refreshKey: number;
  notify: (msg: string) => void;
  fail: (e: unknown) => void;
  onConfigSaved: () => void;
};

export function Page(props: PageProps) {
  switch (props.nav) {
    case 'dashboard':
      return <DashboardPage spec={props.spec} llm={props.llm} notify={props.notify} fail={props.fail} />;
    case 'chat':
      return <ChatPanel refreshKey={props.refreshKey} />;
    case 'config':
      return <ConfigPanel spec={props.spec} onSaved={props.onConfigSaved} fail={props.fail} />;
    case 'logs':
      return <LogPanel refreshKey={props.refreshKey} fail={props.fail} />;
  }
}
