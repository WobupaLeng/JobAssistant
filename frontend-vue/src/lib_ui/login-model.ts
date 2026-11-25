/**
 * 登录态的纯视图模型（平台卡状态点 + 「保存登录 / 退出」按钮派生）。两条来源优先级不同：
 * `login-state` 事件是旁路推送，`login_status` 查询是权威。所以 **`mergeLoginState`：
 * 未知态不覆盖已知态**，否则推送迟到时「已登录」会退化成「未知」。
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
 * 浏览器 / Sidecar **根本没连上**（而不是「连上了但没登录」）：sidecar 不在线时后端回
 * `unknown` + `running: false`。库里有登录标记（`saved === true`）时必须和真·未登录区分开，
 * 否则对用户说「请先登录」是误导。
 */
export function isDetached(state: string | null | undefined, saved?: boolean | null): boolean {
  const s = (state ?? '').trim().toLowerCase();
  if (s !== 'unknown' && s !== '') return false;
  return saved === true;
}

/**
 * 事件里的 `state` 是否「已知」。`'unknown'` 是前端合成的占位（从没查过），
 * 事件真带上来了也当未知处理，不许它覆盖权威查询结果。
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
 * 合并「查询到的权威状态」与「事件推来的状态」，顺序即优先级：无权威时用事件填一份最小
 * 状态（`running: true`）；事件态未知则原样返回权威；平台不同则忽略；其余保留权威的
 * `running` / `saved` / `message`，只换 `state`。纯函数，绝不就地改入参。
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
 * 徽标，口径与 `platform-model.loginDot` 一致，只多给一个 `title`。
 * **双态**：`state === 'unknown'` 且 `saved === true`（标记在、浏览器不在线）时点仍为 ok，
 * label 改成「已登录·浏览器未连接」，避免与聊天页的「浏览器未连接」自相矛盾。
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
  // 双态：标记在、浏览器不在。覆盖上面所有分支——它描述的是整体结论，不是某条原因。
  const detached = state === 'unknown' && status?.saved === true;
  const dotLabel = detached ? '已登录·浏览器未连接' : loginDotLabel(dot);
  if (detached) title = '本地已保存登录；浏览器未连接，点「扫码登录」或「开始抓取」后生效';
  return { dot, dotClass: loginDotClass(dot), dotLabel, title };
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
 * 「保存登录」：只在**已登录且本次会话还没存过**时出现（存过再点没有意义），
 * 正在忙时禁用，防连点。
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
 * 二维码在「登录窗口已打开」时贴到平台卡里（扫描门控按钮之后），窗口关了就撤掉。
 * 载荷用 `login_status.message` 里的 data URI，没有就不画。
 */
export function scanFlagVisible(state: string | null | undefined): boolean {
  return isWaiting(state);
}
