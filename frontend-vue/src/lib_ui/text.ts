/**
 * 「一串文本」与字符串数组之间的转换。配置面板里关键词、排除词、技能都是这种编辑形式：
 * 输入框里敲 `Java, Spring`，提交时切成数组，回显时再拼回去。
 */

/**
 * 分隔符集合：空白（含换行 / 制表符）、中英文逗号、顿号、中英文分号、竖线、正反斜杠。
 * 少任何一类都会让 `Java、Go` 被当成一个关键词，表现为「抓不到岗位」却没有任何报错。
 */
const SPLIT_CHARS = new Set([
  ',',
  '，',
  '、',
  ';',
  '；',
  '|',
  '/',
  '\\',
  '\n',
  '\r',
  '\t',
  ' ',
  // 其余空白（全角空格、垂直制表等）也按分隔处理，对应 Rust 的 char::is_whitespace
]);

function isSeparator(ch: string): boolean {
  return SPLIT_CHARS.has(ch) || /\s/.test(ch);
}

/** 多值文本 → 去空白、去重（保持首次出现顺序）的数组。 */
export function splitList(text: string): string[] {
  const out: string[] = [];
  let buf = '';
  const flush = () => {
    const s = buf.trim();
    buf = '';
    if (s.length > 0 && !out.includes(s)) out.push(s);
  };
  for (const ch of text) {
    if (isSeparator(ch)) flush();
    else buf += ch;
  }
  flush();
  return out;
}

export function joinList(list: readonly string[]): string {
  return list.join(', ');
}

