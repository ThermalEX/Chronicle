<script setup lang="ts">
import { onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import type { TutorialProgress, TutorialTip } from "../services/onboarding";
const props = defineProps<{ tip: TutorialTip; progress?: TutorialProgress }>();
const emit = defineEmits<{ seen: [tip: TutorialTip] }>();
const visible = ref(false);
const copy: Record<TutorialTip, string> = {
  categories: "分类只整理 Chronicle 内的存档，不会移动原始文件。可在侧栏创建子分类，也可拖动存档调整归属。",
  exclusions: "用 *.tmp 或 cache/ 忽略不需要保存的内容，每行一条。排除项不会触发自动备份，也不写入新快照。",
  registry: "填写 HKCU/HKLM 下的具体子键，路径不包含“计算机”。注册表支持手动备份，恢复时可选合并或覆盖，不监听注册表变化。",
};
onMounted(() => {
  if (props.progress?.status === "completed" && !props.progress.seenTips.includes(props.tip)) {
    visible.value = true;
    emit("seen", props.tip);
  }
});
</script>

<template>
  <div v-if="visible" class="tutorial-hint" role="status"><span>{{ copy[tip] }}</span><button type="button" aria-label="关闭使用提示" @click="visible = false"><X :size="16" /></button></div>
</template>

<style scoped>
.tutorial-hint { display: flex; align-items: flex-start; gap: 12px; padding: 12px; margin: 10px 0; color: var(--primary-dark); background: var(--primary-soft); border: 1px solid var(--border); border-radius: 8px; font-size: 12px; line-height: 1.6; }
.tutorial-hint button { display: grid; place-items: center; flex: none; width: 28px; height: 28px; padding: 0; color: inherit; background: transparent; border-radius: 5px; }
</style>
