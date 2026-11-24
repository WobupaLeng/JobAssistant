/**
 * M3 配置页的纯逻辑（草稿 / diff / 掩码），与视图解耦便于单测。
 *
 * 配置草稿与状态机：只保留冻结原型
 * （UI-SPEC-chat-detach-v3 §11 v6）留下的三张卡：大模型 / 提示词 / 技能。
 * 高级卡（门槛 / 上限 / 预筛 / 关键词 / token）已被用户裁决删除，不在本页。
 *
 * 写入铁律（根 AGENTS.md §2 / frontend-vue C11）：
 * - **只提交 diff**：没改的键不进 patch（整份回传会把未设置项清空）。
 * - **空串写 ''，不是 null**：null 在后端是「删除该键 = 恢复出厂」，与「清空值」不同。
 * - **API Key 掩码不回写**：留空 = 不改；以 **** 开头的掩码值一律丢弃。
 * - 本页无门槛 / 上限控件，故只走 set_app_config，不触发 set_limit。
 *
 * 温度（`temperature` / `greet_temperature`）与文本字段共用 diff，但落库时转成
 * 数字并夹进 [0, 2]；解析不出的值不进 patch，界面同时给出提示。
 */

import type { AppConfigReply } from '../../api/types';

/** 五个可编辑字段，全部以字符串承载（textarea / input 的原始值）。 */
export interface ConfigDraft {
  /** 永远是用户本次输入，初始 ''；绝不从回执回填掩码或明文。 */
  api_key: string;
  base_url: string;
  model: string;
  greet_prompt: string;
  skills: string;
  /** 打分温度，'0'~'2' 的文本 */
  temperature: string;
  /** 话术温度，'0'~'2' 的文本 */
  greet_temperature: string;
}

/**
 * 参与编辑与 diff 的字段全集：六项文本 + 两个温度。
 *
 * 温度**以字符串承载**（和文本字段同一套 diff 逻辑）：数字类型的草稿一碰
 * `v-model.number` 就会把「0.」变成 0，用户还没打完就被改写。落库前再夹区间。
 */
export const TEXT_FIELDS = [
  'base_url',
  'model',
  'greet_prompt',
  'skills',
  'temperature',
  'greet_temperature',
] as const;
export type TextField = (typeof TEXT_FIELDS)[number];

/** 落库是数字的字段：只在 buildPatch 里转成 number。 */
export const NUMBER_FIELDS: readonly TextField[] = ['temperature', 'greet_temperature'];

/** 温度的合法区间与出厂默认值，与后端 `infra::llm` 同源。 */
export const TEMPERATURE_MIN = 0;
export const TEMPERATURE_MAX = 2;
export const TEMPERATURE_DEFAULT_SCORE = 0.2;
export const TEMPERATURE_DEFAULT_GREET = 0.8;

/** 空草稿（加载前 / 独立 mount 时的初值）。 */
export function emptyDraft(): ConfigDraft {
  return {
    api_key: '',
    base_url: '',
    model: '',
    greet_prompt: '',
    skills: '',
    temperature: '',
    greet_temperature: '',
  };
}

/** 温度显示成文本：0.2 → '0.2'，1 → '1'（不补零，避免每次读回都被"改写"）。 */
export function temperatureText(v: number | null | undefined, dflt: number): string {
  const n = typeof v === 'number' && Number.isFinite(v) ? v : dflt;
  return String(Number(n.toFixed(2)));
}

/**
 * 解析用户填的温度。
 *
 * - 空串 → `null`（空 = 不改，**不能**回落成出厂值，否则「清空保存」会凭空写一次库）
 * - 非数字 → `null`（调用方据此提示错误，绝不猜测）
 * - 越界 → 夹进 [0, 2]；两位小数，与后端归一化一致
 */
export function parseTemperature(raw: string, _dflt: number): number | null {
  const t = (raw ?? '').trim();
  if (t === '') return null;
  const n = Number(t);
  if (!Number.isFinite(n)) return null;
  const c = Math.min(TEMPERATURE_MAX, Math.max(TEMPERATURE_MIN, n));
  return Math.round(c * 100) / 100;
}

/** 温度字段的即时校验文案：`null` 表示合法（合法时不弹提示）。 */
export function temperatureError(raw: string): string | null {
  if ((raw ?? '').trim() === '') return null;
  if (parseTemperature(raw, TEMPERATURE_DEFAULT_SCORE) !== null) return null;
  return `温度必须是 ${TEMPERATURE_MIN}~${TEMPERATURE_MAX} 之间的数字`;
}

/**
 * 从回执建草稿。`api_key` 恒为 ''（后端根本不回明文，只有 `api_key_hint` 掩码）。
 * 单行字段 trim；提示词模板保留原样（多行），diff 时再 trim 比较。
 */
export function draftFromReply(cfg: AppConfigReply): ConfigDraft {
  return {
    api_key: '',
    base_url: (cfg.base_url ?? '').trim(),
    model: (cfg.model ?? '').trim(),
    greet_prompt: cfg.greet_prompt ?? '',
    skills: (cfg.skills ?? '').trim(),
    temperature: temperatureText(cfg.temperature, TEMPERATURE_DEFAULT_SCORE),
    greet_temperature: temperatureText(cfg.greet_temperature, TEMPERATURE_DEFAULT_GREET),
  };
}

/**
 * 计算「只含改动字段」的 patch。
 *
 * - 文本字段：trim 后与基线不同才进 patch，值写 trim 后的串；**清空写 ''，不写 null**。
 * - API Key：非空且不以 **** 开头才进 patch（掩码 / 留空都丢弃）。
 * - 绝不产出 null，绝不产出 min_score / api_key_hint（后者由 api 层 sanitize 再兜一道）。
 */
export function buildPatch(draft: ConfigDraft, stored: ConfigDraft): Record<string, unknown> {
  const patch: Record<string, unknown> = {};
  for (const k of TEXT_FIELDS) {
    const cur = (draft[k] ?? '').trim();
    const base = (stored[k] ?? '').trim();
    if (cur === base) continue;
    if (NUMBER_FIELDS.includes(k)) {
      // 温度是数值键：后端 `set_app_config` 会把非字符串按 JSON 原样写库，
      // 写成字符串 '0.8' 也能读，但让库里全是数字更干净、也更好比对。
      // 清空 ≠ 恢复出厂：清空就是「不改」（返回 null），否则「把输入框全选删掉再保存」
      // 会把出厂值当成一次改动重新写库，语义上属于凭空捏造一次写入。
      // 要回出厂值请填数字；解析不出来的**绝不进 patch**，宁可少写一个键也不写 NaN。
      const dflt =
        k === 'temperature' ? TEMPERATURE_DEFAULT_SCORE : TEMPERATURE_DEFAULT_GREET;
      const n = parseTemperature(cur, dflt);
      if (n === null) continue;
      patch[k] = n;
      continue;
    }
    patch[k] = cur;
  }
  const key = (draft.api_key ?? '').trim();
  if (key !== '' && !key.startsWith('****')) patch.api_key = key;
  return patch;
}

/** patch 是否为空（= 没有需要保存的改动）。 */
export function isEmptyPatch(patch: Record<string, unknown>): boolean {
  return Object.keys(patch).length === 0;
}

/** 密钥回显文案：优先后端掩码 `****abcd`；无掩码时按 has_key 给「已配置 / 未配置」。 */
export function keyMaskText(cfg: AppConfigReply | null): string {
  if (!cfg) return '未配置';
  const hint = (cfg.api_key_hint ?? '').trim();
  if (hint) return hint;
  return cfg.has_key ? '已配置' : '未配置';
}

/** API Key 输入框 placeholder：用掩码示意，绝不泄露明文。 */
export function keyPlaceholder(cfg: AppConfigReply | null): string {
  const hint = (cfg?.api_key_hint ?? '').trim();
  return hint || '****';
}

/** 「恢复默认」用的出厂提示词。 */
export function defaultPrompt(cfg: AppConfigReply | null): string {
  return cfg?.greet_prompt_default ?? '';
}
