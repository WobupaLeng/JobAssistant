/**
 * 「一串文本」与字符串数组之间的转换。
 *
 * 配置面板里关键词、排除词、技能都是这种编辑形式：用户在一个输入框里敲
 * `Java, Spring`，提交时切成数组，回显时再拼回去。
 */

/**
 * 多值文本 → 去重数组。逗号（中英文）/ 分号 / 换行 / 空白都算分隔符。
 */
export function splitList(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(/[,，;；\s]+/)) {
    const s = raw.trim();
    if (s && !out.includes(s)) out.push(s);
  }
  return out;
}

export function joinList(list: string[] | null | undefined): string {
  return (list ?? []).join(', ');
}

/**
 * 把若干新词并进现有关键词串，**追加不覆盖**。
 *
 * 独立成函数的原因：这个「去重且保持原顺序」的规则被期望职位面板与抓取面板
 * 各写过一遍，任一处写错都会静默丢掉用户手填的关键词。
 */
export function mergeKeywords(current: string, incoming: string[]): { text: string; added: string[] } {
  const cur = splitList(current);
  const merged = [...cur];
  const added: string[] = [];
  for (const n of incoming) {
    const name = n.trim();
    if (name && !merged.includes(name)) {
      merged.push(name);
      added.push(name);
    }
  }
  return { text: merged.join(', '), added };
}
