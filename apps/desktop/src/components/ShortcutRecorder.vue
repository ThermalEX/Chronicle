<script setup lang="ts">
import { Keyboard } from "@lucide/vue";
import { ref } from "vue";
import { shortcutFromKeyboardEvent } from "../services/settings";

const props = defineProps<{ modelValue: string; label: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const recording = ref(false);

function startRecording(): void {
  recording.value = true;
}

function stopRecording(): void {
  recording.value = false;
}

function capture(event: KeyboardEvent): void {
  if (!recording.value) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    stopRecording();
    return;
  }
  if (event.key === "Delete") {
    emit("update:modelValue", "");
    stopRecording();
    return;
  }
  const shortcut = shortcutFromKeyboardEvent(event);
  if (!shortcut) return;
  emit("update:modelValue", shortcut);
  stopRecording();
}
</script>

<template>
  <button
    type="button"
    class="shortcut-recorder"
    :class="{ recording }"
    :aria-label="`${label}：${recording ? '正在录制' : modelValue || '未设置'}`"
    :title="recording ? '按组合键保存；Esc 取消；Delete 清除' : '点击后按组合键设置热键'"
    @click="startRecording"
    @keydown="capture"
    @blur="stopRecording"
  >
    <Keyboard :size="15" aria-hidden="true" />
    <span>{{ recording ? '按下组合键…' : modelValue || '未设置' }}</span>
  </button>
</template>

<style scoped>
.shortcut-recorder { display: inline-flex; align-items: center; justify-content: center; gap: 8px; width: 194px; min-height: 38px; padding: 0 12px; color: var(--text-2); background: #f8faf9; border: 1px solid var(--border-2); border-radius: 7px; font-family: "Cascadia Code", Consolas, monospace; font-size: 11px; font-weight: 650; letter-spacing: .02em; transition: border-color .16s ease, background .16s ease, box-shadow .16s ease; }.shortcut-recorder:hover { color: var(--primary-dark); border-color: #8bbdb4; background: #fff; }.shortcut-recorder.recording { color: var(--primary-dark); background: var(--primary-soft); border-color: var(--primary); box-shadow: 0 0 0 2px #0d8b7d1f; }.shortcut-recorder:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }.shortcut-recorder span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
