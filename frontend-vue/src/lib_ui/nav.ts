/**
 * 导航：四个页面 + 进入时的副作用表。
 *
 * [`NAV_ENTER`] 决定「切到这个页面要不要重拉数据」，放在这里而不是各页面里，
 * 是为了让「哪些页需要新鲜数据」一眼可见：概览页不订阅 tick、不重拉，
 * 其余三页进入即刷新。
 */

export type Nav = 'dashboard' | 'chat' | 'config' | 'logs';

/** 全部页面，顺序即顶栏顺序。 */
export const NAV_ALL: readonly Nav[] = ['dashboard', 'chat', 'config', 'logs'];

/**
 * 进入该页是否触发重拉 / 副作用。
 *
 * **概览页恒为 `false`**：它自己按 5 秒轮询页签条数，再叠一层「进入即重拉」
 * 会让每次切页都打出两条重复请求（旧版就踩过）。
 */
export function entersSideEffect(nav: Nav): boolean {
  return nav !== 'dashboard';
}

/** 有副作用的页面集合（顺序同 [`NAV_ALL`]）。 */
export const NAV_ENTER: readonly Nav[] = NAV_ALL.filter(entersSideEffect);

export interface NavItem {
  key: Nav;
  name: string;
}

export const NAV: readonly NavItem[] = [
  { key: 'dashboard', name: '概览' },
  { key: 'chat', name: 'Boss 消息' },
  { key: 'config', name: '配置' },
  { key: 'logs', name: '决策记录' },
];

/**
 * 顶栏导航块的 class 名。
 *
 * `AppShell` 把「导航页签 + 敏感库入口」一起包进 `.nav`：敏感库入口要**紧挨着**
 * 「决策记录」（用户裁决），而 `.spacer` 会把它们推成隔着半个顶栏的两个孤岛。
 * 高度由 CSS 的 `.nav { display: contents }` 决定，仍然和 `.topbar` 的其余项
 * 处在同一个 flex 行上。
 */
export const NAV_BLOCK_CLASS = 'nav';

/**
 * `data-testid` 前缀，E2E 依赖这两个串：
 * 顶栏页签 `nav-<slug>`，页面容器 `page-<slug>`。
 */
export function navTestId(nav: Nav): string {
  return `nav-${nav}`;
}

export function pageTestId(nav: Nav): string {
  return `page-${nav}`;
}
