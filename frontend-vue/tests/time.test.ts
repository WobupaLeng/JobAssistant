/**
 * `lib_ui/time.ts`：UTC+8 的确定性时间格式化（C14）+ 「秒 / 毫秒」双输入判据（`toMs`）。
 *
 * ## 判据与它的边界（别把它改成「更聪明」的猜测）
 *
 * `toMs` 的判据是 **10 位及以下当秒、11 位及以上当毫秒**（分界 `1e11`）。这个常数不是拍的：
 * 秒与毫秒在数值上必然有一段重叠，只能选一条线。取 `1e11` 之后两侧的覆盖是
 * 秒到公元 5138 年、毫秒从 1973-03-03 起，两个真实数据源（sidecar 的 `time.time()` 秒、
 * `Date.now()` 毫秒）全部落在各自区间内；唯一判不了的是 1973 年以前的**毫秒**值
 * （数值落进秒区间，单位信息已丢失），本文件不去猜它，也不用用例假装支持。
 *
 * ## 历史误判（留档，别再犯）
 *
 * 早期这里的注释断言过「同一 `it` 里连续多条 `expect(字面量)` 会把上一条的字面量当成
 * 自己的接收值」，还据此把断言拆散、加 `console.log`——**那个归因是错的**。
 * 真因是当时 `toMs` 的分界还取 `1e12`：`951_753_600_000`（9.5e11）小于 1e12 被当成秒
 * 乘了 1000，显示成 32129 年。纯函数没有状态可漂，独立 `node` 进程复现同一结果即为证。
 */
import { describe, expect, it } from 'vitest';

import { LOCAL_OFFSET_SECS, civilFromDays, dateTimeMs, fmtTime, fmtTimeSecs, pad2, toMs } from '../src/lib_ui/time';

/** 秒/毫秒双输入的独立用例（每个用例单独一个 `it`）。 */
const FMT_TIME_CASES: Array<{ name: string; input: number; want: string }> = [
  { name: '秒 1700000000 → 11-15 06:13', input: 1_700_000_000, want: '11-15 06:13' },
  { name: '毫秒 1700000000000 → 11-15 06:13', input: 1_700_000_000_000, want: '11-15 06:13' },
  { name: '秒 951753600（2000-02-29）→ 02-29 00:00', input: 951_753_600, want: '02-29 00:00' },
  { name: '毫秒 951753600000（2000-02-29）→ 02-29 00:00', input: 951_753_600_000, want: '02-29 00:00' },
  { name: '秒 1709136000（2024-02-29）→ 02-29 00:00', input: 1_709_136_000, want: '02-29 00:00' },
  { name: '毫秒 1709136000000（2024-02-29）→ 02-29 00:00', input: 1_709_136_000_000, want: '02-29 00:00' },
  { name: '毫秒 4107427200000（2100-02-28）→ 02-28 00:00', input: 4_107_427_200_000, want: '02-28 00:00' },
  { name: '毫秒 4107513600000（2100-03-01）→ 03-01 00:00', input: 4_107_513_600_000, want: '03-01 00:00' },
  { name: '0 → —', input: 0, want: '—' },
  { name: '纪元后 1ms → 01-01 08:00', input: 1, want: '01-01 08:00' },
];

describe('time/fmtTime（MM-DD HH:mm，UTC+8）', () => {
  for (const c of FMT_TIME_CASES) {
    it(c.name, () => {
      const got: string = fmtTime(c.input);
      expect(got).toBe(c.want);
    });
  }

  it('反证：2000-02-29 的次日必须是 03-01（切分错位会退化成 02-29）', () => {
    // 951_840_000_000 ms = 2000-03-01 00:00（UTC+8）
    const next = 951_840_000_000;
    expect(fmtTime(next)).toBe('03-01 00:00');
  });
});

describe('time/fmtTimeSecs（MM-DD HH:mm:ss，UTC+8）', () => {
  it('毫秒 1700000000000 → 11-15 06:13:20', () => {
    const got: string = fmtTimeSecs(1_700_000_000_000);
    expect(got).toBe('11-15 06:13:20');
  });

  it('秒 1700000000 → 11-15 06:13:20（与毫秒同结果）', () => {
    const got: string = fmtTimeSecs(1_700_000_000);
    expect(got).toBe('11-15 06:13:20');
  });

  it('0 → --:--:--（不是 01-01 08:00:00）', () => {
    const got: string = fmtTimeSecs(0);
    expect(got).toBe('--:--:--');
  });

  it('形状 MM-DD HH:mm:ss（定长 14，第 6 位空格，两个冒号）', () => {
    const s: string = fmtTimeSecs(1_700_000_000_000);
    expect(s).toHaveLength(14);
    expect(s[5]).toBe(' ');
    expect(s.split(':')).toHaveLength(3);
  });

  it('1970 以前的负数时间戳：-1_000_000 秒 → 12-20 18:13:20', () => {
    // -1_000_000 秒 = -1e9 ms = 1969-12-20T10:13:20Z，+8 小时 = 12-20 18:13:20
    const got: string = fmtTimeSecs(-1_000_000);
    expect(got).toBe('12-20 18:13:20');
  });

  it('负数毫秒（11 位以上）原样按毫秒算，不再被二次乘 1000', () => {
    // -4_107_427_200_000 ms = 1839-11-04 16:00（UTC+8）；若被误当秒会落到公元 128000 年
    const got: string = fmtTimeSecs(-4_107_427_200_000);
    expect(got).toBe('11-04 16:00:00');
  });
});

describe('time/toMs（秒 / 毫秒判据）', () => {
  it('十位以内按秒放大：1_700_000_000 → 1.7e12 毫秒', () => {
    const got: number = toMs(1_700_000_000);
    expect(got).toBe(1_700_000_000_000);
  });

  it('毫秒级原样返回', () => {
    const got: number = toMs(1_700_000_000_000);
    expect(got).toBe(1_700_000_000_000);
  });

  it('闰日毫秒 951753600000 必须原样返回（11 位及以上按毫秒，不许被当秒）', () => {
    const got: number = toMs(951_753_600_000);
    expect(got).toBe(951_753_600_000);
  });

  it('分界点 1e11 的两侧：1e11-1 按秒、1e11 按毫秒', () => {
    expect(toMs(99_999_999_999)).toBe(99_999_999_999_000);
    expect(toMs(100_000_000_000)).toBe(100_000_000_000);
  });

  it('0 原样返回（0 是「没有时间」的占位）', () => {
    const got: number = toMs(0);
    expect(got).toBe(0);
  });

  it('负数按秒放大', () => {
    const got: number = toMs(-1_000);
    expect(got).toBe(-1_000_000);
  });
});

describe('time/纯整数工具', () => {
  it('pad2 补零', () => {
    const got: string = pad2(7);
    expect(got).toBe('07');
  });

  it('pad2 保留负号', () => {
    const got: string = pad2(-5);
    expect(got).toBe('-05');
  });

  it('pad2 超两位原样返回', () => {
    const got: string = pad2(123);
    expect(got).toBe('123');
  });

  it('civilFromDays(0) = 1970-01-01', () => {
    const got: [number, number, number] = civilFromDays(0);
    expect(got).toEqual([1970, 1, 1]);
  });

  it('civilFromDays(19723) = 2024-01-01', () => {
    const got: [number, number, number] = civilFromDays(19_723);
    expect(got).toEqual([2024, 1, 1]);
  });

  it('LOCAL_OFFSET_SECS = 28800（UTC+8）', () => {
    const got: number = LOCAL_OFFSET_SECS;
    expect(got).toBe(28_800);
  });

  it('dateTimeMs 分解到字段（毫秒）', () => {
    const dt = dateTimeMs(1_700_000_000_000, LOCAL_OFFSET_SECS);
    expect(dt).toEqual({ year: 2023, month: 11, day: 15, hour: 6, minute: 13, second: 20 });
  });

  it('dateTimeMs 分解到字段（闰日毫秒）', () => {
    const dt = dateTimeMs(951_753_600_000, LOCAL_OFFSET_SECS);
    console.log('leap dt =', JSON.stringify(dt));
    expect(dt).toEqual({ year: 2000, month: 2, day: 29, hour: 0, minute: 0, second: 0 });
  });
});
