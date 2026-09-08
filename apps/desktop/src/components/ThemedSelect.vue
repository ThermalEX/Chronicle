<script setup lang="ts">
import { ChevronDown } from "@lucide/vue";
import { computed, ref, watch, type CSSProperties } from "vue";
import { floatingMenuStyle, positionFloatingMenu } from "../services/floatingMenu";

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
const picker = ref<HTMLElement>();
const optionsStyle = ref<CSSProperties>();
let triggerElement: HTMLButtonElement | undefined;
const selectedLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? "未选择");

function choose(value: string | null): void {
  open.value = false;
  emit("update:modelValue", value);
}

function closeWhenLeaving(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (!picker.value?.contains(next)) open.value = false;
}

function positionOptions(): void {
  if (!triggerElement) return;
  const triggerRect = triggerElement.getBoundingClientRect();
  optionsStyle.value = floatingMenuStyle(positionFloatingMenu(
    triggerRect,
    { width: Math.max(triggerRect.width, 168), height: props.options.length * 32 + 10 },
    { width: window.innerWidth, height: window.innerHeight },
  ));
}

function toggleOptions(event: MouseEvent): void {
  const button = event.currentTarget as HTMLButtonElement | null;
  if (!button) return;
  triggerElement = button;
  if (open.value) { open.value = false; return; }
  positionOptions();
  open.value = true;
}

function openOptions(event: KeyboardEvent): void {
  const button = event.currentTarget as HTMLButtonElement | null;
  if (!button) return;
  triggerElement = button;
  positionOptions();
  open.value = true;
}

watch(open, (visible, _, onCleanup) => {
  if (!visible) return;
  const reposition = () => positionOptions();
  window.addEventListener("resize", reposition);
  window.addEventListener("scroll", reposition, true);
  onCleanup(() => {
    window.removeEventListener("resize", reposition);
    window.removeEventListener("scroll", reposition, true);
  });
});
</script>

<template>
  <div ref="picker" class="themed-select" @focusout="closeWhenLeaving" @keydown.escape.stop="open = false">
    <button
      type="button"
      class="trigger"
      :disabled="disabled"
      :aria-label="label"
      aria-haspopup="listbox"
      :aria-expanded="open"
      @click="toggleOptions"
      @keydown.down.prevent="openOptions"
    >
      <span>{{ selectedLabel }}</span><ChevronDown :size="15" :class="{ open }" />
    </button>
    <Teleport to="body">
    <div v-if="open" class="options" role="listbox" :aria-label="label" :style="optionsStyle" @keydown.escape.stop="open = false">
      <button
        v-for="option in options"
        :key="option.value ?? '__none__'"
        type="button"
        role="option"
        :disabled="option.disabled"
        :aria-selected="option.value === modelValue"
        :class="{ selected: option.value === modelValue }"
        @pointerdown.prevent="choose(option.value)"
      >{{ option.label }}</button>
    </div>
    </Teleport>
  </div>
</template>

<style scoped>
.themed-select{position:relative;min-width:0}.trigger{display:flex;align-items:center;justify-content:space-between;gap:8px;width:100%;height:34px;padding:0 9px;color:var(--text);background:var(--field);border:1px solid var(--border-2);border-radius:6px;font-size:10px;text-align:left}.trigger:hover{border-color:var(--primary);background:var(--surface-raised)}.trigger[aria-expanded="true"]{border-color:var(--primary);box-shadow:0 0 0 2px color-mix(in srgb, var(--primary) 18%, transparent)}.trigger span{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.trigger svg{flex:0 0 auto;color:var(--text-3);transition:transform .14s ease}.trigger svg.open{transform:rotate(180deg)}.options{position:fixed;z-index:100;min-width:0;width:max-content;max-width:min(330px,calc(100vw - 16px));padding:5px;background:var(--surface);border:1px solid var(--border-2);border-radius:8px;box-shadow:0 12px 30px var(--shadow-color)}.options button{display:block;width:100%;min-height:32px;padding:0 9px;color:var(--text-2);background:transparent;border-radius:5px;font-size:10px;text-align:left;white-space:nowrap}.options button:hover,.options button:focus-visible{color:var(--primary-dark);background:var(--hover);outline:2px solid var(--focus-ring);outline-offset:-2px}.options button.selected{color:var(--primary-dark);background:var(--primary-soft);font-weight:650}.options button:disabled{color:var(--text-3);background:transparent;cursor:default}
</style>
