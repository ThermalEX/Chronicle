<script setup lang="ts">
import { ClipboardCopy, Download, ExternalLink, X } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { nextTick, onMounted, ref } from "vue";
import type { ReleaseUpdate } from "../services/updateService";

const props = defineProps<{ update: ReleaseUpdate }>();
const emit = defineEmits<{ close: [] }>();
const closeButton = ref<HTMLButtonElement>();
const installing = ref(false);
const message = ref("");

async function copyDownloadLink(): Promise<void> {
  try {
    await navigator.clipboard.writeText(props.update.downloadUrl);
    message.value = "下载链接已复制";
  } catch {
    message.value = "无法复制下载链接";
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
  message.value = "正在下载并校验安装包…";
  try {
    await invoke("download_and_install_update", {
      url: props.update.installer.browserDownloadUrl,
      fileName: props.update.installer.name,
      sha256SumsUrl: props.update.checksumUrl ?? null,
    });
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error);
    installing.value = false;
  }
}

onMounted(() => { void nextTick(() => closeButton.value?.focus()); });
</script>

<template>
  <div class="update-backdrop">
    <section class="update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title" @keydown.esc="emit('close')">
      <header>
        <div><p>发现新版本</p><h2 id="update-title">{{ update.title }}</h2></div>
        <button ref="closeButton" aria-label="关闭更新提示" title="关闭更新提示" @click="emit('close')"><X :size="19" /></button>
      </header>
      <main>
        <p class="update-version">当前版本将更新至 {{ update.version }}</p>
        <pre>{{ update.notes }}</pre>
        <p v-if="message" class="update-message" role="status">{{ message }}</p>
      </main>
      <footer>
        <button class="subtle" @click="copyDownloadLink"><ClipboardCopy :size="15" />复制链接</button>
        <div><button class="subtle" @click="openReleasePage"><ExternalLink :size="15" />在浏览器中打开</button><button class="install" :disabled="!update.installer || installing" @click="downloadAndInstall"><Download :size="15" />{{ installing ? '下载并校验中' : update.installer ? '下载并安装' : '暂无 Windows 安装包' }}</button></div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.update-backdrop { position: fixed; z-index: 70; inset: 0; display: grid; place-items: center; padding: 24px; background: #18181b99; backdrop-filter: blur(3px); }.update-dialog { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; width: min(640px, calc(100vw - 48px)); max-height: min(680px, calc(100vh - 48px)); overflow: hidden; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }.update-dialog header, .update-dialog footer { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 18px 22px; }.update-dialog header { border-bottom: 1px solid var(--border); }.update-dialog header p { margin: 0; color: var(--primary); font-size: 10px; font-weight: 750; letter-spacing: .08em; }.update-dialog h2 { margin: 4px 0 0; font-size: 20px; }.update-dialog header button { display: grid; flex: 0 0 auto; place-items: center; width: 38px; height: 38px; color: var(--text); background: transparent; border-radius: 7px; }.update-dialog header button:hover { background: var(--hover); }.update-dialog main { min-height: 0; overflow: auto; padding: 20px 22px; }.update-version { margin: 0 0 12px; color: var(--text-2); font-size: 12px; }.update-dialog pre { margin: 0; color: var(--text-2); font: 12px/1.7 ui-monospace, Consolas, monospace; white-space: pre-wrap; word-break: break-word; }.update-message { margin: 16px 0 0; padding: 9px 11px; color: var(--primary-dark); background: var(--primary-soft); border-radius: 7px; font-size: 11px; }.update-dialog footer { border-top: 1px solid var(--border); }.update-dialog footer > div { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }.update-dialog footer button { display: inline-flex; align-items: center; gap: 6px; min-height: 35px; padding: 0 11px; border-radius: 7px; font-size: 11px; font-weight: 650; }.subtle { color: var(--text-2); background: var(--subtle); border: 1px solid var(--border-2); }.subtle:hover { background: var(--hover); }.install { color: var(--on-primary); background: var(--primary); }.install:hover:not(:disabled) { background: var(--primary-dark); }.install:disabled { cursor: default; opacity: .58; }.update-dialog button:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }@media (max-width: 560px) { .update-dialog footer { align-items: stretch; flex-direction: column; }.update-dialog footer > div { justify-content: stretch; }.update-dialog footer button { justify-content: center; }.update-dialog footer > div button { flex: 1 1 auto; } }
</style>
