<script setup lang="ts">
import {nextTick,onBeforeUnmount,ref,watch} from "vue";
import type {WallpaperFrame} from "../services/wallpaperPlayer";
const props=defineProps<{frame:WallpaperFrame}>();
const visible=ref(true);let request=0;let animation:number|undefined;
watch(()=>props.frame.current,async()=>{
  const version=++request;cancelAnimationFrame(animation??0);
  visible.value=!props.frame.transitionMs;await nextTick();
  if(version!==request||!props.frame.transitionMs)return;
  animation=requestAnimationFrame(()=>{animation=requestAnimationFrame(()=>{if(version===request)visible.value=true;});});
});
onBeforeUnmount(()=>{request++;cancelAnimationFrame(animation??0);});
</script>
<template>
  <div class="wallpaper-layers" aria-hidden="true">
    <img v-if="frame.previous" class="app-wallpaper" :src="frame.previous" alt="" :draggable="false" />
    <img v-if="frame.current" class="app-wallpaper" :src="frame.current" alt="" :draggable="false"
      :style="{opacity:visible?1:0,transition:`opacity ${frame.transitionMs}ms ease-in-out`}" />
  </div>
</template>
<style scoped>
.wallpaper-layers{position:absolute;z-index:-1;inset:0;pointer-events:none;overflow:hidden}
.wallpaper-layers .app-wallpaper{z-index:auto}
</style>
