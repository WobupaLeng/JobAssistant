<script setup lang="ts">
/**
 * 单张岗位卡（SPEC §16.4/§16.5 裁决 T/V/W2 的五行卡 + 打招呼方块）。
 *
 * 组件只做展示与事件转出，判定逻辑全在 `features/jobs/list-model.ts`：
 * 卡片在 `.job-grid` 里的位置由父级决定，`selected` / `selecting` 由父级下发。
 *
 * 为什么 check 绑 `@click.stop`：多选态下点勾选框只想改勾选，不该触发
 * 卡片单击（那会变成"再切一次"，两次结果互相抵消）。
 */
import { computed } from 'vue';
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
  (e: 'open', jobId: string): void;
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
  if (props.selecting) return;
  emit('open', props.view.jobId);
}
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
    @click="onOpen"
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
          :aria-label="`允许投递 ${view.title}`"
          @click.stop="onAllow"
        >
          允许投递
        </button>
        <button
          v-else-if="showGreet && tab === 'delivered'"
          class="btn btn-sm btn-ghost job-greet"
          type="button"
          :data-undeliver="view.jobId"
          :data-testid="`job-undeliver-${view.tid}`"
          :aria-label="`取消投递 ${view.title}`"
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
