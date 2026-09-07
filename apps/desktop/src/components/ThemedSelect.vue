<script setup lang="ts">
import { ChevronDown } from "@lucide/vue";
import { computed, ref } from "vue";

export type ThemedSelectOption = {
  value: string | null;
  label: string;
  disabled?: boolean;
};

const props = withDefaults(defineProps<{
  modelValue: string | null;
  options: ThemedSelectOption[];
  label: string;
  disabled?: boolean;
}>(), { disabled: false });

const emit = defineEmits<{ "update:modelValue": [value: string | null] }>();
const open = ref(false);
const selectedLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? "未选择");

function choose(value: string | null): void {
  open.value = false;
  emit("update:modelValue", value);
}

function closeWhenLeaving(event: FocusEvent): void {
  const picker = event.currentTarget as HTMLElement;
  if (!picker.contains(event.relatedTarget as Node | null)) open.value = false;
}
</script>

<template>
  <div class="themed-select" @focusout="closeWhenLeaving" @keydown.escape.stop="open = false">
    <button
      type="button"
      class="trigger"
      :disabled="disabled"
      :aria-label="label"
      aria-haspopup="listbox"
      :aria-expanded="open"
      @click="open = !open"
      @keydown.down.prevent="open = true"
    >
      <span>{{ selectedLabel }}</span><ChevronDown :size="15" :class="{ open }" />
    </button>
    <div v-if="open" class="options" role="listbox" :aria-label="label">
      <button
        v-for="option in options"
        :key="option.value ?? '__none__'"
        type="button"
        role="option"
        :disabled="option.disabled"
        :aria-selected="option.value === modelValue"
        :class="{ selected: option.value === modelValue }"
        @click="choose(option.value)"
      >{{ option.label }}</button>
    </div>
  </div>
</template>

<style scoped>
.themed-select{position:relative;min-width:0}.trigger{display:flex;align-items:center;justify-content:space-between;gap:8px;width:100%;height:34px;padding:0 9px;color:#263431;background:#f8faf9;border:1px solid var(--border-2);border-radius:6px;font-size:10px;text-align:left}.trigger:hover{border-color:#8bbdb4;background:#fff}.trigger[aria-expanded="true"]{border-color:var(--primary);box-shadow:0 0 0 2px #0d8b7d18}.trigger span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.trigger svg{flex:0 0 auto;color:var(--text-3);transition:transform .14s ease}.trigger svg.open{transform:rotate(180deg)}.options{position:absolute;z-index:60;top:calc(100% + 5px);left:0;min-width:100%;width:max-content;max-width:min(330px,calc(100vw - 48px));padding:5px;background:#fff;border:1px solid var(--border-2);border-radius:8px;box-shadow:0 12px 30px #153b372b}.options button{display:block;width:100%;min-height:32px;padding:0 9px;color:var(--text-2);background:transparent;border-radius:5px;font-size:10px;text-align:left;white-space:nowrap}.options button:hover,.options button:focus-visible{color:var(--primary-dark);background:var(--hover);outline:none}.options button.selected{color:var(--primary-dark);background:var(--primary-soft);font-weight:650}.options button:disabled{color:var(--text-3);background:transparent;cursor:default}
</style>
