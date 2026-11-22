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
import type { CardView } from '../lib_ui/job-list-model';

const props = withDefaults(
  defineProps<{
    view: CardView;
    /** 多选态（`.job-grid.is-selecting` 时勾选框才出现，判定在父级）。 */
    selecting?: boolean;
    selected?: boolean;
    /** 回车/空格展开详情用；点击展开由父级按 `shouldOpenCard` 决定。 */
    showGreet?: boolean;
  }>(),
  { selecting: false, selected: false, showGreet: true },
);

const emit = defineEmits<{
  (e: 'toggle', jobId: string): void;
  (e: 'greet', jobId: string): void;
  (e: 'open', jobId: string): void;
}>();

function onCheck(): void {
  emit('toggle', props.view.jobId);
}

function onGreet(): void {
  emit('greet', props.view.jobId);
}

function onOpen(): void {
  if (props.selecting) return;
  emit('open', props.view.jobId);
}
</script>

<template>
  <article
    class="job-card"
    :class="{ 'is-checked': selected, 'is-delivered': view.delivered, 'is-rejected': view.rejected }"
    :data-testid="`job-card-${view.tid}`"
    :data-card="view.jobId"
    @click="onOpen"
  >
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
      <span class="spacer"></span>
      <span class="job-salary">{{ view.salary }}</span>
    </div>
    <div class="job-meta">{{ view.company }}</div>
    <div class="job-meta">{{ view.metaText }}</div>
    <div class="job-info">
      <div
        class="job-addr"
        :class="{ 'is-empty': !view.hasAddr }"
        :title="view.addr"
        :data-testid="`job-addr-${view.tid}`"
      >
        {{ view.hasAddr ? view.addr : '—' }}
      </div>
      <span
        class="score-box"
        :class="view.scoreTier === 'is-high' ? 'is-hi' : ''"
        :data-testid="`score-${view.tid}`"
        >{{ view.score }}<i>分</i></span
      >
      <button
        v-if="showGreet"
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
  </article>
</template>
