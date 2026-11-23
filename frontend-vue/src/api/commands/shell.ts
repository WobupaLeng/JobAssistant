/**
 * `lib.rs` 里那两条**不在 `commands_*.rs`** 的命令：`get_scoring_spec` 与 `app_info`。
 *
 * 它们在 `generate_handler!` 里与其余 46 条并列注册（`lib.rs:522-571`），
 * 但实现直接写在 `lib.rs`：`get_scoring_spec` 返回私有的 `ScoringSpec`，
 * `app_info` 要求注入 `tauri::PathResolver`。两者都没有输入参数。
 *
 * 为什么单独一份文件而不是塞进 `db.ts`：按 Rust 源文件归属划分命令模块，
 * 44 条 + 3 条 job-flag（动态命令名）+ 这 2 条 = 48 条，与 `generate_handler!` 逐条对齐。
 */

import { invokeNone } from '../invoke';
import type { AppInfo, ScoringSpec } from '../types';

/**
 * 满分与默认门槛的**唯一来源**（AGENTS.md C10）。
 *
 * 界面上任何「/100」「低于 70」之类的数字都必须由调用方从这里取，
 * 组件里硬编码即违约。返回 `{ score_max, score_min_default }`。
 */
export function getScoringSpec(): Promise<ScoringSpec> {
  return invokeNone<ScoringSpec>('get_scoring_spec');
}

/** 应用名、版本与数据目录；顶栏与「打开数据目录」用。无入参。 */
export function appInfo(): Promise<AppInfo> {
  return invokeNone<AppInfo>('app_info');
}
