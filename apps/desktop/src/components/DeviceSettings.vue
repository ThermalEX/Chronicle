<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ClipboardCopy, RefreshCw } from "@lucide/vue";
import { deviceRepository, readKnownDevices, deviceLabel, type DeviceIdentity, type KnownSourceDevices } from "../services/devices";
import { t } from "../services/i18n";
import ConfirmDialog from "./ConfirmDialog.vue";

const props = defineProps<{ device?: DeviceIdentity; name: string; saving?: boolean }>();
const emit = defineEmits<{ "update:name": [name: string]; "identity-reset": [device: DeviceIdentity] }>();
const sources = ref<KnownSourceDevices[]>([]);
const error = ref("");
const busy = ref(false);
const resetting = ref(false);
async function load(): Promise<void> {
  try { sources.value = await readKnownDevices(); }
  catch (cause) { error.value = t(String(cause)); }
}
async function resetIdentity(): Promise<void> {
  busy.value = true; error.value = ""; resetting.value = false;
  try {
    emit("identity-reset", await deviceRepository.reset());
  } catch (cause) { error.value = t(cause instanceof Error ? cause.message : String(cause)); }
  finally { busy.value = false; }
}
async function copyId(): Promise<void> {
  try { if (props.device) await navigator.clipboard.writeText(props.device.id); }
  catch (cause) { error.value = t(String(cause)); }
}
onMounted(load);
</script>

<template>
  <section aria-labelledby="device-settings-title">
    <div class="section-heading"><h3 id="device-settings-title">{{ t('设备') }}</h3><p>{{ t('为本机命名，在时间线和同步记录中区分不同设备。') }}</p></div>
    <div v-if="device" class="setting-group">
      <label class="setting-row"><span><b>{{ t('本机名称') }}</b><small>{{ t('名称须为 1–64 个字符；同名设备使用短 ID 区分。') }}</small></span><input class="snapshot-field" :value="name" :disabled="busy || saving" :aria-invalid="!name.trim() || [...name.trim()].length > 64" @input="emit('update:name', ($event.target as HTMLInputElement).value)" /></label>
      <div class="setting-row"><span><b>{{ t('设备 ID') }}</b><small class="device-id">{{ device.id }}</small></span><button class="snapshot-control" type="button" @click="copyId"><ClipboardCopy :size="14" />{{ t('复制') }}</button></div>
      <div class="setting-row"><span><b>{{ t('作为新设备使用') }}</b><small>{{ t('仅更换本机 ID，保留资料库、历史快照和来源路径。') }}</small></span><button class="snapshot-control" type="button" :disabled="busy || saving" @click="resetting = true"><RefreshCw :size="14" />{{ t('作为新设备使用') }}</button></div>
    </div>
    <p v-if="error" class="recycle-error" role="alert">{{ error }}</p>
    <div class="section-heading known-devices"><h3>{{ t('已知设备') }}</h3><p>{{ t('按同步源展示缓存记录；成功同步时间不是实时在线状态。') }}</p></div>
    <div v-for="source in sources" :key="source.sourceId" class="setting-group known-devices"><div class="setting-row"><b>{{ source.sourceName }}</b></div><div v-for="known in source.devices" :key="known.id" class="setting-row"><span><b>{{ deviceLabel(known.id, known.name, device ? [device] : []) }}</b><small>{{ known.lastSuccessfulSyncAt ? new Date(known.lastSuccessfulSyncAt).toLocaleString() : t('未记录成功同步') }}</small></span></div></div>
    <p v-if="!sources.length">{{ t('成功同步后显示已知设备。') }}</p>
    <ConfirmDialog v-if="resetting" :title="t('作为新设备使用')" :message="t('确认生成新的设备 ID？历史记录仍归属于旧设备，此操作不会更改来源路径。')" @keydown.esc.stop.prevent="resetting = false" @confirm="resetIdentity" @cancel="resetting = false" />
  </section>
</template>

<style scoped>
.section-heading { margin-bottom: 20px; }
.section-heading h3 { font-size: 18px; }
.section-heading p { margin: 6px 0 0; color: var(--text-3); font-size: 11px; }
.setting-group { overflow: hidden; border: 1px solid var(--border); border-radius: 10px; }
.setting-row { display: flex; align-items: center; justify-content: space-between; min-height: 70px; gap: 28px; padding: 12px 16px; background: var(--surface); }
.setting-row + .setting-row { border-top: 1px solid var(--border); }
.setting-row > span { display: flex; min-width: 0; flex-direction: column; gap: 5px; }
.setting-row b { font-size: 12px; font-weight: 650; }
.setting-row small { color: var(--text-3); font-size: 10px; line-height: 1.4; }
.setting-row input { min-width: 0; width: 200px; }
.device-id { overflow-wrap: anywhere; }
.known-devices { margin-top: 20px; }
@media (max-width: 700px) { .setting-row { flex-wrap: wrap; gap: 12px; } }
</style>
