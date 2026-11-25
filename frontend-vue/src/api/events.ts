/**
 * Tauri 事件的**唯一**出口。非 Tauri 环境降级成 no-op 退订、不抛错；退订失败静默。
 * `login-state` 只在 `(platform, state)` 变化时播，是**旁路推送**——权威仍是
 * `login_status` 查询。
 */

/** `login-state` 事件的名称（冻结契约，后端 `lib.rs:530` 逐字）。 */
export const LOGIN_STATE_EVENT = 'login-state';

/** 后端 `LoginStatePayload`（`lib.rs:453`）。字段名是蛇形，别改成驼峰。 */
export interface LoginStatePayload {
  readonly platform: string;
  readonly state: string;
  readonly logged_in: boolean;
}

/** 事件回调：`null` = 载荷形状不认识（半截 JSON / 未来新增字段），上层应忽略。 */
export type LoginStateHandler = (payload: LoginStatePayload) => void;

/** 退订函数：断言式，重复调用安全。 */
export type UnlistenFn = () => void;

/** Tauri 2 事件相关的内部函数（只声明用到的两个）。 */
interface TauriEventInternals {
  invoke(cmd: string, args?: unknown): Promise<unknown>;
  transformCallback?(fn: (...args: unknown[]) => void, once?: boolean): number;
}

/** 没有任何监听可退订时的返回：降级路径与「已退订」共用。 */
const NOOP_UNLISTEN: UnlistenFn = () => {
  /* 降级路径：非 Tauri 环境没有可退订的东西 */
};

function internals(): TauriEventInternals | null {
  const w = (globalThis as unknown as { window?: unknown }).window;
  const box = (w ?? globalThis) as { __TAURI_INTERNALS__?: unknown };
  const v = box.__TAURI_INTERNALS__;
  if (!v || typeof v !== 'object') return null;
  const t = v as TauriEventInternals;
  return typeof t.invoke === 'function' ? t : null;
}

/** 当前是否真的跑在 Tauri WebView 里（有事件 API 才算）。 */
export function hasTauriEvents(): boolean {
  const t = internals();
  return t !== null && typeof t.transformCallback === 'function';
}

/**
 * 把任意形状的载荷收成 `LoginStatePayload`。载荷来自 IPC，是信任边界，不能用 `as`
 * 直接断言：缺 `state` 的当「不认识」丢掉，`logged_in` 缺省时按 `state` 现算。
 */
export function normalizeLoginState(raw: unknown): LoginStatePayload | null {
  if (!raw || typeof raw !== 'object') return null;
  const o = raw as Record<string, unknown>;
  const platform = typeof o.platform === 'string' ? o.platform : '';
  const state = typeof o.state === 'string' ? o.state : '';
  if (platform === '' && state === '') return null;
  if (state === '') return null;
  const loggedIn = typeof o.logged_in === 'boolean' ? o.logged_in : state === 'logged_in';
  return { platform, state, logged_in: loggedIn };
}

/**
 * 订阅 `login-state`：只在登记监听失败时 reject；回调异常就地转给 `onError`。
 */
export async function listenLoginState(
  handler: LoginStateHandler,
  onError?: (e: unknown) => void,
): Promise<UnlistenFn> {
  const t = internals();
  if (!t || typeof t.transformCallback !== 'function') return NOOP_UNLISTEN;

  let eventId: number | null = null;
  let closed = false;

  const callback = (event: unknown): void => {
    try {
      // Tauri 2 的回调参数是 `{ event, id, payload }`；兼容裸载荷。
      const box =
        event && typeof event === 'object' && 'payload' in (event as Record<string, unknown>)
          ? (event as Record<string, unknown>).payload
          : event;
      const payload = normalizeLoginState(box);
      if (payload) handler(payload);
    } catch (e) {
      onError?.(e);
    }
  };

  const handlerId = t.transformCallback(callback);
  eventId = (await t.invoke('plugin:event|listen', {
    event: LOGIN_STATE_EVENT,
    target: { kind: 'Any' },
    handler: handlerId,
  })) as number;

  return () => {
    if (closed) return;
    closed = true;
    if (eventId === null) return;
    void Promise.resolve(
      t.invoke('plugin:event|unlisten', { event: LOGIN_STATE_EVENT, eventId }),
    ).catch(() => undefined);
    eventId = null;
  };
}
