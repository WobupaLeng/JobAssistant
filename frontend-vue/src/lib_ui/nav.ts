/**
 * 层标识与进入副作用。没有二级导航：概览常挂，只有 `config` / `logs` 是叠在概览之上的浮层窗，
 * 锚点用各自的 `config-panel` / `decisions-panel`。`NAV_ENTER` 决定「进这一层要不要重拉数据」，
 * 概览恒为 `false`：它自己按 5 秒轮询页签条数，再叠一层会打出重复请求。
 */

export type Nav = 'dashboard' | 'config' | 'logs';

/** 全部层标识，顺序即 DOM 层次顺序。 */
export const NAV_ALL: readonly Nav[] = ['dashboard', 'config', 'logs'];

/** 进入该层是否触发重拉 / 副作用。概览恒为 `false`，见文件头第 2 条。 */
export function entersSideEffect(nav: Nav): boolean {
  return nav !== 'dashboard';
}

/** 有副作用的层集合（顺序同 [`NAV_ALL`]）。 */
export const NAV_ENTER: readonly Nav[] = NAV_ALL.filter(entersSideEffect);

