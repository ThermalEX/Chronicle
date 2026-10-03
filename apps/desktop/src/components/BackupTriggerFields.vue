<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { Files, Gamepad2 } from "@lucide/vue";
import { t } from "../services/i18n";
import {
  backupAutomationLabel,
  backupAutomationSupported,
  listBackupProcesses,
  type BackupTriggerConfig,
} from "../services/backupAutomation";
import ThemedSelect from "./ThemedSelect.vue";
const props = defineProps<{
  modelValue: BackupTriggerConfig;
  disabled: boolean;
}>();
const emit = defineEmits<{ "update:modelValue": [BackupTriggerConfig] }>();
const error = ref("");
const partial = ref(false);
const loading = ref(false);
const processes = ref<{ name: string; executablePath: string }[]>([]);
const disabled = computed(() => props.disabled || !backupAutomationSupported());
function update(value: Partial<BackupTriggerConfig>) {
  emit("update:modelValue", { ...props.modelValue, ...value });
}
async function choose() {
  try {
    const path = await open({
      multiple: false,
      filters: [{ name: t("游戏程序"), extensions: ["exe"] }],
    });
    if (typeof path === "string") update({ executablePath: path });
  } catch (e) {
    error.value = String(e);
  }
}
async function running() {
  loading.value = true;
  error.value = "";
  try {
    const result = await listBackupProcesses();
    processes.value = result.processes;
    partial.value = result.partial;
    if (!result.complete) error.value = "backup_process_unknown";
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
</script>
<template>
  <fieldset class="backup-trigger" :disabled="disabled">
    <legend>{{ t("自动备份触发方式") }}</legend>
    <div class="trigger-modes">
      <label :class="{ selected: modelValue.mode === 'file_change' }"
        ><input
          type="radio"
          name="backup-trigger"
          :checked="modelValue.mode === 'file_change'"
          @change="update({ mode: 'file_change' })"
        /><Files :size="16" aria-hidden="true" />{{ t("文件变化后") }}</label
      ><label :class="{ selected: modelValue.mode === 'game_exit' }"
        ><input
          type="radio"
          name="backup-trigger"
          :checked="modelValue.mode === 'game_exit'"
          @change="update({ mode: 'game_exit' })"
        /><Gamepad2 :size="16" aria-hidden="true" />{{ t("游戏退出后") }}</label
      >
    </div>
    <template v-if="modelValue.mode === 'game_exit'">
      <p>{{ t("绑定实际游戏程序，不是启动器。应用需保持运行或留在托盘。") }}</p>
      <div class="trigger-actions">
        <button type="button" @click="choose">{{ t("选择游戏程序") }}</button
        ><button type="button" :disabled="disabled || loading" @click="running">
          {{ t("从运行中的程序选择") }}
        </button>
      </div>
      <p class="path">{{ modelValue.executablePath || t("未选择") }}</p>
      <ThemedSelect
        v-if="processes.length"
        :model-value="modelValue.executablePath"
        :options="
          processes.map((p) => ({
            value: p.executablePath,
            label: p.name + ' — ' + p.executablePath,
          }))
        "
        :label="t('运行中的程序')"
        :disabled="disabled"
        @update:model-value="update({ executablePath: $event })"
      />
      <small v-if="partial">{{
        t("部分进程不可读取，仅显示可选择的程序。")
      }}</small>
      <label
        >{{ t("退出后的静默时间（秒）")
        }}<input
          type="number"
          min="1"
          max="300"
          :value="modelValue.quietSeconds"
          @input="
            update({
              quietSeconds: Number(($event.target as HTMLInputElement).value),
            })
          "
      /></label>
      <small>{{
        t("内容没有变化时不会新增快照；注册表也可随游戏退出备份。")
      }}</small>
    </template>
    <small v-if="!backupAutomationSupported()">{{
      t("仅 Windows 桌面版可用")
    }}</small>
    <p v-if="error" role="alert">{{ backupAutomationLabel(error) }}</p>
  </fieldset>
</template>
<style scoped>
.backup-trigger {
  display: grid;
  gap: 12px;
  margin: 12px 0 24px;
  min-width: 0;
  border: 0;
  padding: 0;
  color: var(--text);
  font-size: 11px;
}
legend { padding: 0 0 8px; color: var(--text-2); font-size: 10px; font-weight: 700; }
.trigger-modes { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.trigger-modes label { position: relative; min-height: 44px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 8px; cursor: pointer; font-weight: 650; }
.trigger-modes label.selected { color: var(--primary-dark); background: var(--primary-soft); border-color: var(--primary); }
.trigger-modes label:has(input:focus-visible) { outline: 2px solid var(--primary); outline-offset: 2px; }
.trigger-modes input { position: absolute; opacity: 0; width: 1px; height: 1px; }
.trigger-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
label {
  display: flex;
  align-items: center;
  gap: 8px;
}
p {
  margin: 0;
  font-size: 11px;
  line-height: 1.6;
}
small {
  color: var(--text-2);
  font-size: 10px;
  line-height: 1.5;
}
.path {
  overflow-wrap: anywhere;
  padding: 10px 12px;
  background: var(--subtle);
  border-radius: 7px;
}
button,
input[type="number"] {
  border: 1px solid var(--border-2);
  border-radius: 8px;
  padding: 9px;
  color: var(--text);
  background: var(--surface);
  font: inherit;
  min-height: 34px;
}
input[type="number"] {
  width: 85px;
}
button {
  cursor: pointer;
}
button:not(:disabled):hover { background: var(--hover); }
fieldset:disabled .trigger-modes label { cursor: default; }
fieldset:disabled {
  opacity: 0.6;
}
button:focus-visible,
input:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: 2px;
}
</style>
