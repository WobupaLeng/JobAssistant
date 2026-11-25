/**
 * Tauri 命令调用的**唯一**出口：`invokeArgs` 包一层 `{ args }`，`invokeFlat` 平铺，参数形状在
 * `src/api/commands/**` 逐条钉死。`prune` 删 `undefined`、**保留 `null`**——`null` 是有意义的
 * 载荷（`afterJobId: null` = 从头开始），但多送会被命令层当成缺 key。
 */

/** Tauri 2 注入到 `window` 上的 IPC 入口（浏览器里唯一可用的调用约定）。 */
interface TauriInternals {
  invoke(cmd: string, args?: unknown): Promise<unknown>;
}

function tauriInvoke(cmd: string, payload: unknown): Promise<unknown> {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: TauriInternals })
    .__TAURI_INTERNALS__;
  if (!internals || typeof internals.invoke !== 'function') {
    return Promise.reject(
      new Error(`__TAURI_INTERNALS__ 不可用：${cmd} 只能在 Tauri WebView 里调用`),
    );
  }
  return internals.invoke(cmd, payload);
}

/** 把 JS 异常转成可读字符串：命令层 `Result<T, String>` 的 Err 会原样抛出来。 */
function errorText(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  try {
    return String(e);
  } catch {
    return '未知错误';
  }
}

/** 是否值得递归清洗：普通对象与数组才处理，其余（含 `Date` / `Map`）原样返回。 */
function isPlainObject(v: unknown): v is Record<string, unknown> {
  if (typeof v !== 'object' || v === null) return false;
  const proto = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null;
}

/**
 * 递归删掉值为 `undefined` 的键；数组里的 `undefined` 压成 `null` 以免下标移位。
 * **保留 `null`**，`NaN` / `Infinity` 原样保留。
 */
export function prune<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map((v) => (v === undefined ? null : prune(v))) as unknown as T;
  }
  if (isPlainObject(value)) {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (v === undefined) continue;
      out[k] = prune(v);
    }
    return out as unknown as T;
  }
  return value;
}

/** 调「包一层 `args`」的命令：载荷恒为 `{ args: prune(args) }`，即使为空也在。 */
export async function invokeArgs<T>(cmd: string, args: unknown): Promise<T> {
  const payload = { args: prune(args ?? {}) };
  const raw = await tauriInvoke(cmd, payload).catch((e: unknown) => {
    throw new Error(errorText(e));
  });
  return raw as T;
}

/** 调平铺参数的命令：键名与 Rust 参数名同名，**不要在这里统一转换驼峰**。 */
export async function invokeFlat<T>(cmd: string, payload: unknown = {}): Promise<T> {
  const raw = await tauriInvoke(cmd, prune(payload)).catch((e: unknown) => {
    throw new Error(errorText(e));
  });
  return raw as T;
}

/** 无载荷命令，载荷恒为 `{}`。**不要**改传 `null`，否则加参数后会静默失效。 */
export function invokeNone<T>(cmd: string): Promise<T> {
  return invokeFlat<T>(cmd, {});
}
