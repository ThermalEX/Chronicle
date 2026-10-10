<script setup lang="ts">
import { t } from "../services/i18n";
import { ClipboardCopy, Download, ExternalLink, LoaderCircle, X } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import type { ReleaseUpdate } from "../services/updateService";

const props = defineProps<{ update: ReleaseUpdate }>();
const emit = defineEmits<{ close: [] }>();
const closeButton = ref<HTMLButtonElement>();
const installing = ref(false);
const message = ref("");
type UpdateProgress = { requestId: string; phase: "download" | "checksum" | "verify" | "launch"; bytes: number; totalBytes: number | null };
const progress = ref<Omit<UpdateProgress, "requestId">>({ phase: "download", bytes: 0, totalBytes: null });
const percentage = computed(() => progress.value.totalBytes ? Math.min(100, Math.floor(progress.value.bytes / progress.value.totalBytes * 100)) : undefined);
const progressLabel = computed(() => ({ download: t("正在下载安装包…"), checksum: t("正在读取 SHA-256 校验文件…"), verify: t("正在校验 SHA-256…"), launch: props.update.checksumUrl ? t("校验通过，正在启动安装程序…") : t("正在启动安装程序（未提供校验文件）…") })[progress.value.phase]);
let requestId = "";
let stopProgress: UnlistenFn | undefined;
let disposed = false;
function formatBytes(bytes: number): string { return bytes < 1024 ** 2 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 ** 2).toFixed(1)} MB`; }

async function copyDownloadLink(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.update.downloadUrl);
    message.value = t("下载链接已复制");
  } catch {
    message.value = t("无法复制下载链接");
  }
}

async function openReleasePage(): Promise<void> {
  try {
    await openUrl(props.update.releaseUrl);
  } catch {
    window.open(props.update.releaseUrl, "_blank", "noopener,noreferrer");
  }
}

async function downloadAndInstall(): Promise<void> {
  if (!props.update.installer || installing.value) return;
  installing.value = true;
  message.value = t("正在下载并校验安装包…");
  requestId = crypto.randomUUID();
  const currentRequest = requestId;
  progress.value = { phase: "download", bytes: 0, totalBytes: null };
  try {
    const unlisten = await listen<UpdateProgress>("update-progress", ({ payload }) => {
      if (!disposed && payload.requestId === requestId) progress.value = payload;
    });
    if (disposed) { unlisten(); return; }
    stopProgress = unlisten;
    await invoke("download_and_install_update", {
      requestId: currentRequest,
      url: props.update.installer.browserDownloadUrl,
      fileName: props.update.installer.name,
      sha256SumsUrl: props.update.checksumUrl ?? null,
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    message.value = detail === "update-install-cancelled" ? t("已取消安装") : detail;
    installing.value = false;
  } finally {
    stopProgress?.();
    stopProgress = undefined;
    requestId = "";
  }
}

onMounted(() => { void nextTick(() => closeButton.value?.focus()); });
onBeforeUnmount(() => { disposed = true; requestId = ""; stopProgress?.(); stopProgress = undefined; });
</script>

<template>
  <div class="update-backdrop">
    <section class="update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title" @keydown.esc="emit('close')">
      <header>
        <div><p>{{ t('发现新版本') }}</p><h2 id="update-title">{{ update.title }}</h2></div>
        <button ref="closeButton" :aria-label="t('关闭更新提示')" :title="t('关闭更新提示')" @click="emit('close')"><X :size="19" /></button>
      </header>
      <main>
        <p class="update-version">{{ t('当前版本将更新至 {version}', { version: update.version }) }}</p>
        <pre>{{ update.notes }}</pre>
        <div v-if="installing" class="update-message update-progress" role="status">
          <div><LoaderCircle :size="16" class="update-spinner" /><span>{{ progressLabel }}</span><b v-if="percentage !== undefined">{{ percentage }}%</b></div>
          <div class="update-track" role="progressbar" :aria-label="progressLabel" aria-valuemin="0" aria-valuemax="100" :aria-valuenow="percentage" :class="{ indeterminate: percentage === undefined }"><span :style="percentage !== undefined ? { width: `${percentage}%` } : undefined" /></div>
          <small v-if="progress.phase === 'download' || progress.phase === 'verify'">{{ formatBytes(progress.bytes) }}<template v-if="progress.totalBytes"> / {{ formatBytes(progress.totalBytes) }}</template></small>
        </div>
        <p v-else-if="message" class="update-message" role="status">{{ message }}</p>
      </main>
      <footer>
        <button class="subtle" @click="copyDownloadLink"><ClipboardCopy :size="15" />{{ t('复制链接') }}</button>
        <div><button class="subtle" @click="openReleasePage"><ExternalLink :size="15" />{{ t('在浏览器中打开') }}</button><button class="install" :disabled="!update.installer || installing" @click="downloadAndInstall"><Download :size="15" />{{ installing ? t('下载并校验中') : update.installer ? t('下载并安装') : t('暂无 Windows 安装包') }}</button></div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.update-progress > div:first-child { display: flex; align-items: center; gap: 8px; }.update-progress b { margin-left: auto; font-variant-numeric: tabular-nums; }.update-spinner { flex: none; animation: update-spin 1s linear infinite; }.update-track { height: 6px; overflow: hidden; margin: 10px 0 6px; background: var(--border-2); border-radius: 4px; }.update-track > span { display: block; height: 100%; background: var(--primary); border-radius: inherit; transition: width .15s linear; }.update-track.indeterminate > span { width: 30%; animation: update-indeterminate 1.2s ease-in-out infinite; }.update-progress small { color: var(--text-2); font-variant-numeric: tabular-nums; }@keyframes update-spin { to { transform: rotate(360deg); } }@keyframes update-indeterminate { from { transform: translateX(-100%); } to { transform: translateX(340%); } }@media (prefers-reduced-motion: reduce) { .update-spinner, .update-track.indeterminate > span { animation: none; }.update-track > span { transition: none; } }
.update-backdrop { position: fixed; z-index: 70; inset: 0; display: grid; place-items: center; padding: 24px; background: #18181b99; backdrop-filter: blur(3px); }.update-dialog { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; width: min(640px, calc(100vw - 48px)); max-height: min(680px, calc(100vh - 48px)); overflow: hidden; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }.update-dialog header, .update-dialog footer { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 18px 22px; }.update-dialog header { border-bottom: 1px solid var(--border); }.update-dialog header p { margin: 0; color: var(--primary); font-size: 10px; font-weight: 750; letter-spacing: .08em; }.update-dialog h2 { margin: 4px 0 0; font-size: 20px; }.update-dialog header button { display: grid; flex: 0 0 auto; place-items: center; width: 38px; height: 38px; color: var(--text); background: transparent; border-radius: 7px; }.update-dialog header button:hover { background: var(--hover); }.update-dialog main { min-height: 0; overflow: auto; padding: 20px 22px; }.update-version { margin: 0 0 12px; color: var(--text-2); font-size: 12px; }.update-dialog pre { margin: 0; color: var(--text-2); font: 12px/1.7 ui-monospace, Consolas, monospace; white-space: pre-wrap; word-break: break-word; }.update-message { margin: 16px 0 0; padding: 9px 11px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 7px; font-size: 11px; }.update-dialog footer { border-top: 1px solid var(--border); }.update-dialog footer > div { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }.update-dialog footer button { display: inline-flex; align-items: center; gap: 6px; min-height: 35px; padding: 0 11px; border-radius: 7px; font-size: 11px; font-weight: 650; }.subtle { color: var(--text-2); background: var(--subtle); border: 1px solid var(--border-2); }.subtle:hover { background: var(--hover); }.install { color: var(--on-primary); background: var(--primary); }.install:hover:not(:disabled) { background: var(--primary-dark); }.install:disabled { cursor: default; opacity: .58; }.update-dialog button:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }@media (max-width: 560px) { .update-dialog footer { align-items: stretch; flex-direction: column; }.update-dialog footer > div { justify-content: stretch; }.update-dialog footer button { justify-content: center; }.update-dialog footer > div button { flex: 1 1 auto; } }
</style>
