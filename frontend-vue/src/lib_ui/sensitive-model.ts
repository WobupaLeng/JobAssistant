/**
 * 敏感库的纯逻辑：页签 / 关键字 / 全选 / 批量删除的可见性判定。
 *
 * 命令层在 `api/commands/sensitive.ts`，这里只管"该显示什么"。
 * 为什么独立成文件：`sensitive_list` 的 `kind` 只有三态，而"页签"是
 * 前端概念（把 `stored` 与 `rejected` 分开展示）；映射写错不会报错，
 * 只会静默返回全部——正是 C3 那类失效，所以用单测钉住。
 */
import type { SensitiveRecord } from '../api/types';

/** 三个页签：全部 / 已入库 / 已排除。 */
export type SensitiveTab = 'all' | 'stored' | 'excluded';

export interface SensitiveTabMeta {
  readonly key: SensitiveTab;
  readonly label: string;
  /** 发给后端的 `kind`：`excluded` 对应后端的 `rejected`。 */
  readonly kind: 'all' | 'stored' | 'rejected';
}

export const SENSITIVE_TABS: readonly SensitiveTabMeta[] = [
  { key: 'all', label: '全部', kind: 'all' },
  { key: 'stored', label: '已入库', kind: 'stored' },
  { key: 'excluded', label: '已排除', kind: 'rejected' },
];

/** 页签 → 命令参数。**`excluded` 必须映射成 `rejected`**，后端只认后者。 */
export function sensitiveKind(tab: SensitiveTab): 'all' | 'stored' | 'rejected' {
  return SENSITIVE_TABS.find((t) => t.key === tab)?.kind ?? 'all';
}

/**
 * 本地再按 `kind` 过滤一次。
 *
 * 后端已按 `kind` 过滤，但列表是分页拉的、且删除后要就地更新，
 * 所以本地这份判定必须与后端口径**逐字一致**（`stored` / `rejected`）。
 */
export function matchesKind(record: SensitiveRecord, tab: SensitiveTab): boolean {
  const kind = sensitiveKind(tab);
  if (kind === 'all') return true;
  return record.kind === kind;
}

/** 记录的可搜索文本：公司 / 岗位 / 城市 / 原因。 */
export function recordText(record: SensitiveRecord): string {
  return [record.company, record.title, record.location, record.reason]
    .map((s) => (s ?? '').trim())
    .filter((s) => s.length > 0)
    .join(' · ');
}

/** 关键字命中：空关键字 = 不过滤；大小写与首尾空格都不敏感。 */
export function matchesKeyword(record: SensitiveRecord, keyword: string): boolean {
  const kw = keyword.trim().toLowerCase();
  if (kw === '') return true;
  return recordText(record).toLowerCase().includes(kw);
}

/** 页面可见列表 = 页签 + 关键字。 */
export function visibleRecords(
  records: readonly SensitiveRecord[],
  tab: SensitiveTab,
  keyword: string,
): SensitiveRecord[] {
  return records.filter((r) => matchesKind(r, tab) && matchesKeyword(r, keyword));
}

/** 全选按钮的三态：全选/取消全选是同一个按钮，靠 `checked` 是否已覆盖可见集判定。 */
export function allChecked(checked: readonly number[], visible: readonly SensitiveRecord[]): boolean {
  if (visible.length === 0) return false;
  return visible.every((r) => checked.includes(r.id));
}

/** 勾选/取消一条（保持点击顺序，便于删除时按顺序提示）。 */
export function toggleRecord(checked: readonly number[], id: number): number[] {
  if (checked.includes(id)) return checked.filter((x) => x !== id);
  return [...checked, id];
}

/** 删除按钮文案：`删除（3）`；未选时返回 `删除`（按钮此时是 disabled）。 */
export function deleteLabel(n: number): string {
  return n > 0 ? `删除（${n}）` : '删除';
}

/** 计数文案：`共 12 条`。 */
export function countLabel(total: number): string {
  return `共 ${Math.max(0, total)} 条`;
}

/** 删除后本地列表剔除已删 id（不重拉，避免滚动位置丢失）。 */
export function removeIds(records: readonly SensitiveRecord[], ids: readonly number[]): SensitiveRecord[] {
  if (ids.length === 0) return records.slice();
  const gone = new Set(ids);
  return records.filter((r) => !gone.has(r.id));
}
