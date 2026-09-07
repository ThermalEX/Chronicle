<script setup lang="ts">
import { File, Folder, Plus, Tag, X } from "@lucide/vue";
import { ref, watch } from "vue";
import type { ArchiveRecord } from "../domain";

const props = defineProps<{
  archive: ArchiveRecord;
  savingTags?: boolean;
}>();

const emit = defineEmits<{
  addTag: [tag: string];
  removeTag: [tag: string];
}>();

const tagInput = ref("");

watch(() => props.archive.id, () => { tagInput.value = ""; });

function displayPath(path: string): string {
  return path.startsWith("\\\\?\\") ? path.slice(4) : path;
}

function addTag(): void {
  const tag = tagInput.value.trim();
  if (!tag || props.archive.tags.some((item) => item.toLocaleLowerCase() === tag.toLocaleLowerCase())) return;
  emit("addTag", tag);
  tagInput.value = "";
}
</script>

<template>
  <div class="archive-metadata">
    <section aria-labelledby="sources-heading">
      <header><b id="sources-heading">来源</b><span>{{ archive.sources.length }} 项</span></header>
      <div class="source-list">
        <div v-for="source in archive.sources" :key="source.id" class="source-row" :title="displayPath(source.path)">
          <span class="source-icon"><Folder v-if="source.kind === 'folder'" :size="15" /><File v-else :size="15" /></span>
          <span><b>{{ source.name }}</b><small>{{ source.kind === 'folder' ? '文件夹' : '文件' }} · {{ displayPath(source.path) }}</small></span>
        </div>
      </div>
    </section>

    <section aria-labelledby="tags-heading">
      <header><b id="tags-heading">标签</b><span>{{ archive.tags.length }} 个</span></header>
      <div class="tag-editor">
        <span v-for="tag in archive.tags" :key="tag" class="tag-chip"><Tag :size="12" /><span>{{ tag }}</span><button type="button" :aria-label="`删除标签 ${tag}`" :disabled="savingTags" @click="emit('removeTag', tag)"><X :size="12" /></button></span>
        <span v-if="!archive.tags.length" class="tag-empty">暂无标签</span>
        <form class="tag-input" @submit.prevent="addTag">
          <Tag :size="13" />
          <input v-model="tagInput" type="text" maxlength="30" placeholder="添加标签" aria-label="新标签名称" :disabled="savingTags" />
          <button type="submit" aria-label="添加标签" :disabled="savingTags || !tagInput.trim()"><Plus :size="14" /></button>
        </form>
      </div>
    </section>
  </div>
</template>

<style scoped>
.archive-metadata { display: grid; grid-template-columns: minmax(280px, 1.35fr) minmax(240px, 1fr); gap: 18px; padding: 12px 20px 14px; background: #fbfcfc; border-bottom: 1px solid var(--border); }
section { min-width: 0; }
header { display: flex; align-items: center; gap: 7px; min-height: 20px; margin-bottom: 6px; }
header b { color: var(--text-2); font-size: 10px; }
header span { color: var(--text-3); font-size: 9px; }
.source-list { display: flex; max-height: 108px; flex-direction: column; gap: 4px; overflow-y: auto; }
.source-row { display: grid; grid-template-columns: 28px minmax(0, 1fr); align-items: center; min-height: 38px; padding: 4px 7px; background: #f2f6f4; border: 1px solid #e3e9e6; border-radius: 7px; }
.source-icon { display: grid; place-items: center; width: 24px; height: 24px; color: var(--primary); }
.source-row > span:last-child { display: flex; min-width: 0; flex-direction: column; gap: 2px; }
.source-row b, .source-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.source-row b { color: #263431; font-size: 10px; }
.source-row small { color: var(--text-3); font-size: 9px; }
.tag-editor { display: flex; align-items: center; align-content: flex-start; gap: 5px; min-height: 38px; flex-wrap: wrap; }
.tag-chip { display: inline-flex; align-items: center; gap: 4px; max-width: 160px; min-height: 28px; padding-left: 7px; color: #38534e; background: #e8f1ef; border: 1px solid #d2e4e0; border-radius: 6px; font-size: 10px; font-weight: 600; }
.tag-chip > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag-chip button { display: grid; place-items: center; width: 26px; height: 26px; flex: none; color: var(--text-3); background: transparent; border-radius: 5px; }
.tag-chip button:hover { color: #a52e28; background: #fff0ef; }
.tag-empty { color: var(--text-3); font-size: 9px; }
.tag-input { display: grid; grid-template-columns: 16px minmax(70px, 1fr) 28px; align-items: center; width: 142px; min-height: 30px; padding-left: 7px; color: var(--text-3); background: #fff; border: 1px solid var(--border-2); border-radius: 6px; }
.tag-input:focus-within { border-color: #0d9488; box-shadow: 0 0 0 2px #0d948822; }
.tag-input input { min-width: 0; height: 28px; padding: 0 3px; color: #263431; background: transparent; border: 0; outline: 0; font-size: 10px; }
.tag-input button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; color: var(--primary); background: transparent; border-radius: 5px; }
.tag-input button:hover:not(:disabled) { background: var(--primary-soft); }
.tag-input button:disabled { color: #a8b2ae; cursor: default; opacity: 1; }
button:disabled { cursor: wait; opacity: .5; }
@media (max-width: 1180px) { .archive-metadata { grid-template-columns: 1fr; gap: 10px; } .source-list { max-height: 80px; } }
</style>
