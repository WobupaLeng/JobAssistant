/**
 * 全景几何的 Vue 接线：窗口 resize → `syncCanvas()`（`--ui-scale` 与 `--detach-w` 的写入
 * 顺序由它内部保证）。两处细节：拖动窗口时 resize 每像素都发，用
 * `requestAnimationFrame` 合并到一帧，免得整页重排；首帧在 `onMounted` 立刻跑一次。
 */
import { onBeforeUnmount, onMounted } from 'vue';

import { syncCanvas } from './canvas';

export interface CanvasSyncHandle {
  /** 立刻同步一次（读当前 `innerWidth`）。在 resize 之后、或首帧强制刷新时用。 */
  sync: () => void;
  /** 撤掉监听并丢掉挂起的帧。重复调用无害。 */
  stop: () => void;
}

/**
 * 挂在组件生命周期上的接线：挂载时同步一次并监听 resize，
 * 卸载时摘监听、取消挂起的 rAF。
 */
export function useCanvasScale(): CanvasSyncHandle {
  let frame = 0;
  let listening = false;

  const apply = (): void => {
    frame = 0;
    syncCanvas(window.innerWidth);
  };

  const onResize = (): void => {
    if (frame !== 0) return;
    frame = window.requestAnimationFrame(apply);
  };

  const sync = (): void => {
    if (frame !== 0) {
      window.cancelAnimationFrame(frame);
      frame = 0;
    }
    apply();
  };

  const stop = (): void => {
    if (frame !== 0) {
      window.cancelAnimationFrame(frame);
      frame = 0;
    }
    if (listening) {
      window.removeEventListener('resize', onResize);
      listening = false;
    }
  };

  onMounted(() => {
    sync();
    if (!listening) {
      window.addEventListener('resize', onResize);
      listening = true;
    }
  });

  onBeforeUnmount(stop);

  return { sync, stop };
}
