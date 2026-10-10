<script setup lang="ts">
import {ImagePlus,Image as ImageIcon,X} from "@lucide/vue";
import {ref,watch,onBeforeUnmount} from "vue";
import {isTauri} from "@tauri-apps/api/core";
import {previewThemeThumbnail} from "../services/personalization";
import {t} from "../services/i18n";
const props=withDefaults(defineProps<{paths:string[];selectedIndex:number;busy?:boolean}>(),{busy:false});
const emit=defineEmits<{add:[];remove:[index:number];select:[index:number]}>();
const gallery=ref<HTMLElement>();const thumbnails=ref<Record<string,string>>({});let generation=0;
function name(path:string):string{return path.split(/[\\/]/).at(-1)??path;}
watch(()=>props.paths,async(paths)=>{
  const request=++generation;if(!isTauri())return;
  for(const path of paths){if(thumbnails.value[path])continue;
    try{const url=await previewThemeThumbnail(path);if(request!==generation)return;thumbnails.value[path]=url;}catch{/* Keep the item visible so it can be replaced or removed. */}
  }
},{deep:true,immediate:true});
defineExpose({contains(position:{x:number;y:number}):boolean{const rect=gallery.value?.getBoundingClientRect();return !!rect&&position.x>=rect.left&&position.x<=rect.right&&position.y>=rect.top&&position.y<=rect.bottom;}});
onBeforeUnmount(()=>generation++);
</script>
<template>
  <div ref="gallery" class="wallpaper-gallery">
    <div v-for="(path,index) in paths" :key="path" class="wallpaper-item" :class="{selected:index===selectedIndex}">
      <button type="button" class="wallpaper-select" :disabled="busy" :aria-pressed="index===selectedIndex" :aria-label="t('选择图片 {name}',{name:name(path)})" @click="emit('select',index)">
        <img v-if="thumbnails[path]" :src="thumbnails[path]" alt="" /><ImageIcon v-else :size="24" aria-hidden="true" />
        <span :title="name(path)">{{ name(path) }}</span>
      </button>
      <button type="button" class="wallpaper-remove" :disabled="busy" :aria-label="t('移除图片 {name}',{name:name(path)})" :title="t('移除图片 {name}',{name:name(path)})" @click="emit('remove',index)"><X :size="14" /></button>
    </div>
    <button type="button" class="wallpaper-add" data-testid="add-wallpapers" :disabled="busy||paths.length>=32" @click="emit('add')"><ImagePlus :size="22" /><span>{{ t('点击添加或拖入图片') }}</span></button>
  </div>
</template>
<style scoped>
.wallpaper-gallery{display:grid;grid-template-columns:repeat(auto-fill,minmax(138px,1fr));gap:10px;padding:16px}
.wallpaper-item{position:relative;min-width:0;border:1px solid var(--border-2);border-radius:9px;overflow:hidden;background:var(--surface-2)}
.wallpaper-item.selected{border-color:var(--primary);box-shadow:0 0 0 1px var(--primary)}
.wallpaper-select{display:flex;flex-direction:column;align-items:center;justify-content:center;width:100%;height:120px;color:var(--text-2);background:transparent;overflow:hidden}
.wallpaper-select img{width:100%;height:94px;object-fit:cover}.wallpaper-select span{max-width:100%;padding:6px 9px;font-size:11px;text-overflow:ellipsis;white-space:nowrap;overflow:hidden}
.wallpaper-remove{position:absolute;right:5px;top:5px;display:grid;place-items:center;width:27px;height:27px;background:var(--surface);color:var(--text-2);border:1px solid var(--border-2);border-radius:7px}
.wallpaper-remove:hover{background:var(--danger-bg);color:var(--danger)}
.wallpaper-add{min-height:120px;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:8px;border:1px dashed var(--border-2);border-radius:9px;background:transparent;color:var(--text-3);padding:12px;font-size:11px}
.wallpaper-add:hover{border-color:var(--primary);color:var(--primary)}button:focus-visible{outline:2px solid var(--primary);outline-offset:2px}
</style>
