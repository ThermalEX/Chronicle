<script setup lang="ts">
import { File, Folder, HardDrive, Plus, Trash2, UploadCloud, X } from "@lucide/vue";
import { onMounted, ref, watch } from "vue";
import type { ArchiveSource, CreateArchiveInput, SourceKind, StoragePolicy } from "../domain";
import { createBackdropDismissal } from "../services/dialogDismissal";
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
}>();
const emit = defineEmits<{
  close: [];
  pick: [kind: SourceKind];
  remove: [id: string];
  submit: [input: CreateArchiveInput];
}>();

const name = ref(props.editName ?? "");
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
    sources: props.sources,
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
        <div><p>{{ editName ? '编辑存档' : '新建存档' }}</p><h2 id="create-title">{{ editName ? '修改存档设置' : '添加到 Chronicle' }}</h2></div>
        <button type="button" aria-label="关闭新建存档" title="关闭新建存档" @click="emit('close')"><X :size="18" /></button>
      </header>

      <main>
        <label class="field">
          <span>存档名称</span>
          <input ref="nameInput" v-model="name" type="text" maxlength="100" placeholder="用于同步、查询和显示" :aria-invalid="attempted && !name.trim()" />
          <small v-if="attempted && !name.trim()" class="field-error">请输入存档名称</small>
        </label>

        <fieldset>
          <legend>存档内容</legend>
          <p>一个存档可以同时包含多个文件和文件夹，每个时间节点会完整保存这些内容。</p>
          <div class="source-actions">
            <button type="button" :disabled="Boolean(picking) || submitting" @click="emit('pick', 'file')"><File :size="16" />{{ picking === 'file' ? '选择中' : '添加文件' }}</button>
            <button type="button" :disabled="Boolean(picking) || submitting" @click="emit('pick', 'folder')"><Folder :size="16" />{{ picking === 'folder' ? '选择中' : '添加文件夹' }}</button>
          </div>
          <div v-if="sources.length" class="source-list">
            <div v-for="source in sources" :key="source.id">
              <span class="source-icon"><Folder v-if="source.kind === 'folder'" :size="17" /><File v-else :size="17" /></span>
              <span><b>{{ source.name }}</b><small :title="source.path">{{ source.path }}</small></span>
              <button type="button" :aria-label="`移除 ${source.name}`" :title="`移除 ${source.name}`" :disabled="submitting" @click="emit('remove', source.id)"><Trash2 :size="15" /></button>
            </div>
          </div>
          <small v-else-if="attempted" class="field-error">请至少添加一个文件或文件夹</small>
        </fieldset>

        <div class="field"><span>保存方式</span><div class="storage-options">
            <label :class="{ selected: storagePolicy === 'local' }"><input v-model="storagePolicy" type="radio" value="local" /><HardDrive :size="16" /><span><b>仅本地</b><small>保存在本机 Chronicle 仓库</small></span></label>
            <label :class="{ selected: storagePolicy === 'local_and_remote' }"><input v-model="storagePolicy" type="radio" value="local_and_remote" /><UploadCloud :size="16" /><span><b>本地与云端</b><small>云端接入后自动加入同步</small></span></label>
          </div></div>

        <label class="initial-toggle"><span><b>自动备份</b><small>监听该存档的本机来源；文件变化停止后，按设置的静默时间自动创建快照。</small></span><input v-model="autoBackupEnabled" type="checkbox" role="switch" /></label>
        <label class="initial-toggle"><span><b>自动上传</b><small>该存档生成新快照后自动上传到启用的云端同步源；仅“本地与云端”存档可上传。</small></span><input v-model="automaticUploadEnabled" type="checkbox" role="switch" /></label>

        <label v-if="!editName" class="initial-toggle"><span><b>创建后立即备份</b><small>生成第一个可恢复的 7z 时间节点</small></span><input v-model="createInitialSnapshot" type="checkbox" role="switch" /></label>
        <p v-if="error" class="submit-error" role="alert">{{ error }}</p>
      </main>

      <footer><span>{{ sources.length }} 个来源</span><div><button type="button" class="cancel" :disabled="submitting" @click="emit('close')">取消</button><button class="submit" type="submit" :disabled="submitting"><Plus :size="16" />{{ submitting ? (editName ? '正在保存' : '正在创建') : (editName ? '保存修改' : '创建存档') }}</button></div></footer>
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
fieldset { margin: 20px 0; padding: 15px; border: 1px solid var(--border); border-radius: 9px; } legend { padding: 0 6px; } fieldset > p { margin: 0 0 12px; color: var(--text-3); font-size: 10px; }
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
</style>
