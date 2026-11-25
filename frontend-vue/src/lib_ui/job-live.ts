/**
 * 岗位列表的**乐观补丁**：自动投递发出后先隐掉那条、抓取入库后先补几条，
 * 界面不必等重拉就对得上。用户主动刷新（`refreshList`）时整份清掉，回到后端真实数据。
 * 这里只存「本地已知、服务端这一页还没反映」的差量，不做持久化。
 */
import { ref } from 'vue';
import type { Job } from '../api/types';

/** 已发出、列表页还没重拉掉的岗位 id（新的在前）。 */
const sentIds = ref<readonly string[]>([]);
/** 抓取中就地补进列表的行；不参与服务端分页的 offset 基准。 */
const freshJobs = ref<readonly Job[]>([]);
/** 页签数字的临时补丁：走出去一条待投递 -1、已投递 +1。 */
const pendingDelta = ref(0);
const deliveredDelta = ref(0);

/** 自动投递真的发出了一条：那条从「未投递」挪到「已投递」。 */
export function markJobSent(jobId: string): void {
  const id = jobId.trim();
  if (id === '' || sentIds.value.includes(id)) return;
  sentIds.value = [id, ...sentIds.value];
  pendingDelta.value -= 1;
  deliveredDelta.value += 1;
}

/** 抓取刚入库几条：就地补进列表顶部（按 id 去重，重复调用幂等）。 */
export function pushFreshJobs(rows: readonly Job[]): void {
  const seen = new Set(freshJobs.value.map((j) => j.job_id));
  const add = rows.filter((j) => !seen.has(j.job_id));
  if (add.length === 0) return;
  freshJobs.value = [...add, ...freshJobs.value];
}

/** 后端给了权威计数：数字补丁到此失效（行还留着，等列表重拉时一并清）。 */
export function resetCountDelta(): void {
  pendingDelta.value = 0;
  deliveredDelta.value = 0;
}

/** 列表整份重拉过：行与数字都以后端为准。 */
export function clearLivePatches(): void {
  sentIds.value = [];
  freshJobs.value = [];
  resetCountDelta();
}

export interface LivePatches {
  readonly sentIds: readonly string[];
  readonly freshJobs: readonly Job[];
  readonly pendingDelta: number;
  readonly deliveredDelta: number;
}

/** 读当前补丁：在 `computed` 里调用即建立依赖，写入后界面自动跟上。 */
export function livePatches(): LivePatches {
  return {
    sentIds: sentIds.value,
    freshJobs: freshJobs.value,
    pendingDelta: pendingDelta.value,
    deliveredDelta: deliveredDelta.value,
  };
}