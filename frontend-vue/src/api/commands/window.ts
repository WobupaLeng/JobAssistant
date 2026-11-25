/**
 * 无边框窗口控件：最小化 / 最大化切换 / 关闭。三者都无参数，走 `invokeNone`；
 * **只经 invoke，不碰 @tauri-apps/api**（C1）。
 */

import { invokeFlat, invokeNone } from '../invoke';

export function windowMinimize(): Promise<void> {
  return invokeNone('window_minimize');
}

export function windowToggleMaximize(): Promise<void> {
  return invokeNone('window_toggle_maximize');
}

export function windowClose(): Promise<void> {
  return invokeNone('window_close');
}

/** 把当前主题告诉 Rust：窗口模糊底色分深浅两档，深色档是深蓝黑。 */
export function setWindowTheme(theme: 'dark' | 'light'): Promise<void> {
  return invokeFlat('set_window_theme', { theme });
}
