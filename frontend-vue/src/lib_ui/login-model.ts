/**
 * 登录态的纯视图模型（顶栏平台卡的状态点 + 「保存登录 / 退出」按钮派生）。
 *
 * 两条来源、优先级不同（冻结契约，后端 `lib.rs:441-535`）：
 * 1. **事件** `login-state`：旁路推送，500ms 粒度，只在 `(platform, state)` 变化时播；
 * 2. **查询** `login_status`：权威。挂载 / WebView 恢复 / 事件后都要以它为准。
 *
 * 所以这里的关键规则是 **`mergeLoginState`：未知态不覆盖已知态**。
 * 事件只知道 `(platform, state, logged_in)`，不知道 `running` / `saved` / `message`；
 * 若直接把它当成一份完整 `LoginStatus` 塞进界面，会让「已登录」在推送迟到时
 * 退化成「未知」，等于把权威信息丢了。合并规则逐条写在函数注释里。
 *
 * 真源口径复用 `lib_ui/platform-model.ts` 的 `loginDot`（三态 ok/bad/unknown），
 * 这里只做「登录态 → 徽标 + 按钮」这一层，不重复定义颜色与点。
 */
import { loginDot, loginDotClass, loginDotLabel, type LoginDot } from './platform-model';
import type { LoginStatePayload } from '../api/events';
import type { LoginStatus } from '../api/types';

/** 后端 state 全集的字面量（`lib.rs:463-500` 的 `login_snapshot` / `login_emit_decision`）。 */
export const LOGIN_STATES = ['waiting', 'logged_in', 'closed', 'error', 'no_login'] as const;

/** 需要用户继续操作的三个态：可以点「保存登录」。 */
const SAVEABLE: readonly string[] = ['logged_in'];

/** 已登录 —— 可以让用户「退出登录」。 */
export function isLoggedIn(state: string | null | undefined): boolean {
  return (state ?? '').toLowerCase() === 'logged_in';
}

/** 扫码窗口还开着 / 登录进行中（文案提示「等待扫码」）。 */
export function isWaiting(state: string | null | undefined): boolean {
  return (state ?? '').toLowerCase() === 'waiting';
}

/** 登录失败：`error` 是唯一真失败态，`no_login` 只是「没有登录会话」。 */
export function isLoginError(state: string | null | undefined): boolean {
  return (state ?? '').toLowerCase() === 'error';
}

/**
 * 事件里的 `state` 是否「已知」。
 *
 * `'unknown'` 是前端合成的占位（从没查过），事件永远不该带它；
 * 真带上来了也当未知处理，不许它去覆盖权威查询结果。
 */
export function isKnownState(state: string | null | undefined): boolean {
  const s = (state ?? '').trim().toLowerCase();
  return s !== '' && s !== 'unknown';
}

/** 事件 → 部分 `LoginStatus`（只填它真正知道的字段）。 */
export function loginStatusFromEvent(
  payload: LoginStatePayload,
): Pick<LoginStatus, 'platform' | 'state'> {
  return { platform: payload.platform, state: payload.state };
}

/**
 * 合并「查询到的权威状态」与「事件推来的状态」。
 *
 * 规则（顺序即优先级）：
 * 1. 没有权威状态（`current === null`）：用事件填一份最小状态，
 *    `running: true`（事件只会在会话活着时播）、`saved: false`、`message: ''`；
 * 2. 事件态未知（空 / `unknown`）：**原样返回 authoritative**，不动；
 * 3. 事件平台与权威平台不同：忽略（别的平台的变化不该改当前徽标）；
 * 4. 其余：保留权威的 `running` / `saved` / `message`，只换 `state`
 *    （`platform` 保持权威的，避免大小写/别名差异把键换掉）。
 *
 * 纯函数，返回新对象，绝不就地改入参。
 */
export function mergeLoginState(
  authoritative: LoginStatus | null | undefined,
  payload: LoginStatePayload | null | undefined,
): LoginStatus | null {
  if (!payload || !isKnownState(payload.state)) return authoritative ?? null;
  if (!authoritative) {
    return {
      platform: payload.platform,
      state: payload.state,
      running: true,
      message: '',
      saved: false,
    };
  }
  const samePlatform =
    payload.platform === '' ||
    authoritative.platform === '' ||
    payload.platform === authoritative.platform;
  if (!samePlatform) return authoritative;
  if (authoritative.state === payload.state) return authoritative;
  return {
    platform: authoritative.platform,
    state: payload.state,
    running: authoritative.running,
    message: authoritative.message,
    saved: authoritative.saved,
  };
}

/** 徽标一层：点（ok/bad/unknown）+ 类名 + aria-label 文案。 */
export interface LoginBadge {
  readonly dot: LoginDot;
  readonly dotClass: string;
  readonly dotLabel: string;
  /** 状态点的 title：登录失败时带上后端原话，不吞错误。 */
  readonly title: string;
}

/**
 * 徽标。口径与 `platform-model.loginDot` 完全一致（那条分支里 `running === true`
 * → `unknown`，所以「扫码中」显示为灰点而不是红点）；这里只多给一个 `title`。
 */
export function loginBadge(status: LoginStatus | null | undefined): LoginBadge {
  const dot = loginDot(status);
  const state = (status?.state ?? '').toLowerCase();
  const message = (status?.message ?? '').trim();
  let title = loginDotLabel(dot);
  if (isWaiting(state)) title = '等待扫码';
  else if (isLoginError(state)) title = message === '' ? '登录失败' : `登录失败：${message}`;
  else if (state === 'closed') title = '登录窗口已关闭';
  else if (state === 'no_login') title = '尚未登录';
  else if (message !== '' && dot !== 'ok') title = message;
  return { dot, dotClass: loginDotClass(dot), dotLabel: loginDotLabel(dot), title };
}

/** 一个按钮的派生：显不显示、能不能点、点了是哪个命令。 */
export interface LoginActionView {
  readonly testId: string;
  readonly label: string;
  readonly btnClass: string;
  readonly visible: boolean;
  readonly disabled: boolean;
  readonly title: string;
}

/**
 * 「保存登录」：只在**已登录且本次会话还没存过**时出现。
 *
 * 为什么要 `saved`：`login_save` 是把「已登录」落库的只写命令，已经存过的再点
 * 一次没有意义（后端 `LoginSaveReply.saved` 也只是回 false），所以按钮直接收起来。
 * 正在忙（上一次还没回来）时禁用，防连点。
 */
export function saveLoginAction(
  status: LoginStatus | null | undefined,
  busy: boolean,
): LoginActionView {
  const state = status?.state ?? '';
  const visible = isLoggedIn(state) && status?.saved !== true;
  return {
    testId: 'platform-login-save-btn',
    label: busy ? '保存中…' : '保存登录',
    btnClass: 'btn btn-sm btn-ghost',
    visible,
    disabled: busy || !visible,
    title: '把「已登录」记到本机配置，下次启动不必重新扫码',
  };
}

/**
 * 「退出登录」：已登录时出现。`running` 的会话点退出会顺带关掉 sidecar 浏览器
 * （后端 `LogoutReply.closed`），所以按钮一直可点——没会话时后端只是清标记。
 */
export function logoutAction(
  status: LoginStatus | null | undefined,
  busy: boolean,
): LoginActionView {
  const visible = isLoggedIn(status?.state ?? '');
  return {
    testId: 'platform-logout-btn',
    label: busy ? '退出中…' : '退出登录',
    btnClass: 'btn btn-sm btn-ghost',
    visible,
    disabled: busy || !visible,
    title: status?.running === true ? '退出登录并关闭浏览器会话' : '清除本机登录标记',
  };
}

/** 状态文案（顶栏 `title` 与无障碍播报用）。 */
export function loginStateLabel(state: string | null | undefined): string {
  switch ((state ?? '').toLowerCase()) {
    case 'logged_in':
      return '已登录';
    case 'waiting':
      return '等待扫码';
    case 'closed':
      return '登录窗口已关闭';
    case 'error':
      return '登录失败';
    case 'no_login':
      return '尚未登录';
    default:
      return '登录状态未知';
  }
}

/**
 * 二维码在「登录窗口已打开」时贴到平台卡里，窗口关了就撤掉。
 *
 * 这是 `resolve-scan-flag` 的语义落点：冻结规格
 * `docs/ui/UI-SPEC-chat-detach-v3.md:166` 只在 §4.2「沿用」清单里列了这个名字，
 * 原型与全部历史代码都没有实现（`git log --all -S` 只命中冻结规格那一次提交），
 * 所以位置由本轮的登录接线确定：**挂在平台卡内、扫描门控按钮之后**。
 * 载荷用 `login_status.message` 里的 data URI（没有就不画）。
 */
export function scanFlagVisible(state: string | null | undefined): boolean {
  return isWaiting(state);
}
