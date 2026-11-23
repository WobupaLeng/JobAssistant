/**
 * M3 配置页的纯逻辑（草稿 / diff / 掩码），与视图解耦便于单测。
 *
 * 对照 Leptos 版已退役，见 git 历史，提交 53d05cb 之前的配置草稿与状态机，但只保留冻结原型
 * （UI-SPEC-chat-detach-v3 §11 v6）留下的三张卡：大模型 / 提示词 / 技能。
 * 高级卡（门槛 / 上限 / 预筛 / 关键词 / token）已被用户裁决删除，不在本页。
 *
 * 写入铁律（根 AGENTS.md §2 / frontend-vue C11）：
 * - **只提交 diff**：没改的键不进 patch（整份回传会把未设置项清空）。
 * - **空串写 ''，不是 null**：null 在后端是「删除该键 = 恢复出厂」，与「清空值」不同。
 * - **API Key 掩码不回写**：留空 = 不改；以 **** 开头的掩码值一律丢弃。
 * - 本页无门槛 / 上限控件，故只走 set_app_config，不触发 set_limit。
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
}

/** 走 diff 比较的文本字段（api_key 单独处理）。 */
export const TEXT_FIELDS = ['base_url', 'model', 'greet_prompt', 'skills'] as const;
export type TextField = (typeof TEXT_FIELDS)[number];

/** 空草稿（加载前 / 独立 mount 时的初值）。 */
export function emptyDraft(): ConfigDraft {
  return { api_key: '', base_url: '', model: '', greet_prompt: '', skills: '' };
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
    if (cur !== base) patch[k] = cur;
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
