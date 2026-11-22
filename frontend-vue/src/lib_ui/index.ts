/**
 * `lib_ui` 的依赖图最底层：纯函数与常量，不引用 api / pages / features / components
 * （frontend-vue/AGENTS.md §2 的 R2 规则，机器可检）。任何上层都可以引它，
 * 它**不许**反向引用任何上层。
 */

export * from './clock';
export * from './job';
export * from './logfmt';
export * from './motion';
export * from './nav';
export * from './options';
export * from './session';
export * from './text';
export * from './theme';
export * from './time';
export * from './auto-model';
export * from './chat-model';
export * from './job-list-model';
export * from './longpress';
export * from './scrape-filter-model';
export * from './scrape-panel-model';
export {
  SENSITIVE_TABS,
  allChecked,
  countLabel,
  deleteLabel,
  matchesKind,
  recordText,
  removeIds,
  sensitiveKind,
  toggleRecord,
  visibleRecords,
  type SensitiveTab,
  type SensitiveTabMeta,
} from './sensitive-model';
