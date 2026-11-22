/**
 * `commands_sensitive.rs` 的 2 条命令（敏感库）。
 *
 * `sensitive_list` 是 Rust 侧唯一带默认值的驼峰无关键（`SensitiveListArgs`
 * 五个字段全是 `Option` 或 `#[serde(default)]`），但**载荷仍然要包 `args`**：
 * 它照旧是 `fn sensitive_list(state, args: SensitiveListArgs)`。
 */

import { invokeArgs } from '../invoke';
import type { RemoveReply, SensitivePage } from '../types';

/**
 * 列敏感库条目。
 *
 * 载荷结构：`{ args: { platform?, keyword?, kind?, limit?, offset? } }`；
 * 五个键**有值才加**（`prune` 负责丢掉 `undefined`）。
 * `kind` 取 `all` / `stored` / `rejected`，拼错时后端按 `all` 处理（不报错）。
 */
export function sensitiveList(params: {
  platform?: string | null;
  keyword?: string | null;
  kind?: string | null;
  limit?: number | null;
  offset?: number | null;
} = {}): Promise<SensitivePage> {
  return invokeArgs<SensitivePage>('sensitive_list', {
    platform: params.platform ?? undefined,
    keyword: params.keyword ?? undefined,
    kind: params.kind ?? undefined,
    limit: params.limit ?? undefined,
    offset: params.offset ?? undefined,
  });
}

/**
 * 把所选条目移出敏感库，返回**实际删除**的条数。
 *
 * 载荷结构：`{ args: { ids } }`。**空数组直接短路返回 0、不发 IPC**：
 * 后端也会 dedupe + 丢非正 id 后回 0，但「什么都没选」不该跑一次 SQL。
 */
export async function sensitiveRemove(ids: readonly number[]): Promise<number> {
  if (ids.length === 0) return 0;
  const reply = await invokeArgs<RemoveReply>('sensitive_remove', { ids: [...ids] });
  return reply.deleted;
}
