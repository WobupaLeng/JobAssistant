/**
 * Tauri 命令调用的**唯一**出口（AGENTS.md C1）。
 *
 * 为什么不用 `@tauri-apps/api`：前端与 Rust 同进程，浏览器里的调用约定只有
 * 一个 `window.__TAURI_INTERNALS__.invoke(cmd, args)`。引官方包只会多一层
 * 依赖与版本耦合，且它内部也是同一个调用。这里手写薄封装，并把参数形状
 * 在 `src/api/commands/**` 里按命令逐条钉死。
 *
 * **两个调用器 + 一个清洗器**：
 * - `invokeArgs(cmd, args)`：命令层签名是 `fn foo(state, args: FooArgs)`，
 *   载荷要包一层 `{ args }`；
 * - `invokeFlat(cmd, payload)`：命令层签名是平铺参数，键名与参数名同名；
 * - `prune(value)`：删掉值为 `undefined` 的键、**保留 `null`**。
 *
 * 为什么 `prune` 不能用 `JSON.stringify` 后处理：`JSON.stringify` 会把 `null`
 * 和 `undefined` 一起丢掉（前者被对象序列化保留，`undefined` 被删），
 * 更关键的是它还会顺手展开 `toJSON()`、丢掉 `undefined` 之外的一切控制权。
 * 这里要的语义恰好相反——`null` 是**有意义的载荷**（`rescore_jobs` 的
 * `afterJobId: null` 表示「从头开始」，`Option<T>` 的 `Some(None)` 与缺 key
 * 是两回事），`undefined` 才是「别发这个键」。
 *
 * **为什么必须 prune**：命令层的 `Option<T>` 字段大多**没有** `#[serde(default)]`，
 * 显式 `null` 会被判成「缺 key」，报
 * `invalid args 'args' for command 'list_jobs': command list_jobs missing required key args`
 * （ARCH-003 §5.2）。这条错误只在真实运行时出现，类型检查抓不到。
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
 * 递归删掉值为 `undefined` 的键（数组里的 `undefined` 元素压成 `null`，
 * 保持下标不移位——数组位置有意义时，删除会让后续元素前移一位）。
 *
 * **保留 `null`**：见文件头的说明。`NaN` / `Infinity` 原样保留（JSON 会变成
 * `null`，但那是后端契约问题，不该在客户端悄悄改写）。
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

/**
 * 调一个「包一层 `args`」的命令：`fn foo(state, args: FooArgs)`。
 *
 * 载荷恒为 `{ args: <prune(args)> }`——即使 `args` 为空，`args` 这一层也必须存在，
 * 否则命令层会报缺 key。
 */
export async function invokeArgs<T>(cmd: string, args: unknown): Promise<T> {
  const payload = { args: prune(args ?? {}) };
  const raw = await tauriInvoke(cmd, payload).catch((e: unknown) => {
    throw new Error(errorText(e));
  });
  return raw as T;
}

/**
 * 调一个「平铺参数」的命令：`fn foo(state, job_id: String)`。
 *
 * 键名与 Rust 参数名同名；需要驼峰的由命令层 `#[serde(alias)]` 兜住，
 * 具体用哪个名字在 `src/api/commands/**` 里逐条写明，**不要在这里统一转换**。
 */
export async function invokeFlat<T>(cmd: string, payload: unknown = {}): Promise<T> {
  const raw = await tauriInvoke(cmd, prune(payload)).catch((e: unknown) => {
    throw new Error(errorText(e));
  });
  return raw as T;
}

/**
 * 无载荷命令：`fn foo(state)` 之类，载荷恒为 `{}`。
 *
 * 注意**不要**改传 `null` / `undefined`：Tauri 对 `null` 会当成「没有参数对象」，
 * 命令层若声明了 `state` 之外的无默认参数也不会报缺 key，属于「看起来能跑」，
 * 但一旦以后加了参数就是静默失效。
 */
export function invokeNone<T>(cmd: string): Promise<T> {
  return invokeFlat<T>(cmd, {});
}
