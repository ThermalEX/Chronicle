<script setup lang="ts">
import { t } from "../services/i18n";
import { Database, File, Folder, HardDrive, Plus, Trash2, UploadCloud, X } from "@lucide/vue";
import { onMounted, ref, watch } from "vue";
import type { ArchiveSource, CreateArchiveInput, SourceKind, StoragePolicy } from "../domain";
import { createBackdropDismissal } from "../services/dialogDismissal";
import { normalizeRegistryPath } from "../services/backupRules";
import TutorialHint from "./TutorialHint.vue";
import type { TutorialProgress, TutorialTip } from "../services/onboarding";
import ThemedSelect, { type ThemedSelectOption } from "./ThemedSelect.vue";

const props = defineProps<{
  sources: ArchiveSource[];
  defaultInitialSnapshot: boolean;
  picking?: SourceKind;
  submitting?: boolean;
  error?: string;
  editName?: string;
  editStoragePolicy?: StoragePolicy;
  editAutoBackupEnabled?: boolean;
  editAutomaticUploadEnabled?: boolean;
  highlightSources?: boolean;
  editExcludePatterns?: string[];
  tutorialProgress?: TutorialProgress;
}>();
const emit = defineEmits<{
  close: [];
  "tutorial-tip": [tip: TutorialTip];
  pick: [kind: Exclude<SourceKind, "registry">];
  registry: [path: string];
  remove: [id: string];
  submit: [input: CreateArchiveInput];
}>();

const name = ref(props.editName ?? "");
const excludePatterns = ref((props.editExcludePatterns ?? []).join("\n"));
const registryPath = ref("");
const registryError = ref("");
const registryInputOpen = ref(false);
const exclusionTipOpen = ref(false);
function addRegistry(): void {
  try {
    emit("registry", normalizeRegistryPath(registryPath.value));
    registryPath.value = "";
    registryError.value = "";
  } catch (error) { registryError.value = (error as Error).message; }
}
const storagePolicy = ref<StoragePolicy>(props.editStoragePolicy ?? "local");
const autoBackupEnabled = ref(props.editAutoBackupEnabled ?? false);
const automaticUploadEnabled = ref(props.editAutomaticUploadEnabled ?? false);
const createInitialSnapshot = ref(props.defaultInitialSnapshot);
const nameInput = ref<HTMLInputElement>();
const attempted = ref(false);
const backdrop = createBackdropDismissal(() => emit("close"), () => !props.submitting);

watch(() => props.sources, (sources) => {
  if (!name.value && sources.length === 1) name.value = sources[0].name;
}, { deep: true });

function submit(): void {
  attempted.value = true;
  if (!name.value.trim() || !props.sources.length || props.submitting) return;
  emit("submit", {
    name: name.value.trim(),
    sources: props.sources.map((source) => ({ ...source })),
    excludePatterns: excludePatterns.value.split(/\r?\n/).map((pattern) => pattern.trim()).filter(Boolean),
    storagePolicy: storagePolicy.value,
    createInitialSnapshot: createInitialSnapshot.value,
    syncMode: "manual",
    autoBackupEnabled: autoBackupEnabled.value,
    automaticUploadEnabled: automaticUploadEnabled.value,
  });
}

onMounted(() => nameInput.value?.focus());
</script>

<template>
  <div class="dialog-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <form class="create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-title" @submit.prevent="submit">
      <header>
        <div><p>{{ editName ? t('编辑存档') : t('新建存档') }}</p><h2 id="create-title">{{ editName ? t('修改存档设置') : t('添加到 Chronicle') }}</h2></div>
        <button type="button" :aria-label="t('关闭新建存档')" :title="t('关闭新建存档')" @click="emit('close')"><X :size="18" /></button>
      </header>

      <main>
        <label class="field" data-tour="name">
          <span>{{ t('存档名称') }}</span>
          <input ref="nameInput" v-model="name" type="text" maxlength="100" :placeholder="t('用于同步、查询和显示')" :aria-invalid="attempted && !name.trim()" />
          <small v-if="attempted && !name.trim()" class="field-error">{{ t('请输入存档名称') }}</small>
        </label>

        <fieldset data-tour="sources" :class="{ 'source-location-required': highlightSources }">
          <legend>{{ t('存档内容') }}</legend>
          <p>{{ highlightSources ? t('请重新选择该存档在本机的来源。') : t('一个存档可以包含多个文件、文件夹和注册表子键。') }}</p>
          <div class="source-actions">
            <button type="button" :disabled="Boolean(picking) || submitting" @click="emit('pick', 'file')"><File :size="16" />{{ picking === 'file' ? t('选择中') : t('添加文件') }}</button>
            <button type="button" :disabled="Boolean(picking) || submitting" @click="emit('pick', 'folder')"><Folder :size="16" />{{ picking === 'folder' ? t('选择中') : t('添加文件夹') }}</button>
            <button type="button" :disabled="submitting" :aria-expanded="registryInputOpen" @click="registryInputOpen = !registryInputOpen"><Database :size="16" />{{ t('添加注册表') }}</button>
          </div>
          <div v-if="registryInputOpen" class="registry-input field">
            <TutorialHint tip="registry" :progress="tutorialProgress" @seen="emit('tutorial-tip', $event)" />
            <label for="registry-path">{{ t('注册表子键路径') }}</label>
            <input id="registry-path" v-model="registryPath" placeholder="HKCU\Software\Example" :aria-invalid="Boolean(registryError)" aria-describedby="registry-help" @keydown.enter.prevent="addRegistry" />
            <small id="registry-help">{{ t('路径不包含“计算机”。仅 Windows 桌面版。支持手动备份和云端同步，不监听注册表变化。') }}</small>
            <small v-if="registryError" class="field-error" role="alert">{{ registryError }}</small>
            <button type="button" :disabled="submitting || !registryPath.trim()" @click="addRegistry">{{ t('添加子键') }}</button>
          </div>
          <div v-if="sources.length" class="source-list">
            <div v-for="source in sources" :key="source.id">
              <span class="source-icon"><Folder v-if="source.kind === 'folder'" :size="17" /><Database v-else-if="source.kind === 'registry'" :size="17" /><File v-else :size="17" /></span>
              <span><b>{{ source.name }}</b><small :title="source.path">{{ source.path }}</small></span>
              <button type="button" :aria-label="t('移除 {name}', { name: source.name })" :title="t('移除 {name}', { name: source.name })" :disabled="submitting" @click="emit('remove', source.id)"><Trash2 :size="15" /></button>
            </div>
          </div>
          <small v-else-if="attempted" class="field-error">{{ t('请至少添加一个来源') }}</small>
        </fieldset>

        <TutorialHint v-if="exclusionTipOpen" tip="exclusions" :progress="tutorialProgress" @seen="emit('tutorial-tip', $event)" />
        <label class="field exclusion-field" @focusin="exclusionTipOpen = true"><span>{{ t('排除规则（每行一条）') }}</span><textarea v-model="excludePatterns" rows="3" placeholder="*.tmp&#10;cache/" aria-describedby="exclusion-help"></textarea><small id="exclusion-help">{{ t('相对于每个文件来源的根目录；*.tmp 排除临时文件，cache/ 排除同名目录及其内容。仅影响后续快照和自动备份触发；恢复时保留排除的现有文件。注册表不应用这些规则。') }}</small></label>

        <div class="field" data-tour="storage"><span>{{ t('保存方式') }}</span><div class="storage-options">
            <label :class="{ selected: storagePolicy === 'local' }"><input v-model="storagePolicy" type="radio" value="local" /><HardDrive :size="16" /><span><b>{{ t('仅本地') }}</b><small>{{ t('保存在本机 Chronicle 仓库') }}</small></span></label>
            <label :class="{ selected: storagePolicy === 'local_and_remote' }"><input v-model="storagePolicy" type="radio" value="local_and_remote" /><UploadCloud :size="16" /><span><b>{{ t('本地与云端') }}</b><small>{{ t('云端接入后自动加入同步') }}</small></span></label>
          </div></div>

        <div data-tour="automation"><label class="initial-toggle"><span><b>{{ t('自动备份') }}</b><small>{{ t('仅监听文件和文件夹；合并时间内的变化保存为一份最新快照。注册表变化不会触发备份。') }}</small></span><input v-model="autoBackupEnabled" type="checkbox" role="switch" /></label>
        <label class="initial-toggle"><span><b>{{ t('自动上传') }}</b><small>{{ t('该存档生成新快照后自动上传到启用的云端同步源；仅“本地与云端”存档可上传。') }}</small></span><input v-model="automaticUploadEnabled" type="checkbox" role="switch" /></label></div>

        <label v-if="!editName" class="initial-toggle"><span><b>{{ t('创建后立即备份') }}</b><small>{{ t('生成第一个可恢复的 7z 时间节点') }}</small></span><input v-model="createInitialSnapshot" type="checkbox" role="switch" /></label>
        <p v-if="error" class="submit-error" role="alert">{{ error }}</p>
      </main>

      <footer><span>{{ t('{count} 个来源', { count: sources.length }) }}</span><div><button type="button" class="cancel" :disabled="submitting" @click="emit('close')">{{ t('取消') }}</button><button data-tour="create-submit" class="submit" type="submit" :disabled="submitting"><Plus :size="16" />{{ submitting ? (editName ? t('正在保存') : t('正在创建')) : (editName ? t('保存修改') : t('创建存档')) }}</button></div></footer>
    </form>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 50; inset: 0; display: grid; place-items: center; padding: 28px; background: #18181b99; backdrop-filter: blur(3px); }
.create-dialog { display: grid; grid-template-rows: 72px minmax(0, 1fr) 64px; width: min(720px, calc(100vw - 56px)); max-height: calc(100vh - 56px); overflow: hidden; background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }
header, footer { display: flex; align-items: center; justify-content: space-between; padding: 0 22px; }
header { border-bottom: 1px solid var(--border); } header p, header h2 { margin: 0; } header p { color: var(--primary); font-size: 9px; font-weight: 750; letter-spacing: .1em; } header h2 { margin-top: 4px; font-size: 19px; }
header button { display: grid; place-items: center; width: 38px; height: 38px; background: transparent; border-radius: 7px; } header button:hover, .cancel:hover { background: var(--hover); }
main { overflow-y: auto; padding: 23px 26px 28px; }
.field { display: flex; flex-direction: column; gap: 7px; } .field > span, legend { color: var(--text-2); font-size: 10px; font-weight: 700; }
input[type="text"] { width: 100%; height: 38px; padding: 0 11px; color: #263431; background: #f8faf9; border: 1px solid var(--border-2); border-radius: 7px; font-size: 11px; } input[aria-invalid="true"] { border-color: #b83a32; }
fieldset { margin: 20px 0; padding: 15px; border: 1px solid var(--border); border-radius: 9px; } legend { padding: 0 6px; } fieldset > p { margin: 0 0 12px; color: var(--text-3); font-size: 10px; }.source-location-required { background: #fff4d6; border-color: #f2cc60; box-shadow: 0 0 0 3px #f2cc6040; }.source-location-required > p { color: #9a6700; font-weight: 650; }.source-location-required .source-actions button { color: #9a6700; background: #fff9e8; border-color: #e4bc4d; }
.source-actions { display: flex; gap: 8px; } .source-actions button { display: inline-flex; align-items: center; gap: 7px; min-height: 35px; padding: 0 11px; color: var(--primary-dark); background: var(--primary-soft); border: 1px solid #c5ded8; border-radius: 7px; font-size: 10px; font-weight: 650; }
.source-list { margin-top: 12px; border: 1px solid var(--border); border-radius: 8px; overflow: hidden; } .source-list > div { display: grid; grid-template-columns: 32px minmax(0, 1fr) 34px; align-items: center; min-height: 54px; padding: 6px 8px; } .source-list > div + div { border-top: 1px solid var(--border); } .source-icon { display: grid; place-items: center; width: 28px; height: 28px; color: var(--primary); background: var(--primary-soft); border-radius: 6px; } .source-list span:nth-child(2) { display: flex; min-width: 0; flex-direction: column; gap: 3px; } .source-list b { font-size: 10px; } .source-list small { overflow: hidden; color: var(--text-3); font-size: 9px; text-overflow: ellipsis; white-space: nowrap; } .source-list button { display: grid; place-items: center; width: 32px; height: 32px; color: var(--text-3); background: transparent; border-radius: 6px; } .source-list button:hover { color: #b83a32; background: #fff0ef; }
.storage-options { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; } .storage-options > label { display: grid; grid-template-columns: 0 22px 1fr; align-items: center; min-height: 60px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; cursor: pointer; } .storage-options > label.selected { color: var(--primary-dark); background: var(--primary-soft); border-color: #9fcfc5; } .storage-options input { opacity: 0; width: 0; } .storage-options span { display: flex; flex-direction: column; gap: 3px; } .storage-options b { font-size: 10px; } .storage-options small { color: var(--text-3); font-size: 8px; line-height: 1.35; }
.initial-toggle { display: flex; align-items: center; justify-content: space-between; min-height: 60px; margin-top: 20px; padding: 10px 14px; background: #f5f8f7; border-radius: 8px; } .initial-toggle > span { display: flex; flex-direction: column; gap: 4px; } .initial-toggle b { font-size: 10px; } .initial-toggle small { color: var(--text-3); font-size: 9px; }
.initial-toggle input { position: relative; width: 38px; height: 22px; flex: none; appearance: none; background: #cbd5d1; border-radius: 20px; cursor: pointer; } .initial-toggle input::after { content: ""; position: absolute; top: 3px; left: 3px; width: 16px; height: 16px; background: #fff; border-radius: 50%; box-shadow: 0 1px 3px #102b2738; transition: transform .16s; } .initial-toggle input:checked { background: var(--primary); } .initial-toggle input:checked::after { transform: translateX(16px); }
.field-error, .submit-error { color: #a52e28; font-size: 9px; } .submit-error { margin: 13px 0 0; padding: 10px 12px; background: #fff0ef; border-radius: 7px; }
footer { border-top: 1px solid var(--border); color: var(--text-3); font-size: 9px; } footer > div { display: flex; gap: 8px; } footer button { display: inline-flex; align-items: center; gap: 6px; min-height: 36px; padding: 0 13px; border-radius: 7px; font-size: 10px; font-weight: 650; } .cancel { background: transparent; } .submit { color: #fff; background: var(--primary); } .submit:hover { background: var(--primary-dark); }
button:disabled { opacity: .55; cursor: default; }
.sync-mode-field { margin-top: 16px; }.sync-mode-field small { color: var(--text-3); font-size: 9px; }
.sync-mode-field :deep(.trigger) { height: 38px; border-radius: 7px; font-size: 11px; }
@media (max-width: 760px) { .storage-options { grid-template-columns: 1fr; } .create-dialog { width: calc(100vw - 28px); max-height: calc(100vh - 28px); } .dialog-backdrop { padding: 14px; } }
input[type="text"] { color: var(--text); background: var(--field); }
.storage-options > label.selected { border-color: color-mix(in srgb, var(--primary) 35%, var(--border)); }
.initial-toggle { background: var(--subtle); }
.initial-toggle input { background: var(--border-2); }
.initial-toggle input::after { background: var(--surface); }
.submit { color: var(--on-primary); }
.source-actions { flex-wrap: wrap; }
.registry-input { margin-top: 12px; }
.registry-input input, .exclusion-field textarea { width: 100%; padding: 9px 11px; color: var(--text); background: var(--field); border: 1px solid var(--border-2); border-radius: 7px; font: inherit; }
.registry-input button { align-self: flex-start; padding: 8px 12px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 7px; }
.exclusion-field { margin-bottom: 20px; }
.exclusion-field textarea { resize: vertical; min-height: 76px; }
.exclusion-field small, .registry-input small { color: var(--text-2); font-size: 11px; line-height: 1.5; }
</style>
