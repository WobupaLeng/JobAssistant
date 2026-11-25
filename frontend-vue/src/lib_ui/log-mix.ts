/**
 * 概览页日志卡的「一条流」模型（用户 m00182 / m00374：运行日志与岗位事件流别分两块显示，
 * 主页合成一条时间线）。
 *
 * 合并的两类行用的是**同一个时钟**：运行日志是后端 `LogRow.ts` 的 Unix 毫秒，事件流本来
 * 不带时间，由 `DashboardPage.drainFeed` 在收到增量时补本地接收时刻（也是毫秒）。
 * 决策记录**不进这条线**——它在「决策记录」覆盖页里单独看（用户 m00404）。
 *
 * 这里只放纯函数：排序、截断、筛页签。DOM 与取数留在 `components/LogCard.vue` 与
 * `pages/DashboardPage.vue`。
 */
import type { FeedLine } from './scrape-panel-model';
import type { LogLevel } from './logfmt';

/** 日志卡里的一行运行日志（与 `components/LogCard.vue` 的 `LogLine` 同构）。 */
export interface MixLog {
  readonly ts: number;
  readonly seq: number;
  readonly level: LogLevel;
  readonly message: string;
}

/** 页签：三类日志级别 + 「全部」+「事件」（✔ / 跳过 岗位行）。 */
export type MixTab = 'all' | 'error' | 'warn' | 'info' | 'event';

export type MixRow =
  | {
      readonly kind: 'log';
      readonly key: string;
      readonly ts: number;
      readonly seq: number;
      readonly level: LogLevel;
      readonly message: string;
    }
  | {
      readonly kind: 'feed';
      readonly key: string;
      readonly ts: number;
      readonly tag: string;
      readonly msg: string;
      readonly tone: FeedLine['tone'];
    };

/** 卡里最多留多少行（卡高只放得下约 8 行，留多了纯属浪费 DOM）。 */
export const MIX_LIMIT = 200;

/** 页签顺序即显示顺序：全部 → 三档级别 → 岗位事件。 */
export function mixTabs(): readonly { readonly key: MixTab; readonly label: string }[] {
  return [
    { key: 'all', label: '全部' },
    { key: 'error', label: '错误' },
    { key: 'warn', label: '警告' },
    { key: 'info', label: '信息' },
    { key: 'event', label: '事件' },
  ];
}

/**
 * 合并成一条按时间**正序**（旧 → 新）的流。同毫秒时运行日志在前、事件在后——
 * 事件行是「这条岗位怎么处理了」的结果，摆在同时刻的过程日志之后更符合阅读顺序。
 */
export function mergeMix(
  logs: readonly MixLog[],
  feed: readonly FeedLine[],
  limit: number = MIX_LIMIT,
): MixRow[] {
  const rows: { readonly at: number; readonly order: number; readonly row: MixRow }[] = [];
  for (const l of logs) {
    rows.push({
      at: l.ts,
      order: 0,
      row: { kind: 'log', key: `log-${l.ts}-${l.seq}`, ts: l.ts, seq: l.seq, level: l.level, message: l.message },
    });
  }
  for (const f of feed) {
    // `ts` 缺失 / 非法（老 IPC 桩、手写测试数据）按 0 处理，不许变成 NaN 把整列排乱。
    const at = Number.isFinite(f.ts) && f.ts > 0 ? f.ts : 0;
    rows.push({
      at,
      order: 1,
      row: { kind: 'feed', key: `feed-${f.n}-${at}`, ts: at, tag: f.tag, msg: f.msg, tone: f.tone },
    });
  }
  rows.sort((a, b) => a.at - b.at || a.order - b.order);
  const tail = rows.length > limit ? rows.slice(rows.length - limit) : rows;
  return tail.map((r) => r.row);
}

/** 该行在某个页签下是否显示。 */
export function mixVisible(row: MixRow, tab: MixTab): boolean {
  if (tab === 'all') return true;
  if (tab === 'event') return row.kind === 'feed';
  return row.kind === 'log' && row.level === tab;
}

/** 合并 + 筛页签，一步到位。 */
export function pickMix(rows: readonly MixRow[], tab: MixTab): MixRow[] {
  return rows.filter((r) => mixVisible(r, tab));
}

/** 空列表时那句话：区分「一条都还没有」与「这一类还没有」，别让用户以为筛坏了。 */
export function mixEmptyText(tab: MixTab): string {
  switch (tab) {
    case 'error':
      return '还没有错误日志。';
    case 'warn':
      return '还没有警告日志。';
    case 'info':
      return '还没有信息日志。';
    case 'event':
      return '还没有岗位事件。抓取到的岗位会以「✔ / 跳过」出现在这里。';
    case 'all':
    default:
      return '尚无事件。点击「开始抓取」后这里会滚动显示进度。';
  }
}