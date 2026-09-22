<script setup lang="ts">
import { computed } from "vue";
import { t } from "../services/i18n";
import { Check, Folder, Moon, Plus, Sun } from "@lucide/vue";
import type { Appearance, ColorTheme } from "../services/appearance";
const props = defineProps<{ appearance: Appearance; saving?: boolean }>();
const emit = defineEmits<{ change: [appearance: Appearance] }>();
const themes = computed<{ value: ColorTheme; label: string; color: string }[]>(() => [
  { value: "teal", label: t("青绿"), color: "#397f78" },
  { value: "indigo", label: t("靛蓝"), color: "#6270a8" },
  { value: "violet", label: t("紫罗兰"), color: "#866da4" },
  { value: "amber", label: t("琥珀"), color: "#966634" },
  { value: "rose", label: t("玫红"), color: "#ad5d6e" },
  { value: "gray", label: t("灰色"), color: "#64748b" },
]);
</script>

<template>
  <div class="theme-picker">
    <div class="mode-options" role="group" :aria-label="t('外观模式')">
      <button :disabled="saving" :aria-pressed="appearance.colorMode === 'light'" @click="emit('change', { ...props.appearance, colorMode: 'light' })"><Sun :size="16" />{{ t('浅色') }}</button>
      <button :disabled="saving" :aria-pressed="appearance.colorMode === 'dark'" @click="emit('change', { ...props.appearance, colorMode: 'dark' })"><Moon :size="16" />{{ t('深色') }}</button>
    </div>
    <div class="theme-preview" role="img" :aria-label="t('{theme}主题，{mode}界面预览', { theme: themes.find(theme => theme.value === appearance.colorTheme)?.label || '', mode: appearance.colorMode === 'dark' ? t('深色') : t('浅色') })">
      <div class="preview-titlebar"><span class="preview-dots"><i /><i /><i /></span><b>Chronicle</b><span>{{ t('界面预览') }}</span></div>
      <div class="preview-workspace">
        <div class="preview-sidebar"><span class="preview-add"><Plus :size="14" />{{ t('添加存档') }}</span><span><Folder :size="15" />{{ t('全部存档') }}</span><span class="preview-selected"><Folder :size="15" />{{ t('工作配置') }}</span></div>
        <div class="preview-content"><div class="preview-heading"><b>{{ t('工作配置') }}</b><span>{{ t('创建快照') }}</span></div><p>{{ t('保存每一个值得留下的状态') }}</p><div class="preview-snapshot"><span class="preview-check"><Check :size="16" /></span><span><b>{{ t('当前版本') }}</b><small>{{ t('刚刚 · 已保存在本机') }}</small></span><span class="preview-badge">{{ t('已备份') }}</span></div><div class="preview-lines"><i /><i /></div></div>
      </div>
    </div>
    <div class="theme-options" role="group" :aria-label="t('配色主题')">
      <button v-for="theme in themes" :key="theme.value" :aria-label="t('选择{theme}主题', { theme: theme.label })" :aria-pressed="appearance.colorTheme === theme.value" :disabled="saving" @click="emit('change', { ...props.appearance, colorTheme: theme.value })"><span class="theme-swatch" :style="{ background: theme.color }"><Check v-if="appearance.colorTheme === theme.value" :size="17" /></span><span>{{ theme.label }}</span></button>
    </div>
    <p class="preview-hint">{{ t('实时预览并保存，之后可在设置中随时更换。') }}</p>
  </div>
</template>

<style scoped>
.mode-options { display: inline-flex; padding: 4px; gap: 4px; background: var(--subtle); border: 1px solid var(--border); border-radius: 10px; margin-bottom: 18px; }
.mode-options button { display: inline-flex; align-items: center; gap: 7px; padding: 8px 18px; background: transparent; color: var(--text-2); border-radius: 7px; font-size: 12px; }
.mode-options button[aria-pressed="true"] { color: var(--primary-dark); background: var(--surface); box-shadow: 0 2px 6px var(--shadow-color); }
.theme-preview { overflow: hidden; border: 1px solid var(--border-2); border-radius: 12px; text-align: left; box-shadow: 0 12px 28px var(--shadow-color); background: var(--surface); }
.preview-titlebar { display: flex; align-items: center; gap: 14px; height: 34px; padding: 0 14px; background: var(--titlebar); color: var(--titlebar-text); font-size: 11px; }.preview-titlebar > span:last-child { margin-left: auto; font-size: 10px; }.preview-dots { display: flex; gap: 4px; }.preview-dots i { width: 5px; height: 5px; background: currentColor; border-radius: 50%; opacity: .65; }
.preview-workspace { display: grid; grid-template-columns: 130px minmax(0, 1fr); min-height: 260px; }.preview-sidebar { display: flex; flex-direction: column; gap: 10px; padding: 14px 10px; background: var(--sidebar); border-right: 1px solid var(--border); }.preview-sidebar > span { display: flex; align-items: center; gap: 6px; padding: 8px; font-size: 11px; color: var(--text-2); border-radius: 6px; }.preview-sidebar .preview-add { background: var(--primary); color: var(--on-primary); justify-content: center; }.preview-sidebar .preview-selected { background: var(--primary-soft); color: var(--primary-dark); }
.preview-content { min-width: 0; padding: 19px 16px; }.preview-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 14px; }.preview-heading > span { padding: 6px 8px; background: var(--primary); color: var(--on-primary); border-radius: 5px; font-size: 10px; }.preview-content p { font-size: 11px; color: var(--text-2); margin: 10px 0 16px; }.preview-snapshot { display: flex; gap: 9px; align-items: center; padding: 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--subtle); }.preview-check { color: var(--primary); }.preview-snapshot b { display: block; font-size: 11px; }.preview-snapshot small { display: block; margin-top: 5px; font-size: 10px; color: var(--text-2); }.preview-badge { margin-left: auto; padding: 4px 6px; border-radius: 12px; font-size: 10px; background: var(--primary-soft); color: var(--primary-dark); }.preview-lines { display: grid; gap: 7px; margin-top: 14px; }.preview-lines i { display: block; width: 75%; height: 4px; background: var(--border); border-radius: 3px; }.preview-lines i:last-child { width: 45%; }
.theme-options { display: flex; justify-content: center; flex-wrap: wrap; gap: 8px; margin: 22px 0 12px; }.theme-options button { display: grid; justify-items: center; gap: 7px; min-width: 60px; padding: 9px 8px; color: var(--text-2); background: transparent; border: 1px solid transparent; border-radius: 9px; font-size: 11px; }.theme-options button[aria-pressed="true"] { border-color: var(--primary); background: var(--primary-soft); color: var(--primary-dark); }.theme-swatch { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 50%; color: #fff; }.preview-hint { margin: 12px 0 20px; color: var(--text-2); font-size: 11px; }button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 3px; }button:disabled { cursor: wait; opacity: .65; }button:hover:not(:disabled) { box-shadow: 0 0 0 1px var(--border-2); }
@media(max-width: 600px) { .preview-workspace { grid-template-columns: 100px minmax(0, 1fr); }.preview-content { padding: 14px 10px; }.preview-badge { display: none; }.theme-options { gap: 3px; }.theme-options button { min-width: 46px; } }
</style>
