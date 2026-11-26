/**
 * token 累计的「该重读了」信号。
 *
 * 后端按 ADR-005 在打分 / 打招呼 / 自动投递的回执里累加落库（`bump_tokens` 单事务），但顶栏
 * 读数只在概览挂载时读一次 `get_app_config`，累加之后没有任何人重读——数字会停在进页那一刻。
 * 所以：**谁调了会花钱的命令，就在成功回执之后 ping 一次**；概览据此做一次只读重读。
 *
 * 这里只放一枚计数器：不带数值、不起轮询、不碰业务状态，消费方自己决定怎么重读。
 */
import { ref } from 'vue';

/** 每次「刚刚花掉过 token」+1。值本身没有含义，只看变化。 */
export const tokenSpentTick = ref(0);

/** 成功回执之后调用一次（失败没花钱，不必 ping）。 */
export function markTokensSpent(): void {
  tokenSpentTick.value += 1;
}