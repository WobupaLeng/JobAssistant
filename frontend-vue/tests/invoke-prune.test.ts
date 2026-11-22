/**
 * `api/invoke.ts` 的 `prune`：`undefined` 与 `null` 的语义分界（ARCH-003 §5.2）。
 *
 * 这是全项目最容易静默失效的一处：命令层的 `Option<T>` 字段大多没有
 * `#[serde(default)]`，显式 `null` 会被判成「缺 key」，而 `prune` 恰好
 * **保留 `null`**。两者混掉，只有到真实 WebView 里才会炸。
 */
import { describe, expect, it } from 'vitest';

import { prune } from '../src/api/invoke';

describe('invoke/prune', () => {
  it('删掉 undefined 键', () => {
    expect(prune({ a: 1, b: undefined })).toEqual({ a: 1 });
    expect(Object.keys(prune({ a: 1, b: undefined }))).toEqual(['a']);
  });

  it('保留 null（null 是有意义的载荷）', () => {
    const out = prune({ afterJobId: null, limit: null, useLlm: true });
    expect(out).toEqual({ afterJobId: null, limit: null, useLlm: true });
    expect('afterJobId' in out).toBe(true);
  });

  it('数组里的 undefined 压成 null，且不移动下标', () => {
    expect(prune([1, undefined, 3])).toEqual([1, null, 3]);
  });

  it('嵌套对象同样生效', () => {
    expect(prune({ args: { ids: [undefined], drop: undefined, keep: null } })).toEqual({
      args: { ids: [null], keep: null },
    });
  });

  it('标量 / 非普通对象原样返回', () => {
    expect(prune('x')).toBe('x');
    expect(prune(0)).toBe(0);
    expect(prune(false)).toBe(false);
    expect(prune(null)).toBeNull();
    const d = new Date(0);
    expect(prune(d)).toBe(d);
  });
});
