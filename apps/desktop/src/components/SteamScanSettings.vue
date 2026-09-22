<script setup lang="ts">
import { locale, t } from "../services/i18n";
import { computed, onMounted, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Search, RefreshCw } from "@lucide/vue";
import { archiveRepository } from "../services/repository";
import { appSettings } from "../services/settings";
import { alreadyManaged, sourceKey, steamSourceGroups, type SteamGame, type SteamScanResult, type SteamSource } from "../services/steamScan";
import type { ArchiveRecord } from "../domain";

const emit = defineEmits<{ saved: []; busy: [value: boolean] }>();
const desktop = isTauri();
const steamPath = ref("");
const scanning = ref(false);
const loading = ref(false);
const importing = ref(false);
const result = ref<SteamScanResult>();
const archives = ref<ArchiveRecord[]>([]);
const selected = ref<string[]>([]);
const search = ref("");
const error = ref("");
const message = ref("");
const initialSnapshot = ref(appSettings.createInitialSnapshot);
const busy = computed(() => loading.value || scanning.value || importing.value);
onMounted(async () => {
  if (!desktop) return;
  loading.value = true; emit("busy", true);
  try {
    archives.value = await archiveRepository.listArchives();
    result.value = await invoke<SteamScanResult | undefined>("load_steam_scan_results") || undefined;
    steamPath.value = result.value?.steamPath || "";
  } catch (e) { error.value = String(e); }
  finally { loading.value = false; emit("busy", false); }
});
const key = (game: SteamGame, source: SteamSource) => `${game.appId}:${sourceKey(source)}`;
const managed = (source: SteamSource) => alreadyManaged(source, archives.value);
const visibleGames = computed(() => result.value?.games.filter(game => `${game.name} ${game.appId} ${game.sources.map(s => `${s.user?.displayName || ''} ${s.user?.accountId || ''}`).join(' ')}`.toLowerCase().includes(search.value.toLowerCase())) ?? []);
const available = computed(() => visibleGames.value.flatMap(game => game.sources.filter(source => !managed(source)).map(source => key(game, source))));
const selectedArchives = computed(() => result.value?.games.reduce((count, game) => count + steamSourceGroups(game).filter(group => group.sources.some(source => selected.value.includes(key(game, source)) && !managed(source))).length, 0) ?? 0);
function selectAll() { selected.value = available.value.every(id => selected.value.includes(id)) ? selected.value.filter(id => !available.value.includes(id)) : [...new Set([...selected.value, ...available.value])]; }
async function chooseSteam() {
  const path = await open({ directory: true, multiple: false, title: t("选择 Steam 安装目录（包含 steamapps）") });
  if (typeof path === "string") steamPath.value = path;
}
async function scan(refreshDatabase = false) {
  scanning.value = true; emit("busy", true); error.value = ""; message.value = "";
  try {
    archives.value = await archiveRepository.listArchives();
    result.value = await invoke<SteamScanResult>("scan_steam_saves", { steamPath: steamPath.value || null, refreshDatabase });
    selected.value = [];
  } catch (e) { error.value = String(e); }
  finally { scanning.value = false; emit("busy", false); }
}
async function importSelected() {
  importing.value = true; emit("busy", true); error.value = ""; message.value = "";
  let added = 0;
  const failures: string[] = [];
  try {
    archives.value = await archiveRepository.listArchives();
    for (const game of result.value?.games ?? []) {
      for (const group of steamSourceGroups(game)) {
      const sources = group.sources.filter(source => selected.value.includes(key(game, source)) && !managed(source));
      if (!sources.length) continue;
      let created: ArchiveRecord | undefined;
      try {
        created = await archiveRepository.createArchive({ name: group.archiveName, sources: sources.map(source => ({ path: source.path, kind: source.kind, id: crypto.randomUUID(), name: source.path.split(/[\\/]/).at(-1) || game.name })), storagePolicy: "local", syncMode: "manual", createInitialSnapshot: false, autoBackupEnabled: false, automaticUploadEnabled: false });
        archives.value.push(created); added++;
        if (initialSnapshot.value) await archiveRepository.createSnapshot(created, t("初始版本"));
      } catch (e) { failures.push(`${group.archiveName}：${created ? t("存档已添加，初始备份失败：") : t("添加失败：")}${String(e)}`); }
      }
    }
    message.value = t("已添加 {count} 个存档。可在存档编辑页开启自动备份或云端同步。", { count: added });
    error.value = failures.join("\n");
  } catch (e) { error.value = String(e); }
  finally { if (added) emit("saved"); importing.value = false; emit("busy", false); }
}
</script>

<template>
  <section class="steam-settings" aria-labelledby="steam-title">
    <h3 id="steam-title">{{ t('Steam 存档') }}</h3>
    <p class="intro">{{ t('自动识别已安装的 Steam 游戏与本机存档位置，勾选后添加到资料库。') }}</p>
    <div class="scan-controls">
      <label for="steam-path">{{ t('Steam 安装目录') }}</label>
      <div class="path-row"><input id="steam-path" v-model="steamPath" :disabled="busy || !desktop" :placeholder="t('留空则自动识别')" /><button :disabled="busy || !desktop" :aria-label="t('选择 Steam 安装目录')" @click="chooseSteam"><FolderOpen :size="16" /></button></div>
      <div class="actions"><button class="primary" :disabled="busy || !desktop" @click="scan()"><Search :size="15" />{{ scanning ? t('正在扫描…') : result ? t('刷新扫描') : t('扫描游戏存档') }}</button><button :disabled="busy || !desktop" @click="scan(true)"><RefreshCw :size="15" />{{ t('更新路径库并扫描') }}</button></div>
      <small>{{ desktop ? t('首次扫描需联网下载路径库；以后可离线扫描，更新路径库需联网。') : t('此功能仅在 Windows 桌面版可用。') }}</small>
    </div>
    <p v-if="scanning" role="status">{{ t('正在读取游戏库并匹配存档路径，首次下载可能需要片刻…') }}</p>
    <p v-if="loading" role="status">{{ t('正在读取上次扫描结果…') }}</p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="message" class="success" role="status">{{ message }}</p>
    <template v-if="result">
      <p class="summary">{{ t('上次扫描：{time}。结果已保存在本机；安装新游戏或存档位置变化后，可手动刷新。', { time: new Date(result.scannedAt * 1000).toLocaleString(locale) }) }}</p>
      <p class="summary">{{ t('发现 {games} 个已安装游戏，{saves} 个找到存档位置。路径库更新于 {time}。', { games: result.games.length, saves: result.games.filter(g => g.sources.length).length, time: new Date(result.databaseUpdatedAt * 1000).toLocaleString(locale) }) }}</p>
      <details v-if="result.warnings.length"><summary>{{ t('扫描提示（{count}）', { count: result.warnings.length }) }}</summary><p v-for="warning in result.warnings" :key="warning">{{ warning }}</p></details>
      <div class="selection-bar"><input v-model="search" type="search" :aria-label="t('搜索已扫描的游戏')" :placeholder="t('搜索游戏名称或 Steam AppID')" /><button :disabled="busy || !available.length" @click="selectAll">{{ available.length && available.every(id => selected.includes(id)) ? t('取消全选') : t('全选可添加项') }}</button></div>
      <div class="game-list">
        <article v-for="game in visibleGames" :key="game.appId">
          <h4>{{ game.name }} <small>{{ game.appId }}</small></h4>
          <p v-if="!game.sources.length" class="empty">{{ game.hasRules ? t('未找到实际存档，可能需要先运行游戏并保存一次。') : t('路径库暂未收录，且未发现 Steam 云存档，可手动添加。') }}</p>
          <section v-for="group in steamSourceGroups(game)" :key="group.id" class="account-group">
            <div class="account-heading"><b>{{ group.label }}</b><small v-if="group.user">{{ t('用户 ID：{id}', { id: group.user.accountId }) }}</small><small v-else>{{ t('无法确定所属 Steam 账号') }}</small></div>
            <label v-for="source in group.sources" :key="key(game, source)" class="source"><input v-model="selected" type="checkbox" :value="key(game, source)" :disabled="busy || managed(source)" /><span><span class="kind">{{ source.kind === 'registry' ? t('注册表') : source.kind === 'folder' ? t('文件夹') : t('文件') }}{{ managed(source) ? t(' · 已添加') : '' }}</span><code>{{ source.path }}</code></span></label>
          </section>
        </article>
        <p v-if="!visibleGames.length" class="empty">{{ result.games.length ? t('没有匹配的游戏。') : t('此游戏库中没有找到已安装游戏，请检查 Steam 目录。') }}</p>
      </div>
      <div class="import-actions"><label><input v-model="initialSnapshot" type="checkbox" :disabled="busy" />{{ t('添加后立即备份') }}</label><button class="primary" :disabled="busy || !selectedArchives" @click="importSelected">{{ importing ? t('正在添加并备份…') : t('添加选中的 {count} 个存档', { count: selectedArchives }) }}</button></div>
      <small>{{ t('添加为仅本地存档；自动备份和自动上传默认关闭。请核对路径，可只勾选需要的文件。') }}</small>
    </template>
    <p class="attribution">{{ t('路径数据：') }}<a href="https://github.com/mtkennerly/ludusavi-manifest" target="_blank" rel="noreferrer">Ludusavi Manifest</a> · <a href="https://www.pcgamingwiki.com/" target="_blank" rel="noreferrer">PCGamingWiki</a></p>
  </section>
</template>

<style scoped>
.account-group { margin-top: 10px; padding: 8px; background: var(--subtle); border-radius: 8px; }.account-heading { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; padding: 4px 8px; overflow-wrap: anywhere; }.account-heading b { font-size: 12px; }.account-heading small { font-size: 10px; }
.steam-settings { color: var(--text); font-size: 12px; }.steam-settings h3 { margin: 0; font-size: 18px; }.intro,.summary,.attribution,small { color: var(--text-3); line-height: 1.6; }.intro { margin: 6px 0 20px; }.scan-controls { display: grid; gap: 10px; padding: 16px; border: 1px solid var(--border); border-radius: 10px; }.path-row,.actions,.selection-bar,.import-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }.path-row input,.selection-bar > input { flex: 1; min-width: 120px; }.steam-settings input:not([type=checkbox]) { padding: 9px 11px; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 7px; }.steam-settings button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; padding: 9px 12px; color: var(--primary-dark); background: var(--primary-soft); border: 1px solid var(--border); border-radius: 7px; font-size: 11px; }.steam-settings button.primary { color: white; background: var(--primary); }.steam-settings button:disabled { opacity: .55; cursor: default; }.steam-settings input[type=checkbox] { accent-color: var(--primary); flex-shrink: 0; }.selection-bar { margin: 16px 0 10px; }.game-list { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }.game-list article { padding: 13px; }.game-list article + article { border-top: 1px solid var(--border); }.game-list h4 { margin: 0 0 9px; font-size: 13px; }.game-list h4 small { margin-left: 6px; font-weight: normal; }.source { display: flex; align-items: start; gap: 8px; padding: 8px; border-radius: 6px; }.source:hover { background: var(--subtle); }.source > span { min-width: 0; }.source code { display: block; overflow-wrap: anywhere; color: var(--text-2); font-size: 11px; }.kind { display: block; margin-bottom: 3px; color: var(--text-3); font-size: 10px; }.empty { color: var(--text-3); padding: 6px 10px; }.import-actions { justify-content: space-between; margin: 16px 0 8px; }.import-actions label { display: flex; align-items: center; gap: 6px; }.error,.success { padding: 12px; border-radius: 7px; white-space: pre-wrap; overflow-wrap: anywhere; }.error { color: var(--danger); background: var(--danger-soft); }.success { color: var(--primary-dark); background: var(--primary-soft); }details { padding: 10px; background: var(--subtle); overflow-wrap: anywhere; }.attribution { margin-top: 20px; font-size: 11px; }.attribution a { color: var(--primary); }
</style>
