<script setup lang="ts">
/**
 * 单张岗位卡（五行卡 + 打招呼方块）。只做展示与事件转出，判定全在
 * `features/jobs/list-model.ts`；勾选框绑 `@click.stop`，多选态下点它不该触发卡片单击。
 * 长按（≥500ms）的状态机在 `lib_ui/longpress.ts`，这里只管起表 / 撤表并转出到期事件。
 */
import { computed, onBeforeUnmount } from 'vue';
import {
  armLongPress,
  ignorePointerTarget,
  interactiveSelector,
  movedBeyond,
  type TimerHandle,
} from '../lib_ui/longpress';
import type { Job } from '../api/types';
import type { CardView } from '../lib_ui/job-list-model';
import type { ListTab } from '../lib_ui/job-list-model';

const props = withDefaults(
  defineProps<{
    view: CardView;
    /** 当前列表页签：卡片尺寸/边框与岗位列表完全一致，只有语义色随页签切换
     *  （待投递=主题色 / 已投递=成功绿 / 不投递=否决红）。 */
    tab?: ListTab;
    /** 多选态（`.job-grid.is-selecting` 时勾选框才出现，判定在父级）。 */
    selecting?: boolean;
    selected?: boolean;
    /** 回车/空格展开详情用；点击展开由父级按 `shouldOpenCard` 决定。 */
    showGreet?: boolean;
    /** 与当前展开聊天的 Boss 同公司：联动高亮（判定在父级）。 */
    linked?: boolean;
  }>(),
  { tab: 'pending', selecting: false, selected: false, showGreet: true, linked: false },
);

const emit = defineEmits<{
  (e: 'toggle', jobId: string): void;
  (e: 'greet', jobId: string): void;
  /** 不投递列表里的反向动作：恢复成待投递（回到「未投递」列表）。 */
  (e: 'allow', jobId: string): void;
  /** 已投递列表里的反向动作：取消投递标记（回到「未投递」列表）。 */
  (e: 'undeliver', jobId: string): void;
  /** 打开详情：整条岗位随事件交出去，父级不回列表里反查（分页窗口里可能已没有它）。 */
  (e: 'open', job: Job): void;
  /** 长按到点：进入多选并勾上这张卡。 */
  (e: 'longpress', jobId: string): void;
}>();

function onCheck(): void {
  emit('toggle', props.view.jobId);
}

function onGreet(): void {
  emit('greet', props.view.jobId);
}

function onAllow(): void {
  emit('allow', props.view.jobId);
}

function onUndeliver(): void {
  emit('undeliver', props.view.jobId);
}

/** 流光炫彩强度分三档：分越高越炫（满分 100 口径，见 SPEC 打分）。 */
const scoreLevelClass = computed(() => {
  const s = props.view.score;
  if (s >= 80) return 'score-lv-3';
  if (s >= 60) return 'score-lv-2';
  return 'score-lv-1';
});

function onOpen(): void {
  emit('open', props.view.job);
}

/** 从按下到长按到点之间一直存活的撤销句柄；null 表示当前没有按着。 */
let pressTimer: TimerHandle | null = null;
let pressOrigin = { x: 0, y: 0 };
/** 长按已经触发过：随后的 click 是这次长按的余波，不能再当成「打开详情」。 */
let longPressed = false;

function cancelPress(): void {
  pressTimer?.cancel();
  pressTimer = null;
}

function onPointerDown(ev: PointerEvent): void {
  // 只认主键：右键菜单、中键滚动都不该起表。
  if (ev.button !== 0) return;
  // 按钮 / 输入框上的长按不归多选管（要能正常点、正常聚焦）。
  const target = ev.target as HTMLElement | null;
  const interactive = target?.closest?.(interactiveSelector()) ?? null;
  if (ignorePointerTarget(target?.tagName ?? '', interactive !== null)) return;
  pressTimer?.cancel();
  pressTimer = null;
  longPressed = false;
  pressOrigin = { x: ev.clientX, y: ev.clientY };
  pressTimer = armLongPress(() => {
    pressTimer = null;
    longPressed = true;
    emit('longpress', props.view.jobId);
  });
}

function onPointerMove(ev: PointerEvent): void {
  if (!pressTimer) return;
  if (movedBeyond(pressOrigin, { x: ev.clientX, y: ev.clientY })) {
    pressTimer.cancel();
    pressTimer = null;
  }
}

/** 抬起：定时器作废；长按到点过的这次按下，尾巴上的 click 会被吞掉。 */
function onPointerUp(): void {
  pressTimer?.cancel();
  pressTimer = null;
}

/**
 * 单击：多选期间只切勾选，不弹详情（父级按 `selecting` 决定）。长按刚触发时浏览器仍会补
 * 一个 click，必须吞掉——否则勾选状态一闪即空。
 */
function onClick(): void {
  if (longPressed) {
    longPressed = false;
    return;
  }
  onOpen();
}

onBeforeUnmount(cancelPress);
</script>

<template>
  <article
    class="job-card hl-surface"
    :class="{
      'is-checked': selected,
      'is-delivered': view.delivered,
      'is-rejected': view.rejected,
      'is-linked': linked,
      'tone-delivered': tab === 'delivered',
      'tone-rejected': tab === 'rejected',
    }"
    :data-testid="`job-card-${view.tid}`"
    :data-card="view.jobId"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointerleave="cancelPress"
    @pointercancel="cancelPress"
    @click="onClick"
    @contextmenu.prevent
  >
    <!-- 左列（职位标题 / 公司 / 经验学历 / 地址，逐行省略）与外列（薪资 / 分数 / 打招呼）
         共用一个两列表格：薪资与职位标题同在第一行，左列超长只吃自己的省略号。 -->
    <div class="job-info">
      <div class="job-info-main">
        <div class="job-card-top">
          <span class="job-check-slot">
            <input
              class="job-check"
              type="checkbox"
              :checked="selected"
              :data-job="view.jobId"
              :data-testid="`job-check-${view.tid}`"
              :aria-label="`勾选 ${view.title}`"
              @click.stop
              @change="onCheck"
            />
          </span>
          <span class="job-title">{{ view.title }}</span>
        </div>
        <div class="job-meta">{{ view.company }}</div>
        <div class="job-meta">{{ view.metaText }}</div>
        <div
          class="job-addr"
          :class="{ 'is-empty': !view.hasAddr }"
          :title="view.addr"
          :data-testid="`job-addr-${view.tid}`"
        >
          {{ view.hasAddr ? view.addr : '—' }}
        </div>
      </div>
      <div class="job-info-side">
        <span class="job-salary">{{ view.salary }}</span>
        <span
          class="score-box"
          :class="[view.scoreTier === 'is-high' ? 'is-hi' : '', scoreLevelClass]"
          :data-testid="`score-${view.tid}`"
          ><span class="score-num">{{ view.score }}</span></span
        >
        <button
          v-if="showGreet && tab === 'rejected'"
          class="btn btn-sm btn-ghost job-greet"
          type="button"
          :data-allow="view.jobId"
          :data-testid="`job-allow-${view.tid}`"
          :aria-label="`撤销否决 ${view.title}`"
          @click.stop="onAllow"
        >
          撤销否决
        </button>
        <button
          v-else-if="showGreet && tab === 'delivered'"
          class="btn btn-sm btn-ghost job-greet"
          type="button"
          :data-undeliver="view.jobId"
          :data-testid="`job-undeliver-${view.tid}`"
          :aria-label="`设为待投递 ${view.title}`"
          @click.stop="onUndeliver"
        >
          待投递
        </button>
        <button
          v-else-if="showGreet"
          class="btn btn-primary job-greet"
          type="button"
          :data-greet="view.jobId"
          :data-testid="`job-greet-${view.tid}`"
          :aria-label="`向 ${view.company} 打招呼`"
          @click.stop="onGreet"
        >
          打招呼
        </button>
      </div>
    </div>
  </article>
</template>
