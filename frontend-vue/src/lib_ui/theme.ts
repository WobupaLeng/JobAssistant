/**
 * 主题：深色为默认，`html[data-theme='light']` 覆盖设计令牌。取值顺序是
 * 已保存 > 系统偏好 > 深色，必须与 `index.html` 首帧脚本逐条一致——首帧脚本在 Vue
 * 挂载前就打好了 `data-theme`，否则会先闪一帧深色再切浅色。
 */

export const THEME_KEY = 'aja.theme';

export type Theme = 'dark' | 'light';

export function flipTheme(theme: Theme): Theme {
  return theme === 'dark' ? 'light' : 'dark';
}

/** storage 在 WebView 里可能被禁用（隐私模式 / 策略），一律 `try` 住。 */
function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.localStorage;
  } catch {
    return null;
  }
}

/** 初始主题：已保存的选择 > 系统偏好 > 深色（毛玻璃在深色下观感最好）。 */
export function readInitialTheme(): Theme {
  const s = storage();
  if (s) {
    try {
      const v = s.getItem(THEME_KEY);
      if (v !== null && v !== undefined) return v === 'light' ? 'light' : 'dark';
    } catch {
      /* 读失败：退回系统偏好 */
    }
  }
  // WebView 禁用 storage 时退回系统偏好；没有 matchMedia 的环境一律深色
  try {
    const prefersLight =
      typeof window !== 'undefined' &&
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-color-scheme: light)').matches;
    return prefersLight ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

/** 把主题写到根元素并持久化。写失败不影响渲染（WebView 可能禁用 storage）。 */
export function applyTheme(theme: Theme): void {
  try {
    document.documentElement.setAttribute('data-theme', theme);
  } catch {
    /* 无 document（单测环境）：忽略，CSS 默认就是深色 */
  }
  const s = storage();
  if (s) {
    try {
      s.setItem(THEME_KEY, theme);
    } catch {
      /* 配额满 / 禁用：忽略 */
    }
  }
}

/**
 * 首帧：**只读**当前主题，不写 storage。判据与首帧脚本一致；落盘会把「从没选过主题」
 * 写成一次显式选择，之后系统切浅色就跟不动了。
 */
export function bootstrapTheme(): Theme {
  const theme = readInitialTheme();
  try {
    document.documentElement.setAttribute('data-theme', theme);
  } catch {
    /* 无 document：CSS 默认深色 */
  }
  return theme;
}
