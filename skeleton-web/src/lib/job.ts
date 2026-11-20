/**
 * 岗位域内的判定与格式化。纯函数，不依赖 React。
 */

/** SQLite 没有布尔列：`rejected === 1` 即被排除。 */
export function isRejected(job: { rejected: number }): boolean {
  return job.rejected === 1;
}

/**
 * 分数落在门槛的哪一侧：用于着色。
 *
 * `score` 可能为 null（未打分），必须单独成档——与「0 分」区分开，
 * 否则未打分岗位会被渲染成刺眼的失败红。
 */
export type ScoreTone = 'pass' | 'fail' | 'none';

export function scoreTone(score: number | null | undefined, minScore: number | null): ScoreTone {
  if (score === null || score === undefined) return 'none';
  if (minScore === null) return 'pass';
  return score >= minScore ? 'pass' : 'fail';
}
