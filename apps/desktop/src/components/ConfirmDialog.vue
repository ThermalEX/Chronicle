<script setup lang="ts">
import { AlertTriangle, X } from "@lucide/vue";
import { onMounted, ref } from "vue";
import { createBackdropDismissal } from "../services/dialogDismissal";

const props = defineProps<{
  title: string;
  message: string;
  confirmLabel?: string;
  destructive?: boolean;
  busy?: boolean;
}>();

const emit = defineEmits<{ cancel: []; confirm: [] }>();
const cancelButton = ref<HTMLButtonElement>();
const backdrop = createBackdropDismissal(() => emit("cancel"), () => !props.busy);
onMounted(() => cancelButton.value?.focus());
</script>

<template>
  <div class="confirm-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <section class="confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message">
      <header><span :class="{ destructive }"><AlertTriangle :size="20" /></span><div><p>需要确认</p><h2 id="confirm-title">{{ title }}</h2></div><button aria-label="关闭确认窗口" :disabled="busy" @click="emit('cancel')"><X :size="18" /></button></header>
      <p id="confirm-message" class="message">{{ message }}</p>
      <footer><button ref="cancelButton" class="cancel" :disabled="busy" @click="emit('cancel')">取消</button><button class="confirm" :class="{ destructive }" :disabled="busy" @click="emit('confirm')">{{ busy ? '处理中' : (confirmLabel || '确认') }}</button></footer>
    </section>
  </div>
</template>

<style scoped>
.confirm-backdrop { position: fixed; z-index: 80; inset: 0; display: grid; place-items: center; padding: 24px; background: #1024218a; backdrop-filter: blur(3px); }
.confirm-dialog { width: min(440px, calc(100vw - 48px)); overflow: hidden; background: #fff; border: 1px solid var(--border-2); border-radius: 12px; box-shadow: 0 24px 80px #0d24205c; }
header { display: grid; grid-template-columns: 40px 1fr 38px; align-items: center; gap: 11px; padding: 17px 18px; border-bottom: 1px solid var(--border); }
header > span { display: grid; place-items: center; width: 38px; height: 38px; color: #9a6700; background: #fff4d6; border-radius: 9px; }header > span.destructive { color: #b42318; background: #feeceb; }
header p, header h2 { margin: 0; }header p { color: var(--text-3); font-size: 9px; font-weight: 700; }header h2 { margin-top: 3px; font-size: 17px; }
header button { display: grid; place-items: center; width: 38px; height: 38px; background: transparent; border-radius: 7px; }header button:hover { background: var(--hover); }
.message { margin: 0; padding: 20px 22px; color: var(--text-2); font-size: 12px; line-height: 1.65; white-space: pre-line; }
footer { display: flex; justify-content: flex-end; gap: 8px; padding: 13px 18px; background: #f8faf9; border-top: 1px solid var(--border); }footer button { min-height: 36px; padding: 0 14px; border-radius: 7px; font-size: 11px; font-weight: 650; }.cancel { background: transparent; }.cancel:hover { background: var(--hover); }.confirm { color: #fff; background: var(--primary); }.confirm.destructive { background: #b42318; }.confirm.destructive:hover { background: #912018; }
.confirm-dialog, footer { background: var(--surface); }
.confirm { color: var(--on-primary); }
</style>
