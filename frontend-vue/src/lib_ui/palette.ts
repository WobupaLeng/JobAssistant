/**
 * 调色板：主题色联动。只改「强调色」一族令牌（主色、辅助色、主按钮与高亮文字），
 * 不碰布局几何。色值写在 `<html>` 行内 style 上覆盖 tokens.css 默认值，所以深浅切换后
 * 依然生效。存储键 `aja.palette`。
 */

export const PALETTE_KEY = 'aja.palette';

export type PaletteId = 'emerald' | 'ocean' | 'violet' | 'amber' | 'rose';

interface PaletteTone {
  primary: string;
  primaryStrong: string;
  primarySoft: string;
  primaryBorder: string;
  primaryText: string;
  accent: string;
  accentSoft: string;
  glassTint: string;
  glassTintStrong: string;
  /** 页面背景的主色染色（径向光斑），随调色板变化。 */
  bgWash: string;
}

export interface PaletteDef {
  id: PaletteId;
  label: string;
  /** 调色板圆点的展示色（取深色主色）。 */
  swatch: string;
  dark: PaletteTone;
  light: PaletteTone;
}

export const PALETTES: readonly PaletteDef[] = [
  {
    id: 'emerald',
    label: '翡翠',
    swatch: '#10b981',
    dark: {
      primary: '#10b981',
      primaryStrong: '#059669',
      primarySoft: 'rgba(16, 185, 129, 0.16)',
      primaryBorder: 'rgba(16, 185, 129, 0.4)',
      primaryText: '#6ee7b7',
      accent: '#2563eb',
      accentSoft: 'rgba(37, 99, 235, 0.18)',
      glassTint: 'rgba(16, 185, 129, 0.05)',
      glassTintStrong: 'rgba(16, 185, 129, 0.10)',
      bgWash: 'rgba(16, 185, 129, 0.22)',
    },
    light: {
      primary: '#10b981',
      primaryStrong: '#059669',
      primarySoft: 'rgba(16, 185, 129, 0.14)',
      primaryBorder: 'rgba(16, 185, 129, 0.4)',
      primaryText: '#047857',
      accent: '#2563eb',
      accentSoft: 'rgba(37, 99, 235, 0.12)',
      glassTint: 'rgba(16, 185, 129, 0.04)',
      glassTintStrong: 'rgba(16, 185, 129, 0.08)',
      bgWash: 'rgba(16, 185, 129, 0.14)',
    },
  },
  {
    id: 'ocean',
    label: '海蓝',
    swatch: '#0ea5e9',
    dark: {
      primary: '#0ea5e9',
      primaryStrong: '#0284c7',
      primarySoft: 'rgba(14, 165, 233, 0.16)',
      primaryBorder: 'rgba(14, 165, 233, 0.4)',
      primaryText: '#7dd3fc',
      accent: '#6366f1',
      accentSoft: 'rgba(99, 102, 241, 0.18)',
      glassTint: 'rgba(14, 165, 233, 0.05)',
      glassTintStrong: 'rgba(14, 165, 233, 0.10)',
      bgWash: 'rgba(14, 165, 233, 0.22)',
    },
    light: {
      primary: '#0ea5e9',
      primaryStrong: '#0284c7',
      primarySoft: 'rgba(14, 165, 233, 0.14)',
      primaryBorder: 'rgba(14, 165, 233, 0.4)',
      primaryText: '#0369a1',
      accent: '#6366f1',
      accentSoft: 'rgba(99, 102, 241, 0.12)',
      glassTint: 'rgba(14, 165, 233, 0.04)',
      glassTintStrong: 'rgba(14, 165, 233, 0.08)',
      bgWash: 'rgba(14, 165, 233, 0.14)',
    },
  },
  {
    id: 'violet',
    label: '紫罗兰',
    swatch: '#8b5cf6',
    dark: {
      primary: '#8b5cf6',
      primaryStrong: '#7c3aed',
      primarySoft: 'rgba(139, 92, 246, 0.16)',
      primaryBorder: 'rgba(139, 92, 246, 0.42)',
      primaryText: '#c4b5fd',
      accent: '#ec4899',
      accentSoft: 'rgba(236, 72, 153, 0.18)',
      glassTint: 'rgba(139, 92, 246, 0.06)',
      glassTintStrong: 'rgba(139, 92, 246, 0.11)',
      bgWash: 'rgba(139, 92, 246, 0.24)',
    },
    light: {
      primary: '#8b5cf6',
      primaryStrong: '#7c3aed',
      primarySoft: 'rgba(139, 92, 246, 0.14)',
      primaryBorder: 'rgba(139, 92, 246, 0.42)',
      primaryText: '#6d28d9',
      accent: '#ec4899',
      accentSoft: 'rgba(236, 72, 153, 0.12)',
      glassTint: 'rgba(139, 92, 246, 0.04)',
      glassTintStrong: 'rgba(139, 92, 246, 0.08)',
      bgWash: 'rgba(139, 92, 246, 0.14)',
    },
  },
  {
    id: 'amber',
    label: '琥珀',
    swatch: '#f59e0b',
    dark: {
      primary: '#f59e0b',
      primaryStrong: '#d97706',
      primarySoft: 'rgba(245, 158, 11, 0.16)',
      primaryBorder: 'rgba(245, 158, 11, 0.42)',
      primaryText: '#fcd34d',
      accent: '#f97316',
      accentSoft: 'rgba(249, 115, 22, 0.18)',
      glassTint: 'rgba(245, 158, 11, 0.05)',
      glassTintStrong: 'rgba(245, 158, 11, 0.10)',
      bgWash: 'rgba(245, 158, 11, 0.20)',
    },
    light: {
      primary: '#f59e0b',
      primaryStrong: '#d97706',
      primarySoft: 'rgba(245, 158, 11, 0.14)',
      primaryBorder: 'rgba(245, 158, 11, 0.42)',
      primaryText: '#92400e',
      accent: '#f97316',
      accentSoft: 'rgba(249, 115, 22, 0.12)',
      glassTint: 'rgba(245, 158, 11, 0.04)',
      glassTintStrong: 'rgba(245, 158, 11, 0.08)',
      bgWash: 'rgba(245, 158, 11, 0.13)',
    },
  },
  {
    id: 'rose',
    label: '玫瑰',
    swatch: '#f43f5e',
    dark: {
      primary: '#f43f5e',
      primaryStrong: '#e11d48',
      primarySoft: 'rgba(244, 63, 94, 0.16)',
      primaryBorder: 'rgba(244, 63, 94, 0.42)',
      primaryText: '#fda4af',
      accent: '#a855f7',
      accentSoft: 'rgba(168, 85, 247, 0.18)',
      glassTint: 'rgba(244, 63, 94, 0.05)',
      glassTintStrong: 'rgba(244, 63, 94, 0.10)',
      bgWash: 'rgba(244, 63, 94, 0.20)',
    },
    light: {
      primary: '#f43f5e',
      primaryStrong: '#e11d48',
      primarySoft: 'rgba(244, 63, 94, 0.14)',
      primaryBorder: 'rgba(244, 63, 94, 0.42)',
      primaryText: '#be123c',
      accent: '#a855f7',
      accentSoft: 'rgba(168, 85, 247, 0.12)',
      glassTint: 'rgba(244, 63, 94, 0.04)',
      glassTintStrong: 'rgba(244, 63, 94, 0.08)',
      bgWash: 'rgba(244, 63, 94, 0.13)',
    },
  },
];

const DEFAULT_PALETTE: PaletteId = 'emerald';

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

/**
 * 把调色板写成 `<html>` 的行内 CSS 变量，覆盖 tokens.css 的强调色默认值。
 * 深浅取对应分支，因此主题切换后再调用一次即可跟随。
 */
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
    st.setProperty('--glass-tint', tone.glassTint);
    st.setProperty('--glass-tint-strong', tone.glassTintStrong);
    st.setProperty('--palette-swatch', def.swatch);
    document.documentElement.setAttribute('data-palette', def.id);
  } catch {
    /* 无 document（单测环境）：忽略 */
  }
  savePalette(def.id);
}
