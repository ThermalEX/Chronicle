<script setup lang="ts">
import { t } from "../services/i18n";
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { BookOpen, Gamepad2, X } from "@lucide/vue";
import SteamScanSettings from "./SteamScanSettings.vue";
import GalgameScanSettings from "./GalgameScanSettings.vue";
import SteamIcon from "./SteamIcon.vue";
import { createBackdropDismissal } from "../services/dialogDismissal";
const emit = defineEmits<{ close: []; saved: [] }>();
const steamBusy = ref(false);
const galgameBusy = ref(false);
const busy = computed(() => steamBusy.value || galgameBusy.value);
const tab = ref<'steam' | 'galgame'>('steam');
const galgameVisited = ref(false);
function selectTab(value: 'steam' | 'galgame') { tab.value = value; if (value === 'galgame') galgameVisited.value = true; }
function tabKey(event: KeyboardEvent) {
  if (busy.value || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
  event.preventDefault();
  selectTab(event.key === 'Home' ? 'steam' : event.key === 'End' ? 'galgame' : tab.value === 'steam' ? 'galgame' : 'steam');
  document.getElementById(`scan-tab-${tab.value}`)?.focus();
}
const closeButton = ref<HTMLButtonElement>();
const previousFocus = document.activeElement as HTMLElement | null;
function close() { if (!busy.value) emit("close"); }
const backdrop = createBackdropDismissal(close, () => !busy.value);
function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") { event.stopImmediatePropagation(); event.preventDefault(); close(); }
}
onMounted(() => { closeButton.value?.focus(); window.addEventListener("keydown", keydown, true); });
onBeforeUnmount(() => { window.removeEventListener("keydown", keydown, true); previousFocus?.focus(); });
</script>
<template>
  <div class="steam-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <section class="steam-dialog" role="dialog" aria-modal="true" aria-labelledby="steam-dialog-title">
      <header><Gamepad2 /><h2 id="steam-dialog-title">{{ t('游戏存档识别') }}</h2><button ref="closeButton" :disabled="busy" :aria-label="t('关闭游戏存档识别')" @click="close"><X :size="18" /></button></header>
      <div class="scan-tabs" role="tablist" :aria-label="t('游戏类型')" @keydown="tabKey">
        <button id="scan-tab-steam" role="tab" aria-controls="scan-panel-steam" :aria-selected="tab === 'steam'" :tabindex="tab === 'steam' ? 0 : -1" :disabled="busy" @click="selectTab('steam')"><SteamIcon aria-hidden="true" />Steam</button>
        <button id="scan-tab-galgame" role="tab" aria-controls="scan-panel-galgame" :aria-selected="tab === 'galgame'" :tabindex="tab === 'galgame' ? 0 : -1" :disabled="busy" @click="selectTab('galgame')"><BookOpen :size="18" aria-hidden="true" />Galgame <span>Beta</span></button>
      </div>
      <div class="steam-dialog-content">
        <div v-show="tab === 'steam'" id="scan-panel-steam" role="tabpanel" aria-labelledby="scan-tab-steam"><SteamScanSettings @busy="steamBusy = $event" @saved="emit('saved')" /></div>
        <div v-show="tab === 'galgame'" id="scan-panel-galgame" role="tabpanel" aria-labelledby="scan-tab-galgame"><GalgameScanSettings v-if="galgameVisited" @busy="galgameBusy = $event" @saved="emit('saved')" /></div>
      </div>
    </section>
  </div>
</template>
<style scoped>
.scan-tabs { display: flex; gap: 8px; padding: 12px 24px 0; }.scan-tabs button { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-radius: 8px; color: var(--text-3); background: transparent; border: 1px solid transparent; }.scan-tabs button[aria-selected=true] { color: var(--primary-dark); background: var(--primary-soft); border-color: var(--border-2); }.scan-tabs button:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }.scan-tabs svg { width: 18px; height: 18px; }.scan-tabs span { font-size: 10px; }.scan-tabs button:disabled { opacity: .6; }
.steam-backdrop { position: fixed; inset: 0; z-index: 45; padding: 24px; display: grid; place-items: center; background: #18181b99; backdrop-filter: blur(3px); }.steam-dialog { display: flex; flex-direction: column; width: min(840px, 100%); max-height: calc(100dvh - 48px); overflow: hidden; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 14px; box-shadow: 0 24px 80px #0d24205c; }.steam-dialog header { display: flex; align-items: center; gap: 12px; padding: 20px 24px; border-bottom: 1px solid var(--border); }.steam-dialog header > svg { width: 25px; height: 25px; color: var(--primary); }.steam-dialog h2 { margin: 0; font-size: 19px; }.steam-dialog header button { margin-left: auto; display: grid; place-items: center; width: 32px; height: 32px; color: var(--text-2); background: transparent; border-radius: 50%; }.steam-dialog header button:disabled { opacity: .5; }.steam-dialog-content { min-height: 0; overflow-y: auto; padding: 24px; }@media(max-width:600px) { .steam-backdrop { padding: 12px; }.steam-dialog { max-height: calc(100dvh - 24px); }.steam-dialog-content { padding: 16px; } }
</style>
