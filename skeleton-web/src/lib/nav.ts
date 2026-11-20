/**
 * 顶层导航：标识、顺序、进入副作用。
 *
 * 放在 `lib/` 而不是 `app/` 的原因（`tools/check_layers.py` 会拦住这个错误）：
 * `Nav` 被两层同时使用——`app/` 负责渲染按钮，`pages/` 负责按它分发页面。
 * 若定义在 `app/`，`pages/` 就得反过来 import 上层，依赖方向立刻成环。
 * 这类「被多处引用的纯常量与类型」属于底层共享契约，归 `lib`。
 */

export type Nav = 'dashboard' | 'chat' | 'config' | 'logs';

export const NAV: { key: Nav; name: string }[] = [
  { key: 'dashboard', name: '概览' },
  { key: 'chat', name: 'Boss 消息' },
  { key: 'config', name: '配置' },
  { key: 'logs', name: '决策记录' },
];

/**
 * 进入某页时副作用的集中声明。
 *
 * 为什么要显式化：早先 `go()` 里内联写了「进 logs 刷新、进 chat 刷新并拉起浏览器」
 * 两条 if，加一页就得再补一条；而这类「进入页面即需要新鲜数据」的规则散落在
 * 组装层，光看页面代码是发现不了的。
 */
export const NAV_ENTER: Record<Nav, 'none' | 'refresh-logs' | 'refresh-chat'> = {
  dashboard: 'none',
  chat: 'refresh-chat',
  config: 'none',
  logs: 'refresh-logs',
};
