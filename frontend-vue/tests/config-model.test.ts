import { describe, expect, it } from 'vitest';
import {
  buildPatch,
  defaultPrompt,
  draftFromReply,
  isEmptyPatch,
  keyMaskText,
  keyPlaceholder,
  type ConfigDraft,
} from '../src/features/config/model';
import type { AppConfigReply } from '../src/api/types';

function cfg(over: Partial<AppConfigReply> = {}): AppConfigReply {
  return {
    base_url: 'https://api.deepseek.com',
    api_key_hint: '****9f3c',
    has_key: true,
    model: 'deepseek-chat',
    score_mode: 'rule',
    theme: 'dark',
    llm_prefilter: 50,
    max_jd: 15,
    min_score: 60,
    score_max: 100,
    greet_prompt: '默认模板',
    greet_prompt_default: '出厂模板',
    skills: 'Java、MySQL',
    exclude_keywords: '',
    exclude_rules: '',
    resume_path: '',
    data_dir: '',
    last_filter: '',
    tokens_total: 0,
    tokens_greet: 0,
    tokens_score: 0,
    scrape_defaults: null,
    ...over,
  };
}

describe('draftFromReply', () => {
  it('api_key 恒为空，绝不回填掩码或明文', () => {
    const d = draftFromReply(cfg({ api_key_hint: '****9f3c', has_key: true }));
    expect(d.api_key).toBe('');
  });

  it('单行字段 trim，提示词保留原样', () => {
    const d = draftFromReply(
      cfg({ base_url: '  https://x  ', model: ' m ', skills: ' s ', greet_prompt: ' a\nb ' }),
    );
    expect(d.base_url).toBe('https://x');
    expect(d.model).toBe('m');
    expect(d.skills).toBe('s');
    expect(d.greet_prompt).toBe(' a\nb ');
  });
});

describe('buildPatch — 只提交 diff', () => {
  const stored = draftFromReply(cfg());

  it('无改动 → 空 patch', () => {
    const draft: ConfigDraft = { ...stored };
    expect(isEmptyPatch(buildPatch(draft, stored))).toBe(true);
  });

  it('只改一个字段，其余不进 patch', () => {
    const draft: ConfigDraft = { ...stored, model: 'gpt-4o' };
    expect(buildPatch(draft, stored)).toEqual({ model: 'gpt-4o' });
  });

  it('清空字段写空串，不写 null（null 是删除键，语义不同）', () => {
    const draft: ConfigDraft = { ...stored, skills: '   ' };
    const patch = buildPatch(draft, stored);
    expect(patch).toEqual({ skills: '' });
    expect(patch.skills).not.toBeNull();
  });

  it('提示词改动进 patch（trim 比较）', () => {
    const draft: ConfigDraft = { ...stored, greet_prompt: '新模板' };
    expect(buildPatch(draft, stored)).toEqual({ greet_prompt: '新模板' });
  });

  it('仅首尾空白变化不算改动', () => {
    const draft: ConfigDraft = { ...stored, base_url: `  ${stored.base_url}  ` };
    expect(isEmptyPatch(buildPatch(draft, stored))).toBe(true);
  });
});

describe('buildPatch — API Key 掩码不回写', () => {
  const stored = draftFromReply(cfg());

  it('留空 = 不改，不进 patch', () => {
    const draft: ConfigDraft = { ...stored, api_key: '' };
    expect(buildPatch(draft, stored)).toEqual({});
  });

  it('粘贴掩码 ****9f3c 一律丢弃', () => {
    const draft: ConfigDraft = { ...stored, api_key: '****9f3c' };
    expect(buildPatch(draft, stored)).toEqual({});
  });

  it('真实新密钥才进 patch', () => {
    const draft: ConfigDraft = { ...stored, api_key: 'sk-new-secret' };
    expect(buildPatch(draft, stored)).toEqual({ api_key: 'sk-new-secret' });
  });
});

describe('buildPatch — 绝不产出 null / 禁键', () => {
  it('全字段清空也不产 null，且不含 min_score / api_key_hint', () => {
    const stored = draftFromReply(cfg());
    const draft: ConfigDraft = {
      api_key: '****9f3c',
      base_url: '',
      model: '',
      greet_prompt: '',
      skills: '',
    };
    const patch = buildPatch(draft, stored);
    for (const v of Object.values(patch)) expect(v).not.toBeNull();
    expect(Object.keys(patch)).not.toContain('min_score');
    expect(Object.keys(patch)).not.toContain('api_key_hint');
    expect(patch).toEqual({ base_url: '', model: '', greet_prompt: '', skills: '' });
  });
});

describe('掩码回显 / placeholder / 恢复默认', () => {
  it('keyMaskText 优先后端掩码', () => {
    expect(keyMaskText(cfg({ api_key_hint: '****9f3c' }))).toBe('****9f3c');
  });
  it('无掩码但 has_key → 已配置', () => {
    expect(keyMaskText(cfg({ api_key_hint: '', has_key: true }))).toBe('已配置');
  });
  it('无掩码且无 key → 未配置', () => {
    expect(keyMaskText(cfg({ api_key_hint: '', has_key: false }))).toBe('未配置');
  });
  it('null 配置 → 未配置', () => {
    expect(keyMaskText(null)).toBe('未配置');
  });
  it('placeholder 用掩码，缺省 ****', () => {
    expect(keyPlaceholder(cfg({ api_key_hint: '****9f3c' }))).toBe('****9f3c');
    expect(keyPlaceholder(cfg({ api_key_hint: '' }))).toBe('****');
    expect(keyPlaceholder(null)).toBe('****');
  });
  it('defaultPrompt 取出厂模板', () => {
    expect(defaultPrompt(cfg({ greet_prompt_default: '出厂模板' }))).toBe('出厂模板');
    expect(defaultPrompt(null)).toBe('');
  });
});
