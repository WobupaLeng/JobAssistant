/**
 * 层标识与进入副作用（二级导航已退役）。
 *
 * **顶栏不再有「概览 | Boss 消息 | 配置 | 决策记录」这一行页签**，依据三处，不重复裁决：
 * - 冻结基线 [`UI-SPEC-legacy-baseline.md`](../../docs/ui/UI-SPEC-legacy-baseline.md) §9 Q-1
 *   「用户裁决：删除二级导航」与 §7-K4（结构树明写**无** `nav.page-tabs`）；
 * - 原型 `prototypes/ui-v1/index.html` L1413「用户裁决 5：二级导航已移除」
 *   —— Boss 消息在右栏、配置在顶栏右侧、决策记录在底部日志卡头部，入口都还在，只是不再单起一行；
 * - ADR-004：聊天不再是整页，`chat` 这个页标识随之消失，页签锚点 `nav-<slug>` 一并退役
 *   （决策在代码侧的溯源注释只有一处，在改造入口 `App.vue`；本文件只引用条款号）。
 *
 * 剩下的两件事：
 * 1. **层标识**：概览常挂，只有 `config` / `logs` 是叠在概览之上的浮层窗；
 *    锚点助手 `navTestId` / `pageTestId` 与 `nav-<slug>` / `page-<slug>` 一起退役
 *    （SPEC-001 AC9「命中 0」）：概览容器锚点照原型改成 `shell-dashboard`
 *    （`prototypes/ui-v1/index.html` L1418），两个浮层窗的锚点本来就是它自己的
 *    `config-panel` / `decisions-panel`，页面不再重复挂一层页锚点。
 * 2. **进入副作用表** [`NAV_ENTER`]：决定「进这一层要不要重拉数据」。放在这里而不是各层里，
 *    是为了让「哪些层需要新鲜数据」一眼可见。概览恒为 `false`：它自己按 5 秒轮询页签条数，
 *    再叠一层「进入即重拉」会让每次切层都打出两条重复请求（旧版踩过，ARCH-003 §5.6 同源不变量）。
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

