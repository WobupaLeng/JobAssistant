/**
 * Liquid Glass 参数同源守卫（规格 5.4-⑯ / SPEC-003 裁定 4）。
 * 三方一致：lib_ui/glass.ts ThemeParams ↔ styles/tokens.css 折射令牌 ↔ index.html 静态 defs。
 * 改令牌不改常量（或改 defs）必须红。纯源码 / 常量断言，无 DOM 渲染（模块 §6）。
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  GLASS_DARK,
  GLASS_LIGHT,
  GLASS_L3,
  GLASS_MAX_FILTERED_CARDS,
  bevelSlope,
  clampDpr,
  computeGlassOffset,
  snellDeflection,
  squircleSDF,
} from '../src/lib_ui/glass';

const FRONTEND = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel: string) => readFileSync(join(FRONTEND, rel), 'utf8');
const tokens = read('styles/tokens.css');
const html = read('index.html');

describe('折射令牌真值仍在 tokens.css（只追加不改既有值）', () => {
  it('深浅 --refract-turbulence = 0.008 / 0.006', () => {
    expect(tokens).toMatch(/--refract-turbulence:\s*0\.008\s*;/);
    expect(tokens).toMatch(/--refract-turbulence:\s*0\.006\s*;/);
  });
  it('深浅 --refract-edge = 14px / 5px', () => {
    expect(tokens).toMatch(/--refract-edge:\s*14px\s*;/);
    expect(tokens).toMatch(/--refract-edge:\s*5px\s*;/);
  });
});

describe('ThemeParams 与令牌同源', () => {
  it('baseFrequency 等于 --refract-turbulence', () => {
    expect(GLASS_DARK.baseFrequency).toBe(0.008);
    expect(GLASS_LIGHT.baseFrequency).toBe(0.006);
  });
  it('seed 深浅相同（切主题图案不跳变）', () => {
    expect(GLASS_DARK.seed).toBe(11);
    expect(GLASS_LIGHT.seed).toBe(11);
  });
  it('scale = --refract-edge 的 10/14 折（深 14→10，浅 5→5）', () => {
    expect(GLASS_DARK.scale).toBe(10);
    expect(GLASS_LIGHT.scale).toBe(5);
  });
  it('numOctaves 深 3 / 浅 2', () => {
    expect(GLASS_DARK.numOctaves).toBe(3);
    expect(GLASS_LIGHT.numOctaves).toBe(2);
  });
});

describe('index.html 静态 defs 与 ThemeParams 逐字一致', () => {
  const filterBlock = (id: string) => {
    const m = html.match(new RegExp(`<filter id="${id}"[\\s\\S]*?</filter>`));
    return m ? m[0] : '';
  };
  it('lg-turb-dark 存在且参数照 GLASS_DARK', () => {
    const b = filterBlock('lg-turb-dark');
    expect(b).toContain('baseFrequency="0.008"');
    expect(b).toContain('numOctaves="3"');
    expect(b).toContain('seed="11"');
    expect(b).toContain('scale="10"');
    expect(b).toContain('color-interpolation-filters="sRGB"');
    expect(b).toContain('xChannelSelector="R"');
    expect(b).toContain('yChannelSelector="G"');
  });
  it('lg-turb-light 存在且参数照 GLASS_LIGHT', () => {
    const b = filterBlock('lg-turb-light');
    expect(b).toContain('baseFrequency="0.006"');
    expect(b).toContain('numOctaves="2"');
    expect(b).toContain('seed="11"');
    expect(b).toContain('scale="5"');
  });
});

describe('P0 反平移偏移纯函数（裁定 6：--glass-x/y = host - source + scroll）', () => {
  it('源在原点无滚动时偏移＝面板视口坐标', () => {
    expect(computeGlassOffset(720, 80, 0, 0, 0, 0)).toEqual({ x: 720, y: 80 });
  });
  it('滚动只改 y（反平移对齐，不重克隆）', () => {
    expect(computeGlassOffset(720, 80, 0, 0, 0, 200)).toEqual({ x: 720, y: 280 });
  });
  it('减去源的视口偏移（源不在窗口原点时仍对齐）', () => {
    expect(computeGlassOffset(720, 80, 16, 64, 0, 120)).toEqual({ x: 704, y: 136 });
  });
});

describe('P1 同屏折射预算常量（规格 L2-3 ≤12）', () => {
  it('GLASS_MAX_FILTERED_CARDS = 12', () => {
    expect(GLASS_MAX_FILTERED_CARDS).toBe(12);
  });
});

describe('L3 透镜光学常量（规格 §3.4 / REFERENCE §2）', () => {
  it('折射率 1.5 / 斜边 18px / 实例≤64 / dpr≤2 / 静止 500ms', () => {
    expect(GLASS_L3.refractiveIndex).toBe(1.5);
    expect(GLASS_L3.bevelWidth).toBe(18);
    expect(GLASS_L3.maxInstances).toBe(64);
    expect(GLASS_L3.dprCap).toBe(2);
    expect(GLASS_L3.idleStopMs).toBe(500);
  });
});

describe('L3 Snell 折射偏转（glass.ts ↔ GLSL 同源数学）', () => {
  it('零坡度零偏转', () => {
    expect(snellDeflection(0)).toBe(0);
  });
  it('正坡度偏转为正且随坡度单调增', () => {
    const a = snellDeflection(0.5);
    const b = snellDeflection(2);
    expect(a).toBeGreaterThan(0);
    expect(b).toBeGreaterThan(a);
  });
  it('n=1.5 永不全内反射（sinθt≤0.667<1）', () => {
    expect(snellDeflection(1e6, 1.5)).toBeLessThan(1);
  });
  it('n<1 大坡度触发全内反射返回 1.0', () => {
    expect(snellDeflection(100, 0.5)).toBe(1);
  });
});

describe('L3 squircle SDF + 法线（p=4 超椭圆）', () => {
  it('平底核心：dist 负、法线零（核心不折射）', () => {
    const s = squircleSDF(0, 0, 100, 60, 22);
    expect(s.dist).toBe(-60);
    expect(s.nx).toBe(0);
    expect(s.ny).toBe(0);
  });
  it('直边中点：dist≈0、法线指向外（+x）', () => {
    const s = squircleSDF(100, 0, 100, 60, 22);
    expect(Math.abs(s.dist)).toBeLessThan(1e-6);
    expect(s.nx).toBeCloseTo(1, 5);
    expect(s.ny).toBeCloseTo(0, 5);
  });
  it('圆角外角：dist 正（在形状外）', () => {
    const s = squircleSDF(110, 70, 100, 60, 22);
    expect(s.dist).toBeGreaterThan(0);
  });
  it('法线单位化（圆角带内模长≈1）', () => {
    const s = squircleSDF(95, 55, 100, 60, 22);
    expect(Math.hypot(s.nx, s.ny)).toBeCloseTo(1, 5);
  });
});

describe('L3 斜边坡度剖面 bevelSlope', () => {
  it('内缘 t=1 坡度归零（平底衔接）', () => {
    expect(bevelSlope(1)).toBe(0);
  });
  it('外缘 t=0 坡度最大且为正', () => {
    expect(bevelSlope(0)).toBeGreaterThan(bevelSlope(0.5));
    expect(bevelSlope(0)).toBeGreaterThan(0);
  });
  it('在 [0,1] 单调递减', () => {
    expect(bevelSlope(0.25)).toBeGreaterThan(bevelSlope(0.75));
  });
});

describe('L3 dpr 夹值（规格 §3.4 上限 2.0）', () => {
  it('超上限夹到 2', () => {
    expect(clampDpr(3)).toBe(2);
    expect(clampDpr(2.5)).toBe(2);
  });
  it('上限内原样', () => {
    expect(clampDpr(1.5)).toBe(1.5);
    expect(clampDpr(1)).toBe(1);
  });
  it('非有限 / 非正回退 1', () => {
    expect(clampDpr(0)).toBe(1);
    expect(clampDpr(-1)).toBe(1);
    expect(clampDpr(Number.NaN)).toBe(1);
  });
});