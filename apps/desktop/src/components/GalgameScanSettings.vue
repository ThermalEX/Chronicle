<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Search, Square } from "@lucide/vue";
import { locale, t } from "../services/i18n";
import { appSettings } from "../services/settings";
import { archiveRepository } from "../services/repository";
import { alreadyManaged } from "../services/scanSources";
import { galgameError, galgameKey, importGalgameSelection, type GalgameProgress, type GalgameScanResult, type GalgameSource } from "../services/galgameScan";
import type { ArchiveRecord } from "../domain";

const emit = defineEmits<{ saved: []; busy: [value: boolean] }>();
const desktop = isTauri();
const rootPath = ref(localStorage.getItem("chronicle.galgame-root.v1") || "");
const result = ref<GalgameScanResult>();
const archives = ref<ArchiveRecord[]>([]);
const selected = ref<string[]>([]);
const search = ref("");
const error = ref("");
const message = ref("");
const loading = ref(false);
const scanning = ref(false);
const importing = ref(false);
const cancelling = ref(false);
const taskId = ref("");
const progress = ref<GalgameProgress>({ taskId: "", checkedDirectories: 0, foundGames: 0 });
const initialSnapshot = ref(appSettings.createInitialSnapshot);
const busy = computed(() => loading.value || scanning.value || importing.value);
const visible = computed(() => result.value?.games.filter(game => `${game.name} ${game.installPath}`.toLowerCase().includes(search.value.toLowerCase())) || []);
const managed = (source: GalgameSource) => alreadyManaged(source, archives.value);
const available = computed(() => visible.value.flatMap(game => game.sources.filter(source => !managed(source)).map(source => galgameKey(game, source))));
const count = computed(() => result.value?.games.filter(game => game.sources.some(source => selected.value.includes(galgameKey(game, source)) && !managed(source))).length || 0);
let unlisten: UnlistenFn | undefined;
let disposed = false;

onMounted(async () => {
  if (!desktop) return;
  loading.value = true; emit("busy", true);
  try {
    const stop = await listen<GalgameProgress>("galgame-scan-progress", event => {
      if (event.payload.taskId !== taskId.value) return;
      progress.value = event.payload;
      if (cancelling.value) void invoke("cancel_galgame_scan", { taskId: taskId.value }).catch(e => { error.value = galgameError(e); });
    });
    if (disposed) { stop(); return; }
    unlisten = stop;
    archives.value = await archiveRepository.listArchives();
    result.value = await invoke<GalgameScanResult | null>("load_galgame_scan_results") || undefined;
    if (!rootPath.value) rootPath.value = result.value?.rootPath || "";
  } catch (e) { error.value = galgameError(e); }
  finally { loading.value = false; emit("busy", false); }
});
onBeforeUnmount(() => { disposed = true; unlisten?.(); if (scanning.value) void invoke("cancel_galgame_scan", { taskId: taskId.value }).catch(() => {}); });
async function chooseRoot() {
  const path = await open({ directory: true, multiple: false, title: t("选择 Galgame 合集文件夹") });
  if (typeof path === "string") { rootPath.value = path; localStorage.setItem("chronicle.galgame-root.v1", path); }
}
async function scan() {
  scanning.value = true; cancelling.value = false; emit("busy", true); error.value = ""; message.value = "";
  taskId.value = crypto.randomUUID(); progress.value = { taskId: taskId.value, checkedDirectories: 0, foundGames: 0 };
  try {
    archives.value = await archiveRepository.listArchives();
    if (cancelling.value) throw "galgame-scan-cancelled";
    result.value = await invoke<GalgameScanResult>("scan_galgame_saves", { rootPath: rootPath.value, taskId: taskId.value });
    selected.value = [];
  } catch (e) { if (String(e) === "galgame-scan-cancelled") message.value = t("已取消扫描，保留上次结果。"); else error.value = galgameError(e); }
  finally { scanning.value = false; cancelling.value = false; emit("busy", false); }
}
async function cancel() {
  cancelling.value = true;
  try { await invoke("cancel_galgame_scan", { taskId: taskId.value }); }
  catch (e) { error.value = galgameError(e); cancelling.value = false; }
}
function selectAll() { selected.value = available.value.every(key => selected.value.includes(key)) ? selected.value.filter(key => !available.value.includes(key)) : [...new Set([...selected.value, ...available.value])]; }
async function importSelected() {
  importing.value = true; emit("busy", true); error.value = ""; message.value = "";
  try {
    const imported = await importGalgameSelection(result.value?.games || [], [...selected.value], initialSnapshot.value, archiveRepository, sources => invoke<string[]>("validate_galgame_sources", { sources }));
    archives.value = imported.archives;
    message.value = t("已添加 {count} 个存档。可在存档编辑页开启自动备份或云端同步。", { count: imported.added });
    error.value = imported.failures.join("\n");
    if (imported.added) emit("saved");
  } catch (e) { error.value = galgameError(e); }
  finally { importing.value = false; emit("busy", false); }
}
const evidenceLabel = (evidence: string) => t(evidence === "engine" ? "引擎规则" : evidence === "directory" ? "通用目录" : "名称匹配，请核对");
</script>

<template>
  <section class="galgame-settings" aria-labelledby="galgame-title">
    <h3 id="galgame-title">{{ t('Galgame 存档') }} <span class="beta">Beta</span></h3>
    <p class="intro">{{ t('选择合集文件夹，递归查找其中的游戏及已存在的存档；勾选确认后添加到资料库。') }}</p>
    <div class="scan-controls">
      <label for="galgame-root">{{ t('合集文件夹') }}</label>
      <div class="path-row"><input id="galgame-root" :value="rootPath" readonly :placeholder="t('请选择包含游戏的文件夹')" /><button :disabled="busy || !desktop" :aria-label="t('选择 Galgame 合集文件夹')" @click="chooseRoot"><FolderOpen :size="17" /></button></div>
      <div class="actions"><button class="primary" :disabled="busy || !desktop || !rootPath" @click="scan"><Search :size="16" />{{ scanning ? t('正在扫描…') : result ? t('刷新扫描') : t('开始扫描') }}</button><button v-if="scanning" :disabled="cancelling" @click="cancel"><Square :size="14" />{{ cancelling ? t('正在取消…') : t('取消扫描') }}</button></div>
      <small>{{ desktop ? t('离线识别：检查游戏目录及 AppData、文档、Saved Games 中的候选位置。扫描不会修改游戏文件。') : t('此功能仅在 Windows 桌面版可用。') }}</small>
    </div>
    <p v-if="loading" role="status">{{ t('正在读取上次扫描结果…') }}</p>
    <p v-if="scanning" role="status">{{ t('已检查 {directories} 个目录，识别 {games} 个游戏。', { directories: progress.checkedDirectories, games: progress.foundGames }) }}</p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="message" class="success" role="status">{{ message }}</p>
    <template v-if="result">
      <p class="summary">{{ t('上次扫描：{time}。结果已保存在本机；安装新游戏或存档位置变化后，可手动刷新。', { time: new Date(result.scannedAt * 1000).toLocaleString(locale) }) }}</p>
      <p class="result-root">{{ t('结果所属目录：{path}', { path: result.rootPath }) }}</p>
      <p class="summary">{{ t('识别 {games} 个游戏，{saves} 个找到存档位置。', { games: result.games.length, saves: result.games.filter(game => game.sources.length).length }) }}</p>
      <details v-if="result.warnings.length"><summary>{{ t('扫描提示（{count}）', { count: result.warnings.length }) }}</summary><p v-for="(warning, index) in result.warnings" :key="index">{{ warning }}</p></details>
      <div class="selection-bar"><input v-model="search" type="search" :aria-label="t('搜索已扫描的游戏')" :placeholder="t('搜索游戏名称或安装目录')" /><button :disabled="busy || !available.length" @click="selectAll">{{ available.length && available.every(key => selected.includes(key)) ? t('取消全选') : t('全选可添加项') }}</button></div>
      <div class="game-list">
        <article v-for="game in visible" :key="game.installPath">
          <h4>{{ game.name }} <span class="engine">{{ game.engine === 'Generic' ? t('通用识别') : game.engine }}</span></h4>
          <p class="install-path">{{ game.installPath }}</p>
          <p v-if="!game.sources.length" class="empty">{{ t('已识别游戏，未找到实际存档。请先运行游戏并保存，或手动添加存档位置。') }}</p>
          <label v-for="source in game.sources" :key="galgameKey(game, source)" class="source"><input v-model="selected" type="checkbox" :value="galgameKey(game, source)" :disabled="busy || managed(source)" /><span><span class="kind">{{ source.kind === 'file' ? t('文件') : t('文件夹') }} · {{ evidenceLabel(source.evidence) }}{{ managed(source) ? t(' · 已添加') : '' }}</span><code>{{ source.path }}</code></span></label>
        </article>
        <p v-if="!visible.length" class="empty">{{ result.games.length ? t('没有匹配的游戏。') : t('未识别到支持的游戏。可选择更接近游戏的文件夹重试，或手动添加。') }}</p>
      </div>
      <div class="import-actions"><label><input v-model="initialSnapshot" type="checkbox" :disabled="busy" />{{ t('添加后立即备份') }}</label><button class="primary" :disabled="busy || !count" @click="importSelected">{{ importing ? t('正在添加并备份…') : t('添加选中的 {count} 个存档', { count }) }}</button></div>
      <small>{{ t('添加为仅本地存档；自动备份和自动上传默认关闭。请核对路径，可只勾选需要的文件。') }}</small>
    </template>
    <p class="intro">{{ t('Beta 支持常见视觉小说引擎与存档目录；自定义引擎和特殊存档位置可能需要手动添加。') }}</p>
  </section>
</template>

<style scoped>
.galgame-settings { color: var(--text); font-size: 12px; }
h3 { margin: 0; font-size: 18px; display: flex; align-items: center; gap: 8px; }
.beta,.engine { padding: 3px 7px; border-radius: 5px; color: var(--primary-dark); background: var(--primary-soft); font-size: 11px; font-weight: normal; }
.intro,.summary,small,.install-path,.result-root { color: var(--text-3); line-height: 1.6; overflow-wrap: anywhere; }
.intro { margin: 8px 0 20px; }.install-path { margin: 0 0 8px; }.result-root { margin: 6px 0; }
.scan-controls { display: grid; gap: 10px; padding: 16px; border: 1px solid var(--border); border-radius: 10px; }
.path-row,.actions,.selection-bar,.import-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.path-row input,.selection-bar input { flex: 1; min-width: 120px; }
input:not([type=checkbox]) { padding: 9px 11px; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 7px; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; padding: 9px 12px; color: var(--primary-dark); background: var(--primary-soft); border: 1px solid var(--border); border-radius: 7px; font-size: 12px; }
button.primary { color: white; background: var(--primary); }button:disabled { opacity: .55; cursor: default; }
input[type=checkbox] { accent-color: var(--primary); flex-shrink: 0; }button:focus-visible,input:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.selection-bar { margin: 16px 0 10px; }.game-list { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
article { padding: 14px; }article + article { border-top: 1px solid var(--border); }h4 { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: 0 0 7px; font-size: 14px; overflow-wrap: anywhere; }
.source { display: flex; align-items: start; gap: 8px; padding: 8px; border-radius: 6px; }.source:hover { background: var(--subtle); }.source > span { min-width: 0; }
code { display: block; overflow-wrap: anywhere; color: var(--text-2); font-size: 11px; }.kind { display: block; margin-bottom: 3px; color: var(--text-3); font-size: 11px; }
.empty { color: var(--text-3); padding: 6px 10px; line-height: 1.6; }.import-actions { justify-content: space-between; margin: 16px 0 8px; }.import-actions label { display: flex; align-items: center; gap: 6px; }
.error,.success { padding: 12px; border-radius: 7px; white-space: pre-wrap; overflow-wrap: anywhere; }.error { color: var(--danger); background: var(--danger-soft); }.success { color: var(--primary-dark); background: var(--primary-soft); }details { padding: 10px; background: var(--subtle); overflow-wrap: anywhere; }
</style>
