/**
 * 无边框窗口控件：最小化 / 最大化切换 / 关闭。
 *
 * 命令名逐字对应 `backend-tauri/src/lib.rs` 的 `window_minimize` /
 * `window_toggle_maximize` / `window_close`（C1：只经 invoke，不碰 @tauri-apps/api）。
 * 三者都无参数，走 `invokeNone`。
 */

import { invokeNone } from '../invoke';

export function windowMinimize(): Promise<void> {
  return invokeNone('window_minimize');
}

export function windowToggleMaximize(): Promise<void> {
  return invokeNone('window_toggle_maximize');
}

export function windowClose(): Promise<void> {
  return invokeNone('window_close');
}
