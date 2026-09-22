<script setup lang="ts">
import { t } from "../services/i18n";
import { onMounted, onBeforeUnmount, ref } from "vue";
import { X } from "@lucide/vue";
import SteamScanSettings from "./SteamScanSettings.vue";
import SteamIcon from "./SteamIcon.vue";
import { createBackdropDismissal } from "../services/dialogDismissal";
const emit = defineEmits<{ close: []; saved: [] }>();
const busy = ref(false);
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
      <header><SteamIcon /><h2 id="steam-dialog-title">{{ t('游戏存档识别') }}</h2><button ref="closeButton" :disabled="busy" :aria-label="t('关闭游戏存档识别')" @click="close"><X :size="18" /></button></header>
      <div class="steam-dialog-content"><SteamScanSettings @busy="busy = $event" @saved="emit('saved')" /></div>
    </section>
  </div>
</template>
<style scoped>
.steam-backdrop { position: fixed; inset: 0; z-index: 45; padding: 24px; display: grid; place-items: center; background: #18181b99; backdrop-filter: blur(3px); }.steam-dialog { display: flex; flex-direction: column; width: min(840px, 100%); max-height: calc(100dvh - 48px); overflow: hidden; color: var(--text); background: var(--surface); border: 1px solid var(--border-2); border-radius: 14px; box-shadow: 0 24px 80px #0d24205c; }.steam-dialog header { display: flex; align-items: center; gap: 12px; padding: 20px 24px; border-bottom: 1px solid var(--border); }.steam-dialog header > svg { width: 25px; height: 25px; color: var(--primary); }.steam-dialog h2 { margin: 0; font-size: 19px; }.steam-dialog header button { margin-left: auto; display: grid; place-items: center; width: 32px; height: 32px; color: var(--text-2); background: transparent; border-radius: 50%; }.steam-dialog header button:disabled { opacity: .5; }.steam-dialog-content { min-height: 0; overflow-y: auto; padding: 24px; }@media(max-width:600px) { .steam-backdrop { padding: 12px; }.steam-dialog { max-height: calc(100dvh - 24px); }.steam-dialog-content { padding: 16px; } }
</style>
