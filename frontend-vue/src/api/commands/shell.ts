/**
 * `lib.rs` 里那两条**不在 `commands_*.rs`** 的命令：`get_scoring_spec` 与 `app_info`。
 * 实现直接写在 `lib.rs`（一个返回私有 `ScoringSpec`，一个要注入 `PathResolver`），
 * 都没有入参；命令模块按 Rust 源文件归属划分，所以单独成一份。
 */

import { invokeNone } from '../invoke';
import type { AppInfo, ScoringSpec } from '../types';

/**
 * 满分与默认门槛的**唯一来源**（AGENTS.md C10）：界面上任何「/100」「低于 70」之类的
 * 数字都必须从这里取，组件里硬编码即违约。
 */
export function getScoringSpec(): Promise<ScoringSpec> {
  return invokeNone<ScoringSpec>('get_scoring_spec');
}

/** 应用名、版本与数据目录；顶栏与「打开数据目录」用。无入参。 */
export function appInfo(): Promise<AppInfo> {
  return invokeNone<AppInfo>('app_info');
}
