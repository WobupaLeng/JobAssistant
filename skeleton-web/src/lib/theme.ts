/**
 * 主题：深色为默认，`[data-theme='light']` 覆盖设计令牌。
 *
 * 独立成文件的原因不只是「少两行」：初始主题的取值顺序（已保存 > 系统偏好 > 深色）
 * 曾是 `App.tsx` 里的一段内联逻辑，且在预览环境里出过「永远锁死深色」的缺陷。
 * 抽出来之后，`prefers-color-scheme` 分支可以被单独检视。
 */

export type Theme = 'dark' | 'light';

export const THEME_KEY = 'aja.theme';

/** 初始主题：已保存的选择 > 系统偏好 > 深色（毛玻璃在深色下观感最好）。 */
export function readInitialTheme(): Theme {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === 'dark' || saved === 'light') return saved;
  } catch {
    /* WebView 禁用 storage 时退回系统偏好 */
  }
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

/** 把主题写到根元素并持久化。写失败不影响渲染。 */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    /* 存不下就算了 */
  }
}
