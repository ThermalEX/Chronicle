<script setup lang="ts">
import { FolderPlus, X } from "@lucide/vue";
import { onMounted, ref } from "vue";
import { createBackdropDismissal } from "../services/dialogDismissal";

const props = defineProps<{ parentName?: string; submitting?: boolean; error?: string }>();
const emit = defineEmits<{ close: []; submit: [name: string] }>();
const name = ref("");
const attempted = ref(false);
const input = ref<HTMLInputElement>();
const backdrop = createBackdropDismissal(() => emit("close"), () => !props.submitting);

function submit(): void {
  attempted.value = true;
  if (name.value.trim()) emit("submit", name.value.trim());
}

onMounted(() => input.value?.focus());
</script>

<template>
  <div class="dialog-backdrop" @pointerdown="backdrop.pointerDown" @pointerup="backdrop.pointerUp" @pointercancel="backdrop.pointerCancel">
    <form class="category-dialog" role="dialog" aria-modal="true" aria-labelledby="category-dialog-title" @submit.prevent="submit">
      <header>
        <span class="dialog-icon"><FolderPlus :size="20" /></span>
        <div><p>资料库分类</p><h2 id="category-dialog-title">{{ parentName ? '新建子分类' : '新建分类' }}</h2></div>
        <button type="button" aria-label="关闭" @click="emit('close')"><X :size="18" /></button>
      </header>
      <main>
        <p v-if="parentName" class="parent-path">创建位置：{{ parentName }}</p>
        <label for="category-name">分类名称</label>
        <input id="category-name" ref="input" v-model="name" maxlength="60" autocomplete="off" :aria-invalid="attempted && !name.trim()" @input="attempted = false" />
        <small v-if="attempted && !name.trim()" class="field-error">请输入分类名称</small>
        <p v-if="error" class="submit-error" role="alert">{{ error }}</p>
      </main>
      <footer><button type="button" class="cancel" :disabled="submitting" @click="emit('close')">取消</button><button class="submit" type="submit" :disabled="submitting">{{ submitting ? '正在创建' : '创建分类' }}</button></footer>
    </form>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 55; inset: 0; display: grid; place-items: center; padding: 24px; background: #1024218a; backdrop-filter: blur(3px); }
.category-dialog { width: min(440px, calc(100vw - 48px)); overflow: hidden; background: var(--surface); border: 1px solid var(--border-2); border-radius: 13px; box-shadow: 0 24px 80px #0d24205c; }
header { display: grid; grid-template-columns: 40px 1fr 36px; align-items: center; gap: 11px; min-height: 72px; padding: 0 18px; border-bottom: 1px solid var(--border); }
.dialog-icon { display: grid; place-items: center; width: 38px; height: 38px; color: var(--primary); background: var(--primary-soft); border-radius: 9px; }
header p, header h2 { margin: 0; } header p { color: var(--text-3); font-size: 9px; font-weight: 700; letter-spacing: .08em; } header h2 { margin-top: 3px; font-size: 18px; }
header button { display: grid; place-items: center; width: 36px; height: 36px; background: transparent; border-radius: 7px; } header button:hover, .cancel:hover { background: var(--hover); }
main { padding: 22px 24px 26px; } .parent-path { margin: 0 0 16px; padding: 9px 11px; overflow: hidden; color: var(--primary-dark); background: var(--primary-soft); border-radius: 7px; font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
label { display: block; margin-bottom: 7px; color: var(--text-2); font-size: 10px; font-weight: 700; } input { width: 100%; height: 38px; padding: 0 11px; color: #263431; background: #f8faf9; border: 1px solid var(--border-2); border-radius: 7px; font-size: 12px; outline: none; } input:focus { border-color: var(--primary); box-shadow: 0 0 0 3px #0f766e1f; } input[aria-invalid="true"] { border-color: #b83a32; }
.field-error { display: block; margin-top: 6px; color: #a52e28; font-size: 9px; }.submit-error { margin: 13px 0 0; padding: 9px 11px; color: #a52e28; background: #fff0ef; border-radius: 7px; font-size: 9px; }
footer { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-height: 62px; padding: 0 18px; border-top: 1px solid var(--border); } footer button { min-height: 36px; padding: 0 14px; border-radius: 7px; font-size: 10px; font-weight: 650; }.cancel { background: transparent; }.submit { color: #fff; background: var(--primary); }.submit:hover { background: var(--primary-dark); }button:disabled { opacity: .55; cursor: wait; }
input { color: var(--text); background: var(--field); }
input:focus { box-shadow: 0 0 0 3px color-mix(in srgb, var(--primary) 16%, transparent); }
.submit { color: var(--on-primary); }
</style>
