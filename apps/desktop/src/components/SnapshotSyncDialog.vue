<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { X, RefreshCw, UploadCloud, LoaderCircle } from "@lucide/vue";
import { listen } from "@tauri-apps/api/event";
import { t, locale } from "../services/i18n";
import { deviceLabel } from "../services/devices";
import { snapshotSync, defaultSelection, executable, type SyncPlan, type SyncOperation, type OperationResult } from "../services/snapshotSync";
import ConfirmDialog from "./ConfirmDialog.vue";
const props = defineProps<{ sourceIds: string[]; archives: { id: string; name: string }[]; snapshotId?: string; allArchives?: boolean }>();
const emit = defineEmits<{ close: []; changed: [] }>();
const plan = ref<SyncPlan>();
const selected = ref<string[]>([]);
const results = ref<OperationResult[]>([]);
const error = ref("");
const busy = ref(false);
const phase = ref<"preview" | "apply" | "enable">("preview");
const sourcesChecked = ref(0);
const requestId = ref("");
const enabling = ref<string>();
const closeButton = ref<HTMLButtonElement>();
let dispose: (() => void) | undefined;
const names: Record<SyncOperation["action"], string> = { upload: "上传", download: "下载", uploadRevision: "上传备注与锁定", downloadRevision: "下载备注与锁定", recycleLocal: "回收本机快照", recycleRemote: "回收云端快照", conflict: "冲突", unchanged: "无需处理" };
const operations = computed(() => plan.value?.sources.flatMap((source) => source.operations) ?? []);
const ancillaryResults = computed(() => results.value.filter((result) => !operations.value.some((op) => op.id === result.operationId)));
const progress = computed(() => {
  const total = phase.value === "enable" ? 0 : phase.value === "preview" ? props.sourceIds.length : selected.value.length;
  const completed = phase.value === "preview" ? sourcesChecked.value : phase.value === "enable" ? 0
    : new Set(results.value.filter((result) => selected.value.includes(result.operationId)).map((result) => result.operationId)).size;
  const current = Math.max(0, Math.min(total, completed));
  return { total, current, determinate: total > 0 && current > 0, percent: total > 0 ? Math.round(current / total * 100) : 0 };
});
const progressLabel = computed(() => phase.value === "enable" ? t('正在启用同步协议…')
  : phase.value === "preview" ? t('正在检查同步源…')
  : progress.value.total > 0 && progress.value.current === progress.value.total ? t('正在更新同步记录…') : t('正在同步…'));
function groups(ops: SyncOperation[], names: Record<string, string>) { return [...new Set(ops.map((op) => op.snapshot.entry_id))].map((id) => ({ id, name: props.archives.find((archive) => archive.id === id)?.name || names[id] || id, ops: ops.filter((op) => op.snapshot.entry_id === id) })); }
function count(ops: SyncOperation[], action: SyncOperation["action"]) { return ops.filter((op) => op.action === action).length; }
function selectDeletes(): void { selected.value = [...new Set([...selected.value, ...operations.value.filter((op) => op.action === "recycleLocal" || op.action === "recycleRemote").map((op) => op.id)])]; }
function date(value: number): string { return new Date(value).toLocaleString(locale.value); }
function bytes(value: number): string { return value >= 1024 ** 2 ? `${(value / 1024 ** 2).toFixed(1)} MB` : `${Math.ceil(value / 1024)} KB`; }
async function preview(): Promise<void> {
  phase.value = "preview";
  busy.value = true; error.value = ""; results.value = []; plan.value = undefined; sourcesChecked.value = 0;
  requestId.value = crypto.randomUUID();
  try { plan.value = await snapshotSync.preview(requestId.value, props.sourceIds, props.allArchives ? [] : props.archives.map((archive) => archive.id), props.snapshotId); selected.value = defaultSelection(operations.value); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function apply(): Promise<void> {
  if (!plan.value) return;
  phase.value = "apply";
  busy.value = true; error.value = ""; requestId.value = plan.value.id;
  try { results.value = await snapshotSync.apply(plan.value.id, selected.value); emit("changed"); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function enable(): Promise<void> {
  const id = enabling.value; enabling.value = undefined;
  if (!id) return;
  phase.value = "enable";
  busy.value = true;
  try { await snapshotSync.enable(id); await preview(); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function cancel(): Promise<void> { try { await snapshotSync.cancel(requestId.value); } catch (cause) { error.value = t(String(cause)); } }
onMounted(async () => {
  closeButton.value?.focus();
  try {
    dispose = await listen<{ requestId: string; results?: OperationResult[]; sourcesChecked?: number }>("snapshot-sync-progress", ({ payload }) => { if (payload.requestId !== requestId.value) return; if (payload.results) results.value = payload.results; if (payload.sourcesChecked !== undefined) sourcesChecked.value = payload.sourcesChecked; });
    await preview();
  } catch (cause) { error.value = t(String(cause)); }
});
onBeforeUnmount(() => { dispose?.(); if (busy.value) void cancel(); });
</script>

<template>
  <div class="sync-plan-backdrop" @keydown.esc.stop="busy ? cancel() : emit('close')">
    <section class="sync-plan-dialog" role="dialog" aria-modal="true" aria-labelledby="sync-plan-title">
      <header><div><p class="label">{{ t('手动同步') }}</p><h2 id="sync-plan-title">{{ t('同步预览') }}</h2><p>{{ t('核对上传、下载与删除。删除内容先进入回收区，不会恢复到游戏目录。') }}</p></div><button class="snapshot-control close-button" ref="closeButton" :disabled="busy" :aria-label="t('关闭')" :title="t('关闭')" @click="emit('close')"><X :size="18" /></button></header>
      <div class="sync-plan-toolbar"><button class="snapshot-control" :disabled="busy" @click="preview"><RefreshCw :size="15" />{{ t('重新预览') }}</button><button class="snapshot-control" :disabled="busy || !plan" @click="selectDeletes">{{ t('选择全部删除项') }}</button>
        <div v-if="busy" class="sync-progress">
          <LoaderCircle class="sync-spinner" :size="18" aria-hidden="true" />
          <div class="sync-progress-body">
            <div class="sync-progress-label" role="status"><span>{{ progressLabel }}</span><small v-if="progress.total">{{ progress.current }} / {{ progress.total }}</small></div>
            <div class="sync-progress-track" :class="{ indeterminate: !progress.determinate }" role="progressbar" :aria-label="progressLabel" :aria-valuemin="progress.determinate ? 0 : undefined" :aria-valuemax="progress.determinate ? 100 : undefined" :aria-valuenow="progress.determinate ? progress.percent : undefined">
              <span :style="progress.determinate ? { width: progress.percent + '%' } : undefined"></span>
            </div>
          </div>
        </div>
      </div>
      <p v-if="error" class="sync-plan-error" role="alert">{{ error }}</p>
      <p v-for="(result, index) in ancillaryResults" :key="index" class="sync-plan-error" role="status">{{ plan?.sources.find((source) => source.sourceId === result.sourceId)?.sourceName }} · {{ t(result.error || '目录与设备记录已更新') }}</p>
      <main>
        <section v-for="source in plan?.sources" :key="source.sourceId" class="sync-source">
          <div class="sync-source-heading"><h3>{{ source.sourceName }}</h3><button class="snapshot-control" v-if="source.upgradeRequired" :disabled="busy" @click="enabling = source.sourceId">{{ t('启用新同步协议') }}</button></div>
          <p v-if="source.error" class="sync-plan-error" role="alert">{{ t('此源预览失败') }}：{{ t(source.error!) }}</p>
          <template v-else>
            <p v-if="source.upgradeRequired" class="sync-plan-warning">{{ t('启用前须确认同库所有设备已升级；旧版混用不受支持。') }}</p>
            <div class="sync-counts"><span v-for="action in (['upload', 'download', 'uploadRevision', 'downloadRevision', 'recycleLocal', 'recycleRemote', 'conflict', 'unchanged'] as const)" :key="action">{{ t(names[action]) }} <b>{{ count(source.operations, action) }}</b></span></div>
            <div v-for="group in groups(source.operations, source.archiveNames)" :key="group.id" class="sync-entry"><h4>{{ group.name }}</h4>
              <label v-for="op in group.ops" :key="op.id" class="sync-operation"><input v-model="selected" type="checkbox" :value="op.id" :disabled="busy || !executable(op) || source.upgradeRequired" /><span><b>{{ op.snapshot.title }}</b><small>{{ date(op.snapshot.created_at_ms) }} · {{ bytes(op.snapshot.size_bytes) }} · {{ deviceLabel(op.snapshot.device_id, op.snapshot.device_name, source.devices) }}</small><small v-if="op.deletion">{{ t('删除设备') }}：{{ deviceLabel(op.deletion.device.id, op.deletion.device.name, source.devices) }}</small><small v-if="op.reason" class="sync-plan-warning">{{ t(op.reason) }}</small><small v-if="results.find((result) => result.operationId === op.id)" role="status">{{ t(results.find((result) => result.operationId === op.id)?.error || '已完成') }}</small></span><span class="sync-action" :class="{ conflict: op.action === 'conflict' }">{{ t(names[op.action]) }}</span></label>
            </div>
            <p v-if="!source.operations.length">{{ t('无需处理') }}</p>
          </template>
        </section>
      </main>
      <footer><small>{{ t('取消只停止后续操作，已完成的操作不会撤销。失败项请重新预览后重试。') }}</small><button class="snapshot-control" v-if="busy" @click="cancel">{{ t('取消') }}</button><button v-else class="snapshot-control primary" :disabled="!plan || !plan.sources.some((source) => !source.error && !source.upgradeRequired) || results.length > 0" @click="apply"><UploadCloud :size="16" />{{ t('执行所选操作') }} ({{ selected.length }})</button></footer>
    </section>
    <ConfirmDialog v-if="enabling" :title="t('启用新同步协议')" :message="t('请确认使用此云端资料库的所有设备均已升级。旧版本可能重新上传已删除节点，不支持混用。已有快照将验证并登记，不推断历史删除。')" @confirm="enable" @cancel="enabling = undefined" />
  </div>
</template>

<style scoped>
.sync-progress { display: flex; align-items: center; gap: 10px; flex: 1; min-width: min(220px, 100%); }
.sync-spinner { flex-shrink: 0; color: var(--primary); animation: sync-spinner-rotate 1s linear infinite; }
.sync-progress-body { flex: 1; min-width: 0; }
.sync-progress-label { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 7px; color: var(--text-2); font-size: 11px; line-height: 1.5; }
.sync-progress-label small { flex-shrink: 0; color: var(--text-3); font-size: 10px; font-variant-numeric: tabular-nums; }
.sync-progress-track { height: 5px; overflow: hidden; border-radius: 999px; background: var(--border); }
.sync-progress-track > span { display: block; height: 100%; border-radius: inherit; background: var(--primary); transition: width .25s ease; }
.sync-progress-track.indeterminate > span { width: 35%; animation: sync-progress-flow 1.4s ease-in-out infinite; }
@keyframes sync-spinner-rotate { to { transform: rotate(360deg); } }
@keyframes sync-progress-flow { from { transform: translateX(-100%); } to { transform: translateX(290%); } }
@media (prefers-reduced-motion: reduce) { .sync-spinner, .sync-progress-track.indeterminate > span { animation: none; } .sync-progress-track > span { transition: none; } }
.sync-plan-dialog .close-button { display: grid; place-items: center; flex-shrink: 0; width: 38px; height: 38px; min-height: 38px; padding: 0; background: transparent; border: 0; border-radius: 7px; color: var(--text); }
.sync-plan-dialog .close-button:hover { background: var(--hover); }
.sync-plan-backdrop { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 24px; background: var(--modal-backdrop, #10182799); backdrop-filter: blur(5px); }
.sync-plan-dialog { width: min(920px, 100%); max-height: min(840px, 90vh); display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border); border-radius: 16px; box-shadow: 0 24px 80px var(--shadow-color); overflow: hidden; }
header, footer { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 22px 26px; }
header { border-bottom: 1px solid var(--border); align-items: flex-start; } header h2 { margin: 5px 0 10px; font-size: 22px; } header p, footer small { color: var(--text-3); font-size: 12px; line-height: 1.6; }
.sync-plan-toolbar { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; padding: 16px 26px; } main { overflow: auto; min-height: 180px; padding: 0 26px 20px; }
.sync-source { border: 1px solid var(--border); border-radius: 10px; padding: 16px; margin-bottom: 16px; } .sync-source-heading { display: flex; justify-content: space-between; align-items: center; gap: 12px; } h3 { font-size: 16px; } h4 { font-size: 13px; margin: 16px 0 8px; }
.sync-counts { display: flex; gap: 12px; flex-wrap: wrap; font-size: 11px; color: var(--text-3); margin: 12px 0; } .sync-counts b { color: var(--text); }
.sync-operation { display: flex; align-items: center; gap: 12px; padding: 12px 0; border-top: 1px solid var(--border); } .sync-operation > span:first-of-type { flex: 1; min-width: 0; display: grid; gap: 4px; } .sync-operation b { font-size: 13px; } .sync-operation small { color: var(--text-3); overflow-wrap: anywhere; font-size: 11px; line-height: 1.5; }
.sync-action { color: var(--primary); font-size: 12px; } .sync-plan-error, .sync-plan-warning, .conflict { color: var(--danger); font-size: 12px; } .sync-plan-dialog > .sync-plan-error { padding: 0 26px; }
footer { border-top: 1px solid var(--border); } @media (max-width: 700px) { .sync-plan-backdrop { padding: 10px; } header, footer { padding: 16px; } .sync-plan-toolbar { flex-wrap: wrap; padding: 12px 16px; } main { padding: 0 16px 16px; } }
</style>
