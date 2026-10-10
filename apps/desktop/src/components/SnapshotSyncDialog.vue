<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { X, RefreshCw, UploadCloud, LoaderCircle, ChevronRight, Check, CircleAlert } from "@lucide/vue";
import { listen } from "@tauri-apps/api/event";
import { t, locale } from "../services/i18n";
import { deviceLabel } from "../services/devices";
import { snapshotSync, defaultPlanSelection, executable, type SyncPlan, type SyncOperation, type OperationResult } from "../services/snapshotSync";
import ConfirmDialog from "./ConfirmDialog.vue";
const props = defineProps<{ sourceIds: string[]; archives: { id: string; name: string }[]; snapshotId?: string; allArchives?: boolean }>();
const emit = defineEmits<{ close: []; changed: [] }>();
const plan = ref<SyncPlan>();
const selected = ref<string[]>([]);
const appliedSelection = ref<string[]>([]);
const expandedSources = ref(new Set<string>());
const expandedArchives = ref(new Set<string>());
const finishedSources = ref(new Set<string>());
const activeSources = ref(new Set<string>());
const results = ref<OperationResult[]>([]);
const error = ref("");
const busy = ref(false);
const phase = ref<"preview" | "apply" | "enable">("preview");
const sourcesChecked = ref(0);
const requestId = ref("");
const enabling = ref<string>();
const pendingApply = ref<{ planId: string; operationIds: string[]; recycling: { id: string; sourceName: string; archiveName: string; title: string; timeLabel: string; action: SyncOperation["action"] }[] }>();
const closeButton = ref<HTMLButtonElement>();
let dispose: (() => void) | undefined;
const names: Record<SyncOperation["action"], string> = { upload: "上传", download: "下载", uploadRevision: "上传备注与锁定", downloadRevision: "下载备注与锁定", recycleLocal: "回收本机快照", recycleRemote: "回收云端快照", conflict: "冲突", unchanged: "无需处理" };
const operations = computed(() => plan.value?.sources.flatMap((source) => source.operations) ?? []);
const availableOperations = computed(() => plan.value?.sources.filter(source => !source.error && !source.upgradeRequired).flatMap(source => source.operations) ?? []);
const recyclingOperations = computed(() => availableOperations.value.filter(op => op.action === "recycleLocal" || op.action === "recycleRemote"));
const operationIds = computed(() => new Set([...operations.value.map(operation => operation.id), ...(plan.value?.sources.flatMap(source => [source.indexRepairId, source.deviceRecordId].filter((id): id is string => Boolean(id))) ?? [])]));
const selectedSet = computed(() => new Set(selected.value));
const allRecyclingSelected = computed(() => recyclingOperations.value.length > 0 && recyclingOperations.value.every(op => selectedSet.value.has(op.id)));
const resultsById = computed(() => new Map(results.value.map(result => [result.operationId, result])));
const ancillaryResults = computed(() => results.value.filter(result => !operationIds.value.has(result.operationId)));
const sourceNames = computed(() => new Map(plan.value?.sources.map(source => [source.sourceId, source.sourceName])));
const actions = ["upload", "download", "uploadRevision", "downloadRevision", "recycleLocal", "recycleRemote", "conflict", "unchanged"] as const;
const sourceViews = computed(() => {
  const archiveNames = new Map(props.archives.map(archive => [archive.id, archive.name]));
  const formatter = new Intl.DateTimeFormat(locale.value, { year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", second: "numeric" });
  const labels = new Map<number, string>();
  return plan.value?.sources.map(source => {
    const counts = Object.fromEntries(actions.map(action => [action, 0])) as Record<SyncOperation["action"], number>;
    const groups = new Map<string, { id: string; name: string; changeCount: number; ops: Array<SyncOperation & { timeLabel: string }> }>();
    for (const op of source.operations) {
      counts[op.action]++;
      const snapshot = op.snapshot, id = snapshot.entry_id;
      let group = groups.get(id);
      if (!group) { group = { id, name: archiveNames.get(id) || source.archiveNames[id] || id, changeCount: 0, ops: [] }; groups.set(id, group); }
      if (op.action !== "unchanged") group.changeCount++;
      let label = labels.get(snapshot.created_at_ms);
      if (label === undefined) { label = formatter.format(snapshot.created_at_ms); labels.set(snapshot.created_at_ms, label); }
      group.ops.push({ ...op, snapshot, timeLabel: label });
    }
    return { ...source, counts, changeCount: source.operations.length - counts.unchanged + (source.indexRepairId ? 1 : 0) + (source.deviceRecordId ? 1 : 0), groups: [...groups.values()] };
  }) ?? [];
});
const appliedSet = computed(() => new Set(appliedSelection.value));
const sourceTotals = computed(() => new Map(plan.value?.sources.map(source => [source.sourceId,
  source.operations.filter(op => appliedSet.value.has(op.id)).length + [source.indexRepairId, source.deviceRecordId].filter(id => id && appliedSet.value.has(id)).length,
])));
const sourceProgress = computed(() => {
  const states = new Map(plan.value?.sources.map(source => [source.sourceId, {
    total: sourceTotals.value.get(source.sourceId) ?? 0, current: 0, failed: Boolean(source.error || source.upgradeRequired),
    finished: finishedSources.value.has(source.sourceId), completed: false, percent: 0, active: activeSources.value.has(source.sourceId), issues: [] as OperationResult[],
  }]));
  for (const result of results.value) {
    const state = states.get(result.sourceId);
    if (!state) continue;
    if (appliedSet.value.has(result.operationId)) state.current++;
    if (result.status !== "success") { state.failed = true; state.issues.push(result); }
  }
  for (const state of states.values()) {
    state.current = Math.min(state.current, state.total);
    state.completed = state.total > 0 && state.finished && !state.failed && state.current === state.total;
    state.percent = state.total ? Math.round(state.current / state.total * 100) : state.completed ? 100 : 0;
  }
  return states;
});
function sourceStatus(sourceId: string): string {
  const state = sourceProgress.value.get(sourceId);
  if (state?.failed) return t('同步失败');
  if (!state?.total) return t('未选择操作，已跳过');
  if (state?.completed) return t('同步完成');
  if (state?.active) return state.current === state.total ? t('正在更新同步记录…') : t('正在同步…');
  return busy.value ? t('等待同步') : t('同步未完成');
}
function toggleSource(id: string): void {
  if (expandedSources.value.has(id)) expandedSources.value.delete(id); else expandedSources.value.add(id);
}
function archiveKey(sourceId: string, entryId: string): string { return JSON.stringify([sourceId, entryId]); }
function toggleArchive(sourceId: string, entryId: string): void {
  const key = archiveKey(sourceId, entryId);
  if (expandedArchives.value.has(key)) expandedArchives.value.delete(key); else expandedArchives.value.add(key);
}
interface SyncProgressEvent { requestId: string; results?: OperationResult[]; sourcesChecked?: number; activeSourceId?: string; finishedSourceId?: string }
function acceptProgress(payload: SyncProgressEvent): void {
  if (payload.requestId !== requestId.value) return;
  if (payload.results) {
    const merged = new Map(results.value.map(result => [`${result.sourceId}:${result.operationId}`, result]));
    for (const result of payload.results) merged.set(`${result.sourceId}:${result.operationId}`, result);
    results.value = [...merged.values()];
  }
  if (payload.sourcesChecked !== undefined) sourcesChecked.value = payload.sourcesChecked;
  if (payload.activeSourceId) activeSources.value.add(payload.activeSourceId);
  if (payload.finishedSourceId) {
    finishedSources.value.add(payload.finishedSourceId);
    activeSources.value.delete(payload.finishedSourceId);
  }
}
const progress = computed(() => {
  const total = phase.value === "enable" ? 0 : phase.value === "preview" ? props.sourceIds.length : appliedSelection.value.length;
  const completed = phase.value === "preview" ? sourcesChecked.value : phase.value === "enable" ? 0
    : [...resultsById.value.keys()].filter(id => appliedSet.value.has(id)).length;
  const current = Math.max(0, Math.min(total, completed));
  return { total, current, determinate: total > 0 && current > 0, percent: total > 0 ? Math.round(current / total * 100) : 0 };
});
const progressLabel = computed(() => phase.value === "enable" ? t('正在启用同步协议…')
  : phase.value === "preview" ? t('正在检查同步源…')
  : progress.value.total > 0 && progress.value.current === progress.value.total ? t('正在更新同步记录…') : t('正在同步…'));
function selectDeletes(): void {
  const ids = new Set(recyclingOperations.value.map(op => op.id));
  selected.value = allRecyclingSelected.value ? selected.value.filter(id => !ids.has(id)) : [...new Set([...selected.value, ...ids])];
}
function bytes(value: number): string { return value >= 1024 ** 2 ? `${(value / 1024 ** 2).toFixed(1)} MB` : `${Math.ceil(value / 1024)} KB`; }
async function preview(): Promise<void> {
  phase.value = "preview";
  busy.value = true; error.value = ""; results.value = []; plan.value = undefined; sourcesChecked.value = 0;
  finishedSources.value.clear(); activeSources.value.clear(); appliedSelection.value = [];
  requestId.value = crypto.randomUUID();
  try { plan.value = await snapshotSync.preview(requestId.value, props.sourceIds, props.allArchives ? [] : props.archives.map((archive) => archive.id), props.snapshotId); selected.value = defaultPlanSelection(plan.value.sources); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; }
}
async function apply(): Promise<void> {
  if (!plan.value || busy.value || pendingApply.value) return;
  const operationIds = defaultPlanSelection(plan.value.sources).filter(id => selectedSet.value.has(id));
  const ids = new Set(operationIds);
  const recycling = sourceViews.value.filter(source => !source.error && !source.upgradeRequired).flatMap(source => source.groups.flatMap(group => group.ops
    .filter(op => ids.has(op.id) && (op.action === "recycleLocal" || op.action === "recycleRemote"))
    .map(op => ({ id: op.id, sourceName: source.sourceName, archiveName: group.name, title: op.snapshot.title, timeLabel: op.timeLabel, action: op.action }))));
  if (recycling.length) { pendingApply.value = { planId: plan.value.id, operationIds, recycling }; return; }
  await executeSelection(plan.value.id, operationIds);
}
function dismissRecycleConfirmation(): void { pendingApply.value = undefined; }
async function confirmRecycle(): Promise<void> {
  const pending = pendingApply.value;
  pendingApply.value = undefined;
  if (pending) await executeSelection(pending.planId, pending.operationIds);
}
async function executeSelection(planId: string, operationIds: string[]): Promise<void> {
  if (busy.value) return;
  if (!plan.value || plan.value.id !== planId) { error.value = t('本机或云端状态已变化，请重新预览'); return; }
  phase.value = "apply";
  busy.value = true; error.value = ""; requestId.value = planId;
  appliedSelection.value = [...operationIds]; finishedSources.value.clear(); activeSources.value.clear();
  try { results.value = await snapshotSync.apply(planId, appliedSelection.value); plan.value.sources.forEach(source => finishedSources.value.add(source.sourceId)); emit("changed"); }
  catch (cause) { error.value = t(String(cause)); }
  finally { busy.value = false; activeSources.value.clear(); }
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
function escape(): void {
  if (pendingApply.value) dismissRecycleConfirmation();
  else if (enabling.value) enabling.value = undefined;
  else if (busy.value) void cancel();
  else emit("close");
}
onMounted(async () => {
  closeButton.value?.focus();
  try {
    dispose = await listen<SyncProgressEvent>("snapshot-sync-progress", ({ payload }) => acceptProgress(payload));
    await preview();
  } catch (cause) { error.value = t(String(cause)); }
});
onBeforeUnmount(() => { dispose?.(); if (busy.value) void cancel(); });
</script>

<template>
  <div class="sync-plan-backdrop" @keydown.esc.stop="escape">
    <section class="sync-plan-dialog" :inert="Boolean(pendingApply || enabling)" role="dialog" aria-modal="true" aria-labelledby="sync-plan-title">
      <header><div><p class="label">{{ t('手动同步') }}</p><h2 id="sync-plan-title">{{ t('同步预览') }}</h2><p>{{ t('核对上传、下载与删除。删除内容先进入回收区，不会恢复到游戏目录。') }}</p></div><button class="snapshot-control close-button" ref="closeButton" :disabled="busy" :aria-label="t('关闭')" :title="t('关闭')" @click="emit('close')"><X :size="18" /></button></header>
      <div class="sync-plan-toolbar"><button class="snapshot-control" :disabled="busy" @click="preview"><RefreshCw :size="15" />{{ t('重新预览') }}</button><button class="snapshot-control" :disabled="busy || !recyclingOperations.length" @click="selectDeletes">{{ t(allRecyclingSelected ? '取消选择全部回收项' : '选择全部回收项') }}</button>
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
      <p v-for="(result, index) in ancillaryResults" :key="index" class="sync-plan-error" role="status">{{ sourceNames.get(result.sourceId) }} · {{ t(result.error || '目录与设备记录已更新') }}</p>
      <main>
        <section v-for="source in sourceViews" :key="source.sourceId" class="sync-source">
          <div class="sync-source-heading">
            <h3><button class="snapshot-control sync-disclosure" :aria-expanded="expandedSources.has(source.sourceId)" :aria-controls="`sync-source-${source.sourceId}`" @click="toggleSource(source.sourceId)">
              <ChevronRight :size="17" class="sync-chevron" :class="{ expanded: expandedSources.has(source.sourceId) }" aria-hidden="true" /><span>{{ source.sourceName }}</span>
              <span v-if="source.changeCount" class="sync-change-count" :aria-label="t('待处理变更：{count}', { count: source.changeCount })" :title="t('待处理变更：{count}', { count: source.changeCount })">{{ source.changeCount }}</span>
            </button></h3>
            <Check v-if="phase === 'apply' && sourceProgress.get(source.sourceId)?.completed" :size="21" class="sync-source-success" role="img" :aria-label="`${source.sourceName} · ${t('同步完成')}`" />
            <CircleAlert v-else-if="phase === 'apply' && sourceProgress.get(source.sourceId)?.failed" :size="20" class="sync-source-failed" role="img" :aria-label="`${source.sourceName} · ${t('同步失败')}`" />
            <button class="snapshot-control" v-if="source.upgradeRequired" :disabled="busy" @click="enabling = source.sourceId">{{ t('启用新同步协议') }}</button>
          </div>
          <div v-if="phase === 'apply'" class="sync-source-progress">
            <div class="sync-progress-label" role="status"><span><LoaderCircle v-if="sourceProgress.get(source.sourceId)?.active" class="sync-spinner" :size="14" aria-hidden="true" />{{ sourceStatus(source.sourceId) }}</span><small>{{ sourceProgress.get(source.sourceId)?.current }} / {{ sourceProgress.get(source.sourceId)?.total }}</small></div>
            <div v-if="sourceProgress.get(source.sourceId)?.total" class="sync-progress-track" :class="{ success: sourceProgress.get(source.sourceId)?.completed, failed: sourceProgress.get(source.sourceId)?.failed }" role="progressbar" :aria-label="`${source.sourceName} · ${sourceStatus(source.sourceId)}`" :aria-valuemin="0" :aria-valuemax="100" :aria-valuenow="sourceProgress.get(source.sourceId)?.percent"><span :style="{ width: sourceProgress.get(source.sourceId)?.percent + '%' }"></span></div>
          </div>
          <p v-for="issue in sourceProgress.get(source.sourceId)?.issues" :key="issue.operationId" class="sync-plan-error" role="alert">{{ t(issue.error || (issue.status === 'cancelled' ? '已取消' : '同步失败，服务未返回错误详情。')) }}</p>
          <p v-if="source.error" class="sync-plan-error" role="alert">{{ t('此源预览失败') }}：{{ t(source.error!) }}</p>
          <div v-else-if="expandedSources.has(source.sourceId)" :id="`sync-source-${source.sourceId}`">
            <p v-if="source.upgradeRequired" class="sync-plan-warning">{{ t('启用前须确认同库所有设备已升级；旧版混用不受支持。') }}</p>
            <div class="sync-counts"><span v-for="action in actions" :key="action">{{ t(names[action]) }} <b>{{ source.counts[action] }}</b></span></div>
            <label v-if="source.indexRepairId" class="sync-operation"><input v-model="selected" type="checkbox" :value="source.indexRepairId" :disabled="busy || source.upgradeRequired" /><span><b>{{ t('修复云端显示索引') }}</b><small>{{ t('根据已发布的同步记录修复显示，不上传或删除快照文件。') }}</small><small v-if="resultsById.get(source.indexRepairId)" role="status">{{ t(resultsById.get(source.indexRepairId)?.error || '已完成') }}</small></span></label>
            <label v-if="source.deviceRecordId" class="sync-operation"><input v-model="selected" type="checkbox" :value="source.deviceRecordId" :disabled="busy || source.upgradeRequired" /><span><b>{{ t('更新本机设备信息') }}</b><small>{{ t('仅在云端缺少当前设备或设备名称变更时执行。') }}</small><small v-if="resultsById.get(source.deviceRecordId)" role="status">{{ t(resultsById.get(source.deviceRecordId)?.error || '已完成') }}</small></span></label>
            <div v-for="group in source.groups" :key="group.id" class="sync-entry"><h4><button class="snapshot-control sync-disclosure" :aria-expanded="expandedArchives.has(archiveKey(source.sourceId, group.id))" :aria-controls="`sync-entry-${source.sourceId}-${group.id}`" @click="toggleArchive(source.sourceId, group.id)"><ChevronRight :size="15" class="sync-chevron" :class="{ expanded: expandedArchives.has(archiveKey(source.sourceId, group.id)) }" aria-hidden="true" /><span>{{ group.name }}</span><span v-if="group.changeCount" class="sync-change-count" :aria-label="t('待处理变更：{count}', { count: group.changeCount })" :title="t('待处理变更：{count}', { count: group.changeCount })">{{ group.changeCount }}</span></button></h4>
              <div v-if="expandedArchives.has(archiveKey(source.sourceId, group.id))" :id="`sync-entry-${source.sourceId}-${group.id}`">
                <label v-for="op in group.ops" :key="op.id" class="sync-operation"><input v-model="selected" type="checkbox" :value="op.id" :disabled="busy || !executable(op) || source.upgradeRequired" /><span><b>{{ op.snapshot.title }}</b><small>{{ op.timeLabel }} · {{ bytes(op.snapshot.size_bytes) }} · {{ deviceLabel(op.snapshot.device_id, op.snapshot.device_name, source.devices) }}</small><small v-if="op.deletion">{{ t('删除设备') }}：{{ deviceLabel(op.deletion.device.id, op.deletion.device.name, source.devices) }}</small><small v-if="op.reason" class="sync-plan-warning">{{ t(op.reason) }}</small><small v-if="resultsById.get(op.id)" role="status">{{ t(resultsById.get(op.id)?.error || (resultsById.get(op.id)?.status === 'success' ? '已完成' : resultsById.get(op.id)?.status === 'cancelled' ? '已取消' : '同步失败')) }}</small></span><span class="sync-action" :class="{ conflict: op.action === 'conflict' }">{{ t(names[op.action]) }}</span></label>
              </div>
            </div>
            <p v-if="!source.operations.length && !source.indexRepairId && !source.deviceRecordId">{{ t('无需处理') }}</p>
          </div>
        </section>
      </main>
      <footer><small>{{ t('取消只停止后续操作，已完成的操作不会撤销。失败项请重新预览后重试。') }}</small><button class="snapshot-control" v-if="busy" @click="cancel">{{ t('取消') }}</button><button v-else class="snapshot-control primary" :disabled="!plan || !plan.sources.some((source) => !source.error && !source.upgradeRequired) || results.length > 0" @click="apply"><UploadCloud :size="16" />{{ t('执行所选操作') }} ({{ selected.length }})</button></footer>
    </section>
    <ConfirmDialog v-if="enabling" :title="t('启用新同步协议')" :message="t('请确认使用此云端资料库的所有设备均已升级。旧版本可能重新上传已删除节点，不支持混用。已有快照将验证并登记，不推断历史删除。')" @confirm="enable" @cancel="enabling = undefined" />
    <ConfirmDialog v-if="pendingApply" :title="t('确认回收')" :message="t('将回收以下 {count} 项。本机快照进入回收区，云端快照逻辑回收，仍可恢复；不会永久删除文件。确认后执行全部所选同步操作。', { count: pendingApply.recycling.length })" :confirm-label="t('确认回收并同步')" destructive @confirm="confirmRecycle" @cancel="dismissRecycleConfirmation">
      <template #body-extra><ul class="sync-recycle-list"><li v-for="item in pendingApply.recycling" :key="item.id"><b>{{ item.sourceName }} · {{ item.archiveName }}</b><span>{{ item.title }} · {{ t(names[item.action]) }}</span><small>{{ item.timeLabel }}</small></li></ul></template>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.sync-progress { display: flex; align-items: center; gap: 10px; flex: 1; min-width: min(220px, 100%); }
.sync-recycle-list { max-height: min(240px, 35vh); overflow: auto; margin: 0; padding: 0 22px 16px; list-style: none; }
.sync-recycle-list li { display: grid; gap: 4px; padding: 10px 0; border-top: 1px solid var(--border); font-size: 12px; overflow-wrap: anywhere; }
.sync-recycle-list span, .sync-recycle-list small { color: var(--text-3); }
.sync-spinner { flex-shrink: 0; color: var(--primary); animation: sync-spinner-rotate 1s linear infinite; }
.sync-progress-body { flex: 1; min-width: 0; }
.sync-progress-label { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 7px; color: var(--text-2); font-size: 11px; line-height: 1.5; }
.sync-progress-label small { flex-shrink: 0; color: var(--text-3); font-size: 10px; font-variant-numeric: tabular-nums; }
.sync-progress-track { height: 5px; overflow: hidden; border-radius: 999px; background: var(--border); }
.sync-progress-track > span { display: block; height: 100%; border-radius: inherit; background: var(--primary); transition: width .25s ease; }
.sync-progress-track.indeterminate > span { width: 35%; animation: sync-progress-flow 1.4s ease-in-out infinite; }
.sync-source-heading h3 { flex: 1; min-width: 0; margin: 0; }
.sync-disclosure { display: flex; align-items: center; gap: 9px; width: 100%; min-height: 36px; padding: 5px 6px; border: 0; border-radius: 6px; background: transparent; color: var(--text); font: inherit; font-weight: 600; text-align: left; cursor: pointer; }
.sync-disclosure:hover { background: var(--hover); }
.sync-disclosure:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.sync-disclosure > span:first-of-type { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.sync-chevron { flex-shrink: 0; transition: transform .15s ease; }
.sync-chevron.expanded { transform: rotate(90deg); }
.sync-change-count { flex-shrink: 0; color: var(--danger); font-size: 13px; font-variant-numeric: tabular-nums; }
.sync-source-progress { margin: 8px 6px 2px; }
.sync-source-progress .sync-progress-label > span { display: inline-flex; align-items: center; gap: 6px; }
.sync-source-success { flex-shrink: 0; color: var(--success); }
.sync-source-failed { flex-shrink: 0; color: var(--danger); }
.sync-progress-track.success > span { background: var(--success); }
.sync-progress-track.failed > span { background: var(--danger); }
@keyframes sync-spinner-rotate { to { transform: rotate(360deg); } }
@keyframes sync-progress-flow { from { transform: translateX(-100%); } to { transform: translateX(290%); } }
@media (prefers-reduced-motion: reduce) { .sync-spinner, .sync-progress-track.indeterminate > span { animation: none; } .sync-progress-track > span, .sync-chevron { transition: none; } }
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
