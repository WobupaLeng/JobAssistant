/**
 * 平台卡（顶栏三个平台的勾选 / 状态 / 登录）的纯视图模型。
 *
 * 真源：冻结原型 `prototypes/ui-v1/index.html` 的 `platform-*` 骨架
 * （`platform-boss-card` / `platform-boss-check` / `platform-boss-status` /
 * `platform-boss-login-btn`，另两个平台 `is-off` + 「未接入」）
 * 与 C15 的常量表 `lib_ui/options.ts::PLATFORMS`（只有一份）。
 *
 * 不在本文件里的东西：真正拉登录窗口的命令（那是编排层的事）。
 */
import { PLATFORMS, type PlatformOpt } from './options';

/** 平台在顶栏里的三种可用性。 */
export type PlatformMode = 'ready' | 'off';

/** 卡片的一层视图。 */
export interface PlatformCardView {
  readonly id: string;
  readonly name: string;
  readonly testId: string;
  readonly checkTestId: string;
  readonly statusTestId: string;
  readonly loginTestId: string;
  readonly mode: PlatformMode;
  readonly cardClass: string;
  readonly title: string;
  readonly checked: boolean;
  readonly loginVisible: boolean;
  readonly loginLabel: string;
  /** 登录按钮禁用原因（`null` = 可点）。原型里 Boss 是「登录功能接线中」，本轮接线后放开。 */
  readonly loginDisabled: boolean;
  readonly loginTitle: string;
}

/** Boss 直聘的 id：只有它有登录/抓取链路（其余两个是占位）。 */
export const PRIMARY_PLATFORM = 'boss';

/** 当前选中的平台（抓取只跑一个）。 */
export function platformIds(): string[] {
  return PLATFORMS.map((p) => p.id);
}

/** 卡片 testid 的公共前缀，三个平台各一份。 */
export function platformPrefix(id: string): string {
  return `platform-${id}`;
}

/**
 * 单张卡的视图。
 *
 * `checked`：只有选中的平台打勾；未接入的平台恒不勾（原型里它们的 checkbox 是 disabled）。
 * 登录按钮只在「已接入且被选中」时出现——没选中的平台点了登录也没意义。
 *
 * **已登录时登录按钮变成不可点的「已登录」**（用户 m02848）：登录态是权威查询结果，
 * 已登录还留着可点的「登录」会让用户以为要重复扫码。这里把 `loggedIn` 作为入参，
 * 由调用方从 `LoginStatus.state` 取（口径 `login-model.isLoggedIn`），本函数只做视图。
 * 「退出登录 / 保存登录」是另一个按钮，仍由 `login-model` 派生，不受此影响。
 */
export function platformCardView(
  p: PlatformOpt,
  selectedId: string,
  loginBusy: boolean,
  loggedIn = false,
): PlatformCardView {
  const ready = p.ready;
  const checked = ready && p.id === selectedId;
  const prefix = platformPrefix(p.id);
  return {
    id: p.id,
    name: p.name,
    testId: `${prefix}-card`,
    checkTestId: `${prefix}-check`,
    statusTestId: `${prefix}-status`,
    loginTestId: `${prefix}-login-btn`,
    mode: ready ? 'ready' : 'off',
    cardClass: ready ? 'platform-card' : 'platform-card is-off',
    title: ready ? p.name : '未接入',
    checked,
    loginVisible: ready && checked,
    loginLabel: loggedIn ? '已登录' : loginBusy ? '登录中…' : '登录',
    // 已登录恒禁用（不因 busy 与否放开）；未登录时只有 busy 禁用。
    loginDisabled: loggedIn || loginBusy,
    loginTitle: loggedIn
      ? '当前已登录；需要换账号请先点「退出登录」'
      : ready
        ? '打开 Boss 登录窗口扫码'
        : '未接入',
  };
}

/** 三张卡。 */
export function platformCards(
  selectedId: string,
  loginBusy: boolean,
  loggedIn = false,
): PlatformCardView[] {
  return PLATFORMS.map((p) => platformCardView(p, selectedId, loginBusy, loggedIn));
}

/** 点勾选：只有已接入的平台能被选中，且不允许取消成"无平台"。 */
export function pickPlatform(selectedId: string, id: string): string {
  const p = PLATFORMS.find((x) => x.id === id);
  if (!p || !p.ready) return selectedId;
  return id;
}

/**
 * 登录状态点：`aria-label` 与状态类。
 *
 * 状态来自后端 `login_status`（`state` / `running` / `saved`）。没查过时是
 * `unknown`——不猜"已登录"，免得给用户假的安心。
 */
export type LoginDot = 'ok' | 'bad' | 'unknown';

export interface LoginStatusLike {
  readonly state?: string;
  readonly running?: boolean;
  readonly saved?: boolean;
}

/**
 * 颜色只由登录状态决定，与 `running` 无关（运行中也照样判绿/红）。
 * - 已登录（state 为 ok/ready/logged_in，或 saved===true）→ ok
 * - 明确未登录（state 为 no_login/logged_out 等已知非登录值）→ bad
 * - state 缺失/空/unknown 且无 saved 信息（没查过）→ unknown，不猜
 */
export function loginDot(status: LoginStatusLike | null | undefined): LoginDot {
  if (!status) return 'unknown';
  const state = (status.state ?? '').toLowerCase();
  if (state === 'ok' || state === 'ready' || state === 'logged_in') return 'ok';
  if (status.saved === true) return 'ok';
  // 扫码进行中（waiting）尚无结论：灰，不误报红。与 running 无关，只看 state。
  if (state === '' || state === 'unknown' || state === 'waiting') return 'unknown';
  return 'bad';
}

export function loginDotClass(dot: LoginDot): string {
  return dot === 'ok' ? 'status-dot is-ok' : dot === 'bad' ? 'status-dot is-bad' : 'status-dot';
}

export function loginDotLabel(dot: LoginDot): string {
  switch (dot) {
    case 'ok':
      return '已登录';
    case 'bad':
      return '未登录';
    default:
      return '登录状态未知';
  }
}
