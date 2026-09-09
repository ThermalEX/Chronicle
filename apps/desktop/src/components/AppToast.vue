<script setup lang="ts">
import { CheckCircle2, CircleAlert, Info, X } from "@lucide/vue";
defineProps<{ message: string; type: "success" | "error" | "info" }>();
defineEmits<{ close: [] }>();
</script>

<template>
  <Teleport to="body">
    <div class="toast" :class="type" :role="type === 'error' ? 'alert' : 'status'">
      <component :is="type === 'success' ? CheckCircle2 : type === 'error' ? CircleAlert : Info" :size="16" />
      <span>{{ message }}</span>
      <button aria-label="关闭通知" title="关闭通知" @click="$emit('close')"><X :size="15" /></button>
    </div>
  </Teleport>
</template>

<style scoped>
.toast { position: fixed; z-index: 200; right: 20px; bottom: 20px; display: flex; align-items: center; gap: 10px; max-width: min(390px, calc(100vw - 40px)); min-height: 44px; padding: 9px 9px 9px 14px; color: #fff; background: #17834a; border-radius: 8px; box-shadow: 0 10px 30px #102b2730; font-size: 12px; }
.toast.error { background: #7f1d1d; }
.toast.info { background: #1e3a5f; }
.toast > svg, .toast button { flex-shrink: 0; }
.toast button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; color: inherit; background: transparent; border-radius: 5px; }
.toast button:hover { background: #ffffff1a; }
</style>
