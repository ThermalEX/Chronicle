<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Plus, Trash2 } from "@lucide/vue";
import type { CloudSource } from "../services/settings";
import { configureOpenDal, openDalTemplates, publicConfigKeys, validateAdvancedKey } from "../services/opendal";
import ThemedSelect from "./ThemedSelect.vue";

const props = defineProps<{ source: CloudSource; secrets: Record<string, string>; credentialSaved?: boolean; disabled?: boolean }>();
const fieldName = ref("");
const fieldSecret = ref(true);
const fieldError = ref("");
const options = openDalTemplates.map((item) => ({ value: item.scheme, label: item.label }));
const publicFields = computed(() => Object.keys(props.source.config ?? {}));
const canBePublic = computed(() => publicConfigKeys.includes(fieldName.value));
watch(canBePublic, (allowed) => { if (!allowed) fieldSecret.value = true; });
function setScheme(value: string | null): void {
  if (!value) return;
  configureOpenDal(props.source, value);
  for (const key of Object.keys(props.secrets)) delete props.secrets[key];
  fieldError.value = "";
}
function addField(): void {
  const key = fieldName.value.trim();
  fieldError.value = validateAdvancedKey(key);
  if (fieldError.value) return;
  if (publicFields.value.includes(key) || props.source.secretKeys?.includes(key)) { fieldError.value = "字段已存在"; return; }
  if (fieldSecret.value || !canBePublic.value) (props.source.secretKeys ??= []).push(key);
  else (props.source.config ??= {})[key] = "";
  fieldName.value = ""; fieldSecret.value = true;
}
function removeField(key: string): void {
  delete props.source.config?.[key];
  delete props.secrets[key];
  props.source.secretKeys = props.source.secretKeys?.filter((item) => item !== key);
}
</script>

<template>
  <div class="opendal-fields">
    <label><span>名称</span><input v-model.trim="source.name" :disabled="disabled" type="text" /></label>
    <label><span>存储服务模板</span><ThemedSelect :model-value="source.scheme ?? 's3'" :options="options" :disabled="disabled" label="OpenDAL 存储服务" @update:model-value="setScheme" /></label>
    <label class="wide"><span>远端根目录</span><input v-model.trim="source.remotePath" :disabled="disabled" type="text" placeholder="/Chronicle" /><small>所有云端操作均限定在此目录内；更改配置后需重新测试。</small></label>
    <p v-if="source.scheme === 'github'" class="wide hint">此模板只使用仓库默认分支。需要自定义分支、建仓或批量提交时，请选择 GitHub 兼容源。</p>
    <p v-if="source.scheme === 'webdav'" class="wide hint">坚果云请优先使用 WebDAV 兼容源，以保留覆盖上传回退行为。</p>
    <label v-for="key in publicFields" :key="key"><span>{{ key }} <small>公开配置</small></span><div class="field-row"><input v-model.trim="source.config![key]" :disabled="disabled" type="text" :aria-label="`${key} 公开配置`" /><button :disabled="disabled" type="button" :title="`移除 ${key}`" :aria-label="`移除 ${key}`" @click="removeField(key)"><Trash2 :size="14" /></button></div></label>
    <label v-for="key in source.secretKeys ?? []" :key="key"><span>{{ key }} <small>机密 · Windows 凭据管理器</small></span><div class="field-row"><textarea v-if="key === 'credential' && source.scheme === 'gcs'" v-model="secrets[key]" :disabled="disabled" autocomplete="new-password" :aria-label="`${key} 机密配置`" :placeholder="credentialSaved ? '已保存，留空保留' : '粘贴服务账号 JSON'" /><input v-else v-model="secrets[key]" :disabled="disabled" type="password" autocomplete="new-password" :aria-label="`${key} 机密配置`" :placeholder="credentialSaved ? '已保存，留空保留' : '请输入机密值'" /><button :disabled="disabled" type="button" :title="`移除 ${key}`" :aria-label="`移除 ${key}`" @click="removeField(key)"><Trash2 :size="14" /></button></div><small v-if="key === 'credential' && source.scheme === 'gcs'">粘贴服务账号 JSON 内容；不会读取本机凭据文件。</small></label>
    <details class="wide"><summary>高级键值配置</summary><p class="hint">仅支持当前已编译服务的配置键。新增字段默认作为机密保存；未知字段不能设为公开。</p><div class="advanced-row"><label><span>字段名</span><input v-model.trim="fieldName" :disabled="disabled" :aria-invalid="Boolean(fieldError)" type="text" placeholder="例如 session_token" /></label><label class="secret-toggle"><input v-model="fieldSecret" :disabled="disabled || !canBePublic" type="checkbox" />机密值</label><button :disabled="disabled" type="button" @click="addField"><Plus :size="14" />添加字段</button></div><p v-if="fieldError" class="field-error" role="alert">{{ fieldError }}</p></details>
  </div>
</template>

<style scoped>
.opendal-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; padding: 14px 0; }
label { display: grid; gap: 6px; min-width: 0; font-size: 12px; color: var(--text); }
small, .hint { color: var(--text-3); font-size: 11px; line-height: 1.6; }
.wide { grid-column: 1 / -1; }
.hint { margin: 0; }
input[type="text"], input[type="password"], textarea { width: 100%; box-sizing: border-box; min-width: 0; height: 36px; border: 1px solid var(--border-2); border-radius: 6px; background: var(--field); color: var(--text); padding: 7px 10px; font: inherit; resize: vertical; }
.field-row { display: flex; align-items: flex-start; gap: 6px; }
.field-row > :first-child { flex: 1 1 auto; }
.field-row > button { flex: 0 0 36px; width: 36px; height: 36px; padding: 0; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; color: var(--text); background: var(--field); border: 1px solid var(--border-2); border-radius: 6px; padding: 7px; cursor: pointer; }
summary { font-size: 12px; cursor: pointer; color: var(--text); padding-bottom: 8px; }
.advanced-row { display: flex; flex-wrap: wrap; gap: 12px; align-items: end; margin-top: 8px; }
.secret-toggle { display: flex; align-items: center; height: 36px; }
.field-error { color: var(--danger, #b42318); font-size: 12px; }
input[aria-invalid="true"] { border-color: var(--danger, #b42318); }
button:focus-visible, input:focus-visible, summary:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
button:disabled, input:disabled { cursor: default; opacity: .55; }
@media (max-width: 620px) { .opendal-fields { grid-template-columns: 1fr; } }
</style>
