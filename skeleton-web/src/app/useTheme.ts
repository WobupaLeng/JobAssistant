/**
 * 主题状态：初值读取与持久化都委托给 `lib/theme`，本文件只管 React 侧。
 */

import { useCallback, useEffect, useState } from 'react';
import { applyTheme, readInitialTheme, type Theme } from '../lib/theme';

export function useTheme(): { theme: Theme; toggle: () => void } {
  const [theme, setTheme] = useState<Theme>(() => readInitialTheme());

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
  }, []);

  return { theme, toggle };
}
