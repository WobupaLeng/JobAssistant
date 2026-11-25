/**
 * `commands_config.rs` 的 4 条命令。配置写入分两条路（AGENTS.md C11）：
 * **门槛与上限**只走 `set_limit`（塞进 `set_app_config` 会被静默跳过）；
 * 其余键走 `set_app_config`，且**只提交 diff**。
 */

import { invokeArgs, invokeFlat, invokeNone } from '../invoke';
import type {
  AppConfigReply,
  LimitReply,
  SaveConfigReply,
  ScrapeDefaultsArgs,
  ScrapeDefaultsReply,
} from '../types';

/**
 * `set_app_config` 的 patch 里**永远不许出现**的键：`min_score`（只能走 `set_limit`，
 * 塞进来会被静默跳过）与 `api_key_hint`（`****abcd` 掩码，回写等于把密钥改成掩码）。
 */
export const CONFIG_PATCH_FORBIDDEN_KEYS: readonly string[] = ['min_score', 'api_key_hint'];

/**
 * 从 patch 里剔掉不许出现的键，返回**新对象**。`setAppConfig` 内部同样经过它，
 * 「门槛只能走 set_limit」由此单点保证。导出仅为便于单测。
 */
export function sanitizeConfigPatch(patch: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(patch)) {
    if (CONFIG_PATCH_FORBIDDEN_KEYS.includes(k)) continue;
    if (v === undefined) continue;
    out[k] = v;
  }
  return out;
}

/** 读配置。**没有 `api_key`**，只有 `api_key_hint`（尾 4 位掩码）。 */
export function getAppConfig(): Promise<AppConfigReply> {
  return invokeNone<AppConfigReply>('get_app_config');
}

/**
 * 保存配置。**只提交 diff**，载荷平铺 `{ patch }`（不是 `args` 层），不许出现的键会被
 * `sanitizeConfigPatch` 剔掉。顺序要求（C11）：先 `set_app_config` 再 `set_limit`。
 */
export function setAppConfig(patch: Record<string, unknown>): Promise<SaveConfigReply> {
  return invokeFlat<SaveConfigReply>('set_app_config', { patch: sanitizeConfigPatch(patch) });
}

/**
 * 抓取筛选面板的默认条件。载荷 `{ args }`，键名驼峰，**只提交变化过的字段**——
 * 缺席的键在 Rust 侧是 `None`（= 不动），不会被空值覆盖。
 */
export function setScrapeDefaults(args: ScrapeDefaultsArgs): Promise<ScrapeDefaultsReply> {
  return invokeArgs<ScrapeDefaultsReply>('set_scrape_defaults', args);
}

/**
 * 写「有效 JD 上限」与「打分门槛」。载荷 `{ args: { maxJd?, minScore? } }`，**有值才加键**。
 * 回执是后端裁决后的值，调用方要用它覆盖本地草稿。
 */
export function setLimit(maxJd?: number | null, minScore?: number | null): Promise<LimitReply> {
  return invokeArgs<LimitReply>('set_limit', {
    maxJd: maxJd ?? undefined,
    minScore: minScore ?? undefined,
  });
}
