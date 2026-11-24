/**
 * M4：卡片高光边随光源变化（iOS 风格）。
 *
 * 光源取指针位置：`main.ts` 每次 pointermove 把**指针在卡内的相对坐标**（百分比）
 * 写进 `--hl-x` / `--hl-y`，layout.css 的 `.hl-light` 把它消费成一块以指针为圆心、
 * 半径 300px 的径向光斑——中心 100% 纯白且不衰减，向两侧对称衰减到 300px 处归零。
 *
 * 本文件只留纯函数，不吃 DOM，可单测。
 */

/** 指指针不在卡内时（理论上不会走到）用的兜底角度。 */
export const DEFAULT_LIGHT_ANGLE = 135;

/** 指针（卡片内相对坐标）是否落在卡片上。 */
export function isInside(x: number, y: number, w: number, h: number): boolean {
  if (!(w > 0) || !(h > 0)) return false;
  return x >= 0 && y >= 0 && x <= w && y <= h;
}

/**
 * 由卡片内相对坐标算出光源角度（deg）：光从指针处射来，即指针相对卡片中心的方向。
 * 0deg 指向上、顺时针增加，与 CSS linear-gradient 角度约定一致。
 * 当前光斑是径向圆斑、不消费这个角度，保留给定向高光用。
 */
export function lightAngle(x: number, y: number, w: number, h: number): number {
  if (!(w > 0) || !(h > 0)) return DEFAULT_LIGHT_ANGLE;
  const dx = x - w / 2;
  const dy = y - h / 2;
  if (dx === 0 && dy === 0) return DEFAULT_LIGHT_ANGLE;
  // atan2 给的是数学角（x 右、y 下）；转成 CSS 角（0deg 朝上、顺时针）
  const deg = (Math.atan2(dx, -dy) * 180) / Math.PI;
  return (deg + 360) % 360;
}