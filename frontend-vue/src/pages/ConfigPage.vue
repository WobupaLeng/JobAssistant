<script setup lang="ts">
/**
 * 配置页，固定窗三张卡：大模型 / 提示词 / 技能。可独立 mount，无必需 props。token 用量读数
 * 归顶栏 `.tb-stats`，此处不重复渲染。保存只提交 diff（`config/model.buildPatch`），掩码不回写、
 * 空串写 `''` 不写 `null`。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { getAppConfig, setAppConfig } from '../api/commands/config';
import type { AppConfigReply } from '../api/types';
import { hasHigherOverlay } from '../lib_ui/overlay-stack';
import {
  buildPatch,
  defaultPrompt,
  draftFromReply,
  emptyDraft,
  isEmptyPatch,
  keyMaskText,
  keyPlaceholder,
  TEMPERATURE_DEFAULT_GREET,
  TEMPERATURE_DEFAULT_SCORE,
  TEMPERATURE_MAX,
  TEMPERATURE_MIN,
  temperatureError,
  type ConfigDraft,
} from '../features/config/model';
import ConfigSection from '../components/config/ConfigSection.vue';

defineProps<{ activeNav?: string }>();
const emit = defineEmits<{
  /** 浮层窗只有「回到概览」一个去向：聊天不是页。 */
  (e: 'navigate', nav: 'dashboard'): void;
}>();

const config = ref<AppConfigReply | null>(null);
const stored = ref<ConfigDraft>(emptyDraft());
const draft = ref<ConfigDraft>(emptyDraft());
const promptDefault = ref('');

const loading = ref(true);
const loadError = ref('');
const saving = ref(false);
const note = ref('');

const llmOpen = ref(true);
const promptOpen = ref(true);
const skillOpen = ref(true);

const maskText = computed(() => keyMaskText(config.value));
const keyPh = computed(() => keyPlaceholder(config.value));

/* 温度即时校验：输入过程中就给提示，保存时非法值本来也进不了 patch。 */
const scoreTempError = computed(() => temperatureError(draft.value.temperature));
const greetTempError = computed(() => temperatureError(draft.value.greet_temperature));

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function applyConfig(cfg: AppConfigReply): void {
  config.value = cfg;
  stored.value = draftFromReply(cfg);
  draft.value = { ...stored.value };
  promptDefault.value = defaultPrompt(cfg);
}

async function load(): Promise<void> {
  loading.value = true;
  loadError.value = '';
  try {
    applyConfig(await getAppConfig());
  } catch (e) {
    loadError.value = errText(e);
  } finally {
    loading.value = false;
  }
}

async function save(): Promise<void> {
  if (saving.value) return;
  note.value = '';
  // 先挡非法温度：静默跳过会让用户以为「保存成功但没生效」
  if (scoreTempError.value !== null || greetTempError.value !== null) {
    note.value = `保存失败：${scoreTempError.value ?? greetTempError.value}`;
    return;
  }
  const patch = buildPatch(draft.value, stored.value);
  if (isEmptyPatch(patch)) {
    note.value = '没有需要保存的改动';
    return;
  }
  saving.value = true;
  try {
    const reply = await setAppConfig(patch);
    // 重新读权威配置：刷新掩码与后端归一化后的值，草稿基线随之更新。
    applyConfig(await getAppConfig());
    note.value = `已保存 ${reply.updated} 项`;
  } catch (e) {
    note.value = `保存失败：${errText(e)}`;
  } finally {
    saving.value = false;
  }
}

function resetPrompt(): void {
  draft.value.greet_prompt = promptDefault.value;
}

function close(): void {
  emit('navigate', 'dashboard');
}

onMounted(load);

/* Esc 关闭：一次 keydown 只关最上面那一层，决策记录在配置之上。 */
function onKeydown(ev: KeyboardEvent): void {
  if (ev.key !== 'Escape') return;
  if (hasHigherOverlay('config')) return;
  close();
}
onMounted(() => window.addEventListener('keydown', onKeydown));
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <!-- 窗体锚点 config-panel 已唯一，页容器不再重复挂 page-config。 -->
  <section class="page page-config">
    <div
      class="config-modal is-open pan-modal"
      data-testid="config-panel"
      role="dialog"
      aria-modal="true"
      aria-label="配置"
    >
      <div class="cfg-head">
        <span class="card-title">配置</span>
        <span class="card-sub">改动保存后立即生效</span>
        <span class="spacer"></span>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          aria-label="关闭配置"
          data-testid="config-close-btn"
          @click="close"
        >
          ×
        </button>
      </div>

      <div class="cfg-body">
        <div v-if="loadError" class="log-inline-error" data-testid="config-error">
          {{ loadError }}
          <button
            type="button"
            class="btn btn-sm"
            data-testid="config-retry-btn"
            @click="load"
          >
            重试
          </button>
        </div>
        <div v-else-if="loading" class="empty" data-testid="config-loading">正在加载配置…</div>
        <template v-else>
          <ConfigSection
            v-model:open="llmOpen"
            title="大模型"
            subtitle="密钥只回显尾 4 位"
            section-test-id="config-llm-section"
            toggle-test-id="config-llm-toggle"
          >
            <div class="cfg-row-llm">
              <label class="cfg-field">
                <span class="label">API Key</span>
                <input
                  v-model="draft.api_key"
                  class="input"
                  type="password"
                  autocomplete="new-password"
                  :placeholder="keyPh"
                  aria-label="API Key"
                  data-testid="config-api-key"
                />
                <span class="field-hint"
                  >留空表示不改；当前：<b data-testid="config-api-key-mask">{{ maskText }}</b></span
                >
              </label>
              <label class="cfg-field">
                <span class="label">Base URL</span>
                <input
                  v-model="draft.base_url"
                  class="input"
                  type="text"
                  placeholder="大模型接口地址"
                  aria-label="Base URL"
                  data-testid="config-base-url"
                />
                <span class="field-hint">OpenAI 兼容接口的根地址</span>
              </label>
            </div>

            <!-- 第二行：模型 + 两档温度（温度调的就是这个模型的发散程度） -->
            <div class="cfg-row-llm">
              <label class="cfg-field">
                <span class="label">模型</span>
                <input
                  v-model="draft.model"
                  class="input"
                  type="text"
                  placeholder="deepseek-chat"
                  aria-label="模型"
                  data-testid="config-model"
                />
                <span class="field-hint">打分与话术共用这一个模型</span>
              </label>
              <label class="cfg-field">
                <span class="label">打分温度</span>
                <input
                  v-model="draft.temperature"
                  class="input"
                  type="number"
                  :min="TEMPERATURE_MIN"
                  :max="TEMPERATURE_MAX"
                  step="0.1"
                  inputmode="decimal"
                  :placeholder="String(TEMPERATURE_DEFAULT_SCORE)"
                  aria-label="打分温度"
                  :aria-invalid="scoreTempError !== null"
                  data-testid="config-temperature"
                />
                <span class="field-hint" :class="{ 'is-bad': scoreTempError !== null }">
                  <template v-if="scoreTempError">{{ scoreTempError }}</template>
                  <template v-else
                    >给岗位打分用，{{ TEMPERATURE_MIN }}~{{ TEMPERATURE_MAX }}，出厂
                    {{ TEMPERATURE_DEFAULT_SCORE }}。调高分数会不稳定（同一条岗位每次得分可能不同），
                    建议 {{ TEMPERATURE_MIN }}~0.4</template
                  >
                </span>
              </label>
              <label class="cfg-field">
                <span class="label">话术温度</span>
                <input
                  v-model="draft.greet_temperature"
                  class="input"
                  type="number"
                  :min="TEMPERATURE_MIN"
                  :max="TEMPERATURE_MAX"
                  step="0.1"
                  inputmode="decimal"
                  :placeholder="String(TEMPERATURE_DEFAULT_GREET)"
                  aria-label="话术温度"
                  :aria-invalid="greetTempError !== null"
                  data-testid="config-greet-temperature"
                />
                <span class="field-hint" :class="{ 'is-bad': greetTempError !== null }">
                  <template v-if="greetTempError">{{ greetTempError }}</template>
                  <template v-else
                    >写招呼语 / 回复用，{{ TEMPERATURE_MIN }}~{{ TEMPERATURE_MAX }}，出厂
                    {{ TEMPERATURE_DEFAULT_GREET }}。越低越稳定规整，越高越有变化；
                    超过 1.2 容易跑题或编造经历</template
                  >
                </span>
              </label>
            </div>
          </ConfigSection>

          <ConfigSection
            v-model:open="promptOpen"
            title="提示词"
            subtitle="打招呼与简历解读共用"
            section-test-id="config-prompt-section"
            toggle-test-id="config-prompt-toggle"
          >
            <label class="cfg-field">
              <span class="label">提示词模板</span>
              <textarea
                v-model="draft.greet_prompt"
                class="textarea"
                rows="4"
                aria-label="提示词模板"
                data-testid="config-prompt-input"
                placeholder="描述希望生成的招呼语与打分口径…"
              ></textarea>
              <span class="field-hint"
                >留空则用内置模板；这里的内容会拼在系统提示之后，请勿粘贴密钥</span
              >
            </label>
            <div>
              <button
                type="button"
                class="btn btn-sm btn-ghost"
                data-testid="config-prompt-reset-btn"
                @click="resetPrompt"
              >
                恢复默认
              </button>
            </div>
          </ConfigSection>

          <ConfigSection
            v-model:open="skillOpen"
            title="技能"
            subtitle="生成招呼语时优先"
            section-test-id="config-skill-section"
            toggle-test-id="config-skill-toggle"
          >
            <label class="cfg-field">
              <span class="label">技能清单</span>
              <textarea
                v-model="draft.skills"
                class="textarea"
                rows="3"
                aria-label="技能清单"
                data-testid="config-skill-input"
                placeholder="Java、Spring Boot、MySQL…"
              ></textarea>
              <span class="field-hint"
                >生成招呼语时优先用这里的技能；留空则用简历画像里的技能。多个用逗号 / 顿号分隔</span
              >
            </label>
          </ConfigSection>
        </template>
      </div>

      <div class="cfg-foot">
        <span class="field-hint" data-testid="config-note">{{
          note || '只提交改动过的字段（diff）；API Key 留空表示不改，掩码不回写'
        }}</span>
        <span class="spacer"></span>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          data-testid="config-cancel-btn"
          @click="close"
        >
          取消
        </button>
        <button
          type="button"
          class="btn btn-sm btn-primary"
          data-testid="config-save-btn"
          :disabled="saving || loading"
          @click="save"
        >
          {{ saving ? '保存中…' : '保存' }}
        </button>
      </div>
    </div>
  </section>
</template>
