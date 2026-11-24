/**
 * 卡片高光边随光源变化（iOS 风格）。
 *
 * 光源取指针位置：`main.ts` 每次 pointermove 把**指针在卡内的相对坐标**（百分比）
 * 写进 `--hl-x` / `--hl-y`，layout.css 的 `.is-lit::before` 把它消费成一块以指针为
 * 圆心、半径 300px 的径向光斑——中心 100% 纯白且不衰减，向两侧对称衰减到 300px 处归零。
 *
 * 本文件只留纯函数，不吃 DOM，可单测。
 */

/** 离开卡片后默认光带回到的固定角度（左上 → 右下）。 */
export const DEFAULT_LIGHT_ANGLE = 135;

/** 指针（卡片内相对坐标）是否落在卡片上。 */
export function isInside(x: number, y: number, w: number, h: number): boolean {
  if (!(w > 0) || !(h > 0)) return false;
  return x >= 0 && y >= 0 && x <= w && y <= h;
}
