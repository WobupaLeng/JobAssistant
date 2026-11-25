/**
 * 调色板：主题色联动。只改「强调色」一族令牌（主色、辅助色、主按钮与高亮文字）
 * 与外壳染色，不碰布局几何。色值写在 `<html>` 行内 style 上覆盖 tokens.css
 * 默认值，所以深浅切换后依然生效。存储键 `aja.palette`。
 */

export const PALETTE_KEY = 'aja.palette';

export type PaletteId = 'cobalt' | 'teal' | 'moss' | 'marigold' | 'coral' | 'magenta' | 'iris';

interface PaletteTone {
  primary: string;
  primaryStrong: string;
  primarySoft: string;
  primaryBorder: string;
  primaryText: string;
  /** 主按钮文字色：按按钮渐变最暗端算对比度，不随深浅一刀切。 */
  primaryFg: string;
  accent: string;
  accentSoft: string;
  /** 外壳左上角的主色径向光斑，随调色板变化。 */
  bgWash: string;
}

export interface PaletteDef {
  id: PaletteId;
  label: string;
  /** 调色板圆点的展示色（取可辨识的亮色，不一定是按钮主色）。 */
  swatch: string;
  dark: PaletteTone;
  light: PaletteTone;
}

export const PALETTES: readonly PaletteDef[] = [
  {
    id: 'cobalt',
    label: '钴蓝',
    swatch: '#3b82f6',
    dark: {
      primary: '#2563eb',
      primaryStrong: '#1d4ed8',
      primarySoft: 'rgba(37, 99, 235, 0.16)',
      primaryBorder: 'rgba(37, 99, 235, 0.42)',
      primaryText: '#93c5fd',
      primaryFg: '#ffffff',
      accent: '#06b6d4',
      accentSoft: 'rgba(6, 182, 212, 0.18)',
      bgWash: 'rgba(37, 99, 235, 0.22)',
    },
    light: {
      primary: '#2563eb',
      primaryStrong: '#1d4ed8',
      primarySoft: 'rgba(37, 99, 235, 0.12)',
      primaryBorder: 'rgba(37, 99, 235, 0.4)',
      primaryText: '#1e40af',
      primaryFg: '#ffffff',
      accent: '#0891b2',
      accentSoft: 'rgba(8, 145, 178, 0.12)',
      bgWash: 'rgba(37, 99, 235, 0.14)',
    },
  },
  {
    id: 'teal',
    label: '青碧',
    swatch: '#14b8a6',
    dark: {
      primary: '#14b8a6',
      primaryStrong: '#0d9488',
      primarySoft: 'rgba(20, 184, 166, 0.16)',
      primaryBorder: 'rgba(20, 184, 166, 0.42)',
      primaryText: '#5eead4',
      primaryFg: '#04120c',
      accent: '#38bdf8',
      accentSoft: 'rgba(56, 189, 248, 0.18)',
      bgWash: 'rgba(20, 184, 166, 0.20)',
    },
    light: {
      primary: '#0f766e',
      primaryStrong: '#115e59',
      primarySoft: 'rgba(15, 118, 110, 0.12)',
      primaryBorder: 'rgba(15, 118, 110, 0.4)',
      primaryText: '#115e59',
      primaryFg: '#ffffff',
      accent: '#0284c7',
      accentSoft: 'rgba(2, 132, 199, 0.12)',
      bgWash: 'rgba(15, 118, 110, 0.13)',
    },
  },
  {
    id: 'moss',
    label: '苔绿',
    swatch: '#84cc16',
    dark: {
      primary: '#84cc16',
      primaryStrong: '#65a30d',
      primarySoft: 'rgba(132, 204, 22, 0.16)',
      primaryBorder: 'rgba(132, 204, 22, 0.42)',
      primaryText: '#bef264',
      primaryFg: '#04120c',
      accent: '#2dd4bf',
      accentSoft: 'rgba(45, 212, 191, 0.18)',
      bgWash: 'rgba(132, 204, 22, 0.20)',
    },
    light: {
      primary: '#4d7c0f',
      primaryStrong: '#3f6212',
      primarySoft: 'rgba(77, 124, 15, 0.12)',
      primaryBorder: 'rgba(77, 124, 15, 0.38)',
      primaryText: '#3f6212',
      primaryFg: '#ffffff',
      accent: '#0f766e',
      accentSoft: 'rgba(15, 118, 110, 0.12)',
      bgWash: 'rgba(77, 124, 15, 0.12)',
    },
  },
  {
    id: 'marigold',
    label: '金盏',
    swatch: '#f59e0b',
    dark: {
      primary: '#f59e0b',
      primaryStrong: '#d97706',
      primarySoft: 'rgba(245, 158, 11, 0.16)',
      primaryBorder: 'rgba(245, 158, 11, 0.42)',
      primaryText: '#fcd34d',
      primaryFg: '#04120c',
      accent: '#fb7185',
      accentSoft: 'rgba(251, 113, 133, 0.18)',
      bgWash: 'rgba(245, 158, 11, 0.20)',
    },
    light: {
      primary: '#b45309',
      primaryStrong: '#92400e',
      primarySoft: 'rgba(180, 83, 9, 0.12)',
      primaryBorder: 'rgba(180, 83, 9, 0.4)',
      primaryText: '#92400e',
      primaryFg: '#ffffff',
      accent: '#e11d48',
      accentSoft: 'rgba(225, 29, 72, 0.12)',
      bgWash: 'rgba(180, 83, 9, 0.13)',
    },
  },
  {
    id: 'coral',
    label: '珊瑚',
    swatch: '#fb7185',
    dark: {
      primary: '#fb7185',
      primaryStrong: '#f43f5e',
      primarySoft: 'rgba(251, 113, 133, 0.16)',
      primaryBorder: 'rgba(251, 113, 133, 0.42)',
      primaryText: '#fda4af',
      primaryFg: '#04120c',
      accent: '#a855f7',
      accentSoft: 'rgba(168, 85, 247, 0.18)',
      bgWash: 'rgba(251, 113, 133, 0.20)',
    },
    light: {
      primary: '#e11d48',
      primaryStrong: '#be123c',
      primarySoft: 'rgba(225, 29, 72, 0.12)',
      primaryBorder: 'rgba(225, 29, 72, 0.4)',
      primaryText: '#be123c',
      primaryFg: '#ffffff',
      accent: '#7e22ce',
      accentSoft: 'rgba(126, 34, 206, 0.12)',
      bgWash: 'rgba(225, 29, 72, 0.13)',
    },
  },
  {
    id: 'magenta',
    label: '洋红',
    swatch: '#d946ef',
    dark: {
      primary: '#c026d3',
      primaryStrong: '#a21caf',
      primarySoft: 'rgba(192, 38, 211, 0.16)',
      primaryBorder: 'rgba(192, 38, 211, 0.42)',
      primaryText: '#f0abfc',
      primaryFg: '#ffffff',
      accent: '#fb7185',
      accentSoft: 'rgba(251, 113, 133, 0.18)',
      bgWash: 'rgba(192, 38, 211, 0.22)',
    },
    light: {
      primary: '#a21caf',
      primaryStrong: '#86198f',
      primarySoft: 'rgba(162, 28, 175, 0.12)',
      primaryBorder: 'rgba(162, 28, 175, 0.4)',
      primaryText: '#86198f',
      primaryFg: '#ffffff',
      accent: '#be123c',
      accentSoft: 'rgba(190, 18, 60, 0.12)',
      bgWash: 'rgba(162, 28, 175, 0.13)',
    },
  },
  {
    id: 'iris',
    label: '鸢尾',
    swatch: '#8b5cf6',
    dark: {
      primary: '#7c3aed',
      primaryStrong: '#6d28d9',
      primarySoft: 'rgba(124, 58, 237, 0.16)',
      primaryBorder: 'rgba(124, 58, 237, 0.42)',
      primaryText: '#c4b5fd',
      primaryFg: '#ffffff',
      accent: '#d946ef',
      accentSoft: 'rgba(217, 70, 239, 0.18)',
      bgWash: 'rgba(124, 58, 237, 0.24)',
    },
    light: {
      primary: '#6d28d9',
      primaryStrong: '#5b21b6',
      primarySoft: 'rgba(109, 40, 217, 0.12)',
      primaryBorder: 'rgba(109, 40, 217, 0.4)',
      primaryText: '#5b21b6',
      primaryFg: '#ffffff',
      accent: '#a21caf',
      accentSoft: 'rgba(162, 28, 175, 0.12)',
      bgWash: 'rgba(109, 40, 217, 0.13)',
    },
  },
];

const DEFAULT_PALETTE: PaletteId = 'cobalt';

export function findPalette(id: string | null | undefined): PaletteDef {
  return PALETTES.find((p) => p.id === id) ?? PALETTES[0];
}

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** 读取已保存的调色板；缺失或非法时回退默认。 */
export function readPalette(): PaletteId {
  const s = storage();
  if (!s) return DEFAULT_PALETTE;
  try {
    const v = s.getItem(PALETTE_KEY);
    return findPalette(v).id;
  } catch {
    return DEFAULT_PALETTE;
  }
}

function savePalette(id: PaletteId): void {
  const s = storage();
  if (!s) return;
  try {
    s.setItem(PALETTE_KEY, id);
  } catch {
    /* 配额满 / 禁用：忽略 */
  }
}

export const FONT_SCALE_KEY = 'aja.font_scale';

/** 文字大小档位：乘在 `--ui-scale` 上，作用于全部 `--fs-*` 令牌。 */
export const FONT_SCALES = [
  { id: 's', label: '小', value: 0.92 },
  { id: 'm', label: '标准', value: 1 },
  { id: 'l', label: '大', value: 1.1 },
] as const;
export type FontScaleId = (typeof FONT_SCALES)[number]['id'];

export function readFontScale(): FontScaleId {
  const s = storage();
  if (!s) return 'm';
  try {
    const v = s.getItem(FONT_SCALE_KEY);
    return FONT_SCALES.some((f) => f.id === v) ? (v as FontScaleId) : 'm';
  } catch {
    return 'm';
  }
}

export function saveFontScale(id: FontScaleId): void {
  const s = storage();
  if (!s) return;
  try {
    s.setItem(FONT_SCALE_KEY, id);
  } catch {
    /* 忽略 */
  }
}

export function applyFontScale(id: FontScaleId): void {
  const v = FONT_SCALES.find((f) => f.id === id)?.value ?? 1;
  try {
    document.documentElement.style.setProperty('--font-scale', String(v));
  } catch {
    /* 无 document：忽略 */
  }
  saveFontScale(id);
}

/**
 * 把调色板写成 `<html>` 的行内 CSS 变量，覆盖 tokens.css 的强调色默认值。
 * 深浅取对应分支，因此主题切换后再调用一次即可跟随。
 */
export function applyPalette(id: PaletteId, theme: 'dark' | 'light'): void {
  const def = findPalette(id);
  const tone = theme === 'light' ? def.light : def.dark;
  try {
    const st = document.documentElement.style;
    st.setProperty('--primary', tone.primary);
    st.setProperty('--primary-strong', tone.primaryStrong);
    st.setProperty('--primary-soft', tone.primarySoft);
    st.setProperty('--primary-border', tone.primaryBorder);
    st.setProperty('--primary-text', tone.primaryText);
    st.setProperty('--accent', tone.accent);
    st.setProperty('--accent-soft', tone.accentSoft);
    st.setProperty('--palette-wash', tone.bgWash);
    st.setProperty('--btn-primary-fg', tone.primaryFg);
    st.setProperty('--btn-primary-border', tone.primaryBorder);
    st.setProperty('--palette-swatch', def.swatch);
    document.documentElement.setAttribute('data-palette', def.id);
  } catch {
    /* 无 document（单测环境）：忽略 */
  }
  savePalette(def.id);
}
