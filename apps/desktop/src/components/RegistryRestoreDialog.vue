<script setup lang="ts">
import { t } from "../services/i18n";
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { RegistryRestoreMode } from "../domain";
import ConfirmDialog from "./ConfirmDialog.vue";

defineProps<{ paths: string[] }>();
const emit = defineEmits<{ cancel: []; confirm: [mode: RegistryRestoreMode] }>();
const mode = ref<RegistryRestoreMode>("merge");
const secondConfirmation = ref(false);
const panel = ref<HTMLElement>();
let previousFocus: HTMLElement | null = null;
function confirm() {
  if (mode.value === "overwrite") secondConfirmation.value = true;
  else emit("confirm", "merge");
}
function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    if (secondConfirmation.value) secondConfirmation.value = false;
    else emit("cancel");
  }
  if (event.key !== "Tab") return;
  const controls = panel.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)');
  if (!controls?.length) return;
  const first = controls[0], last = controls[controls.length - 1];
  if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
onMounted(() => { previousFocus = document.activeElement as HTMLElement; });
onBeforeUnmount(() => previousFocus?.focus());
</script>

<template>
  <div ref="panel" @keydown="keydown">
    <ConfirmDialog v-if="!secondConfirmation" :title="t('恢复注册表')" :message="t('请选择恢复方式。写入前会备份当前状态；同一存档中的文件来源也将恢复。')" :confirm-label="mode === 'merge' ? t('合并恢复') : t('继续覆盖恢复')" :destructive="mode === 'overwrite'" @cancel="emit('cancel')" @confirm="confirm">
      <template #body-extra>
        <div class="registry-options">
          <p>{{ t('将恢复以下子键及其下级内容：') }}</p>
          <ul><li v-for="path in paths" :key="path"><code>{{ path }}</code></li></ul>
          <fieldset><legend>{{ t('恢复方式') }}</legend>
            <label><input v-model="mode" type="radio" value="merge" /><span><b>{{ t('合并（默认）') }}</b><small>{{ t('恢复备份中的值，保留当前新增的值和子键。') }}</small></span></label>
            <label><input v-model="mode" type="radio" value="overwrite" /><span><b>{{ t('覆盖') }}</b><small>{{ t('使上述子键与备份一致，删除其中后来新增的值和子键。') }}</small></span></label>
          </fieldset>
          <p v-if="mode === 'overwrite'" class="risk" role="alert">{{ t('覆盖会删除所列子树内备份以外的数据；不会修改其他子树。请核对路径。') }}</p>
        </div>
      </template>
    </ConfirmDialog>
    <ConfirmDialog v-else :title="t('确认覆盖注册表子树')" :message="t('以下子键内新增的值和子键将被删除：\n{paths}\n\n恢复前会先创建安全快照。', { paths: paths.join('\n') })" :confirm-label="t('确认覆盖')" destructive @cancel="secondConfirmation = false" @confirm="emit('confirm', 'overwrite')" />
  </div>
</template>

<style scoped>
.registry-options { padding: 0 22px 18px; color: var(--text-2); font-size: 12px; line-height: 1.5; }
ul { max-height: 140px; overflow: auto; padding-left: 18px; }
code { overflow-wrap: anywhere; }
fieldset { display: grid; gap: 12px; border: 1px solid var(--border); border-radius: 7px; padding: 12px; }
label { display: flex; align-items: flex-start; gap: 9px; cursor: pointer; }
label span { display: grid; gap: 4px; }
small { font-size: 11px; }
.risk { color: var(--danger); background: var(--danger-soft); border-radius: 7px; padding: 10px; }
input:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 3px; }
</style>
