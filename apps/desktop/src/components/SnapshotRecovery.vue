<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { RotateCcw, Trash2, RefreshCw } from "@lucide/vue";
import { t, locale } from "../services/i18n";
import { cloudSettings, enabledCloudSources } from "../services/settings";
import { deviceLabel, deviceRepository, readKnownDevices, type DeviceIdentity } from "../services/devices";
import type { ProtocolSnapshot, OperationResult } from "../services/snapshotSync";
import ConfirmDialog from "./ConfirmDialog.vue";
interface RecoveryItem { operationId: string; snapshot: ProtocolSnapshot; device: { id: string; name: string; revision: number }; reason: string; deletedAtMs: number; purged: boolean; restoredSnapshot: ProtocolSnapshot | null; restoreCommitted: boolean; local?: boolean; remoteSources?: string[]; conflict?: boolean }
interface RemoteRecovery { sourceId: string; sourceName: string; error: string | null; records: { record: RecoveryItem; conflict: boolean }[] }
const emit = defineEmits<{ changed: [] }>();
const items = ref<RecoveryItem[]>([]);
const knownDevices = ref<DeviceIdentity[]>([]);
const busy = ref(false);
const error = ref("");
const selectedSources = ref<string[]>([]);
const purgeLocal = ref(true);
const confirming = ref<RecoveryItem>();
const outcomes = ref<OperationResult[]>([]);
const remoteResults = ref<RemoteRecovery[]>([]);
const sources = enabledCloudSources(cloudSettings);
async function load(): Promise<void> {
  if (!isTauri()) return;
  try {
    items.value = (await invoke<RecoveryItem[]>("list_snapshot_recovery")).map((record) => ({ ...record, local: true })); mergeRemote();
    const local = await deviceRepository.read();
    const known = (await readKnownDevices()).flatMap((source) => source.devices).sort((a, b) => b.revision - a.revision);
    knownDevices.value = [local, ...known.filter((device, index) => device.id !== local.id && known.findIndex((candidate) => candidate.id === device.id) === index)];
  }
  catch (cause) { error.value = t(String(cause)); }
}
function mergeRemote(): void {
  for (const source of remoteResults.value) for (const { record, conflict } of source.records) {
    let item = items.value.find((item) => item.operationId === record.operationId);
    if (!item) { item = { ...record, local: false, remoteSources: [], conflict }; items.value.push(item); }
    if (!record.purged && !conflict) item.remoteSources = [...new Set([...(item.remoteSources ?? []), source.sourceId])];
  }
  for (const item of items.value) if (item.remoteSources?.length || item.local) item.conflict = false;
}
async function readCloud(): Promise<void> {
  busy.value = true; error.value = "";
  try { remoteResults.value = await invoke("read_remote_snapshot_recovery", { sourceIds: sources.map((source) => source.id) }); await load(); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function restore(item: RecoveryItem): Promise<void> {
  busy.value = true; error.value = "";
  try {
    if (item.local && !item.purged) await invoke("restore_snapshot_recovery", { operationId: item.operationId });
    else await invoke("restore_remote_snapshot_recovery", { sourceId: item.remoteSources?.[0], operationId: item.operationId });
    await load(); emit("changed");
  }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function purge(): Promise<void> {
  const item = confirming.value; confirming.value = undefined;
  if (!item) return;
  busy.value = true; error.value = "";
  try { outcomes.value = await invoke("purge_snapshot_recovery", { operationId: item.operationId, sourceIds: selectedSources.value, purgeLocal: purgeLocal.value && Boolean(item.local), confirmed: true }); await readCloud(); emit("changed"); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
onMounted(load);
</script>

<template>
  <section class="snapshot-recovery" aria-labelledby="snapshot-recovery-title">
    <div class="heading"><h3 id="snapshot-recovery-title">{{ t('快照回收区') }}</h3><p>{{ t('快照回收独立于存档回收站。恢复生成新节点，删除记录长期保留。') }}</p></div>
    <button class="snapshot-control" :disabled="busy || !sources.length" @click="readCloud"><RefreshCw :size="14" />{{ t('读取云端回收区') }}</button>
    <p v-for="source in remoteResults.filter((source) => source.error)" :key="source.sourceId" role="alert">{{ source.sourceName }}：{{ t(source.error!) }}</p>
    <div class="purge-targets"><span>{{ t('永久清理位置') }}</span><label><input v-model="purgeLocal" type="checkbox" />{{ t('本机') }}</label><label v-for="source in sources" :key="source.id"><input v-model="selectedSources" type="checkbox" :value="source.id" />{{ source.name }}</label></div>
    <p v-if="error" role="alert">{{ error }}</p>
    <div v-for="item in items" :key="item.operationId" class="recovery-item"><span><b>{{ item.snapshot.title }}</b><small>{{ new Date(item.snapshot.created_at_ms).toLocaleString(locale) }} · {{ deviceLabel(item.snapshot.device_id, item.snapshot.device_name, knownDevices) }}</small><small>{{ t('删除设备') }}：{{ deviceLabel(item.device.id, item.device.name, knownDevices) }} · {{ t(item.reason === 'retention' ? '保留策略清理' : '手动删除') }}</small><small v-if="item.local && item.purged">{{ t('本机文件已永久清理；删除记录保留') }}</small><small v-if="item.conflict">{{ t('删除与锁定或元数据修改冲突，保留内容') }}</small><small v-if="item.restoreCommitted && item.restoredSnapshot">{{ t('已恢复为新节点') }} · {{ item.restoredSnapshot.id.slice(0, 8) }}</small></span><div><button class="snapshot-control" :disabled="busy || Boolean(item.conflict) || (item.purged && !item.remoteSources?.length) || (!item.local && !item.remoteSources?.length) || item.restoreCommitted" @click="restore(item)"><RotateCcw :size="14" />{{ t('恢复') }}</button><button class="snapshot-control danger" :disabled="busy || (!(purgeLocal && item.local) && !selectedSources.length)" @click="confirming = item"><Trash2 :size="14" />{{ t('永久清理') }}</button></div></div>
    <p v-if="!items.length">{{ t('快照回收区为空') }}</p>
    <p v-for="(outcome, index) in outcomes" :key="index" role="status">{{ outcome.sourceId === 'local' ? t('本机') : sources.find((source) => source.id === outcome.sourceId)?.name || outcome.sourceId }}：{{ t(outcome.error || '已完成') }}</p>
    <ConfirmDialog v-if="confirming" :title="t('永久清理')" :message="t('永久删除所选位置的回收快照文件，无法恢复。删除记录仍会保留，旧节点不会复活。未选位置不会清理。')" destructive @cancel="confirming = undefined" @confirm="purge" />
  </section>
</template>

<style scoped>
.snapshot-recovery { margin-top: 28px; } h3 { font-size: 18px; } .heading p { color: var(--text-3); font-size: 11px; line-height: 1.6; }
.purge-targets { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; font-size: 11px; margin: 16px 0; } .purge-targets label { display: flex; gap: 6px; align-items: center; }
.recovery-item { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px; border: 1px solid var(--border); border-bottom-width: 0; } .recovery-item:last-of-type { border-bottom-width: 1px; }
.recovery-item span { display: grid; gap: 5px; min-width: 0; } .recovery-item b { font-size: 12px; } small { color: var(--text-3); font-size: 10px; overflow-wrap: anywhere; } .recovery-item > div { display: flex; gap: 8px; } .danger { color: var(--danger); background: var(--danger-soft); } [role="alert"] { color: var(--danger); font-size: 12px; }
@media (max-width: 700px) { .recovery-item { align-items: flex-start; flex-direction: column; } }
</style>
