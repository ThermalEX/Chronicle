<script setup lang="ts">
import {computed,onBeforeUnmount,ref,watch} from "vue";
import {isTauri} from "@tauri-apps/api/core";
import {open,save as saveFile} from "@tauri-apps/plugin-dialog";
import {Monitor,Plus,Play,Square,RefreshCw,LogOut,X,PackagePlus} from "@lucide/vue";
import {createTheme,mergeImportedTheme,mergeThemeSounds,removeSelected,selectedTheme,previewThemePackage,previewThemeSoundPackage,
  discardThemeImports,exportThemePackage,themeExportFilename,previewThemeThumbnail,validatePersonalizationDraft,personalizationErrorLabel,selectThemeImportParts,type ThemeImportParts,type ImportedTheme,type PersonalizationDraft,type ThemeDraft} from "../services/personalization";
import {previewLocalSound,previewThemeSound,soundEvents,stopSoundPreview,type SoundEvent} from "../services/themeSounds";
import {colorThemes,type ColorTheme,type ColorMode} from "../services/appearance";
import {t} from "../services/i18n";
import ThemedSelect from "./ThemedSelect.vue";
import WallpaperGallery from "./WallpaperGallery.vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import defaultIcon from "../assets/icon.png";
const props=defineProps<{modelValue:PersonalizationDraft;busy:boolean}>();
const emit=defineEmits<{"update:modelValue":[draft:PersonalizationDraft];preview:[];warning:[message:string];busy:[busy:boolean]}>();
const draft=ref<PersonalizationDraft>(JSON.parse(JSON.stringify(props.modelValue)));
watch(()=>props.modelValue,value=>{if(JSON.stringify(value)!==JSON.stringify(draft.value))draft.value=JSON.parse(JSON.stringify(value));},{deep:true});
watch(draft,value=>emit("update:modelValue",JSON.parse(JSON.stringify(value))),{deep:true});
const theme=computed(()=>selectedTheme(draft.value));const appearance=computed(()=>draft.value.mode==="custom"?theme.value:draft.value.solid);
const working=ref(false);const error=ref("");const feedback=ref("");const intervalUnit=ref<"seconds"|"minutes">("seconds");
const gallery=ref<InstanceType<typeof WallpaperGallery>>();const iconUrl=ref("");const themeThumbnails=ref<Record<string,string>>({});
const themeLibrary=ref<HTMLElement>();const pendingImport=ref<ImportedTheme>();const importParts=ref<ThemeImportParts>({colors:true,background:true,icon:true,sounds:true});
const removingTheme=ref<ThemeDraft>();
const stages=new Set<string>();let dismissed=false;let iconRequest=0;
const disabled=computed(()=>props.busy||working.value||!!pendingImport.value||!!removingTheme.value);
const importChoices=computed(()=>[
  {key:"colors" as const,label:t("颜色配置"),available:true},
  {key:"background" as const,label:t("背景图片"),available:!!pendingImport.value?.theme.wallpaperSourcePaths.length},
  {key:"icon" as const,label:t("应用图标"),available:!!pendingImport.value?.theme.iconSourcePath},
  {key:"sounds" as const,label:t("音效"),available:Object.values(pendingImport.value?.theme.sounds.sourcePaths??{}).some(Boolean)},
]);
const colorNames:Record<ColorTheme,string>={teal:"青绿",indigo:"靛蓝",violet:"紫罗兰",amber:"琥珀",rose:"玫红",gray:"灰色",custom:"自定义颜色"};
const colorOptions=computed(()=>colorThemes.map(value=>({value,label:t(colorNames[value])})));
const modeOptions=computed(()=>[{value:"system",label:t("跟随系统")},{value:"light",label:t("日间模式")},{value:"dark",label:t("夜间模式")}]);
const playbackOptions=computed(()=>[{value:"fixed",label:t("不自动切换")},{value:"intervalRandom",label:t("定时随机")},{value:"startupRandom",label:t("启动随机")}]);
const soundNames:Record<SoundEvent,string>={connected:"设备连接",disconnected:"设备中断连接",connectionFailed:"设备未能连接",notification:"通知",default:"默认响声"};
const basename=(path?:string|null)=>path?.split(/[\\/]/).at(-1)??"";
function setMode(mode:"solid"|"custom"):void {draft.value.mode=mode;}
function addTheme():void {const created=createTheme(t("新主题"),theme.value);draft.value=mergeImportedTheme(draft.value,created);}
function requestRemoveTheme(id:string):void {if(disabled.value)return;removingTheme.value=draft.value.themes.find(item=>item.id===id);if(removingTheme.value)emit("busy",true);}
function cancelRemoveTheme():void {removingTheme.value=undefined;emit("busy",false);}
function confirmRemoveTheme():void {
  const id=removingTheme.value?.id;const index=draft.value.themes.findIndex(item=>item.id===id);if(index<0)return;
  draft.value.themes.splice(index,1);delete themeThumbnails.value[id!];
  if(draft.value.selectedThemeId===id)draft.value.selectedThemeId=draft.value.themes[Math.min(index,draft.value.themes.length-1)]?.id??null;
  if(!draft.value.themes.length)draft.value.mode="solid";
  cancelRemoveTheme();
}
function setColor(value:string|null):void {if(appearance.value&&value)appearance.value.colorTheme=value as ColorTheme;}
function setColorMode(value:string|null):void {if(appearance.value&&value)appearance.value.colorMode=value as ColorMode;}
function removeWallpaper(index:number):void {if(!theme.value)return;const next=removeSelected(theme.value.wallpaperSourcePaths,index,theme.value.selectedWallpaperIndex);theme.value.wallpaperSourcePaths=next.paths;theme.value.selectedWallpaperIndex=next.selectedIndex;}
async function run(action:()=>Promise<void>):Promise<void> {
  if(disabled.value)return;working.value=true;emit("busy",true);error.value="";feedback.value="";
  try{await action();}catch(reason){if(!dismissed)error.value=personalizationErrorLabel(reason);}finally{working.value=false;if(!dismissed)emit("busy",!!pendingImport.value);}
}
function desktop():boolean {if(isTauri())return true;error.value=t("请在桌面版导入或导出主题");return false;}
async function appendWallpapers(paths:string[]):Promise<void> {
  if(!theme.value||dismissed)return;const target=theme.value.id;
    for(const path of paths){
      const current=draft.value.themes.find(item=>item.id===target);if(!current||dismissed)return;
      if(current.wallpaperSourcePaths.length>=32){emit("warning",t("每个主题最多添加 32 张图片"));break;}
      if(current.wallpaperSourcePaths.some(saved=>saved.replaceAll("\\","/").toLowerCase()===path.replaceAll("\\","/").toLowerCase())){emit("warning",t("图片已在列表中：{name}",{name:basename(path)}));continue;}
      try{await previewThemeThumbnail(path,true);if(dismissed)return;current.wallpaperSourcePaths.push(path);current.selectedWallpaperIndex=current.wallpaperSourcePaths.length-1;}
      catch(reason){if(!dismissed)emit("warning",`${basename(path)}: ${personalizationErrorLabel(reason)}`);}
    }
}
async function addWallpapers(paths:string[]):Promise<void> {await run(()=>appendWallpapers(paths));}
async function chooseWallpapers():Promise<void> {if(disabled.value||!desktop())return;await run(async()=>{const paths=await open({multiple:true,filters:[{name:t("图片"),extensions:["png","jpg","jpeg","webp"]}]});if(!dismissed&&paths)await appendWallpapers(Array.isArray(paths)?paths:[paths]);});}
async function stageTheme(path:string):Promise<void> {
  const imported=await previewThemePackage(path);if(dismissed){await discardThemeImports([imported.stageId]);return;}
  stages.add(imported.stageId);pendingImport.value=imported;
  importParts.value={colors:true,background:!!imported.theme.wallpaperSourcePaths.length,icon:!!imported.theme.iconSourcePath,sounds:Object.values(imported.theme.sounds.sourcePaths).some(Boolean)};
}
async function importTheme():Promise<void> {if(disabled.value||!desktop())return;await run(async()=>{
  const path=await open({multiple:false,filters:[{name:t("主题包"),extensions:["zip"]}]});if(typeof path!=="string"||dismissed)return;
  await stageTheme(path);
});}
function confirmImport():void {
  if(!pendingImport.value||dismissed)return;
  draft.value=mergeImportedTheme(draft.value,selectThemeImportParts(pendingImport.value.theme,importParts.value));
  pendingImport.value=undefined;emit("busy",false);
}
async function cancelImport():Promise<void> {
  const pending=pendingImport.value;if(!pending)return;pendingImport.value=undefined;
  await run(async()=>{await discardThemeImports([pending.stageId]);stages.delete(pending.stageId);});
}
async function exportTheme(id=draft.value.selectedThemeId):Promise<void> {if(disabled.value||!desktop())return;const target=draft.value.themes.find(item=>item.id===id);if(!target)return;const edited=JSON.parse(JSON.stringify(target)) as ThemeDraft;
  const errors=validatePersonalizationDraft({formatVersion:1,mode:"custom",solid:{colorTheme:"teal",colorMode:"system"},themes:[edited],selectedThemeId:edited.id});if(errors.length){error.value=errors[0]!;return;}
  await run(async()=>{const path=await saveFile({defaultPath:themeExportFilename(edited.name),filters:[{name:"ZIP",extensions:["zip"]}]});
    if(!path||dismissed)return;
    // The native save dialog asks for overwrite confirmation before returning.
    const exported=await exportThemePackage(path,edited,true);if(!dismissed)feedback.value=t("已导出至：{value1}",{value1:exported});
  });
}
async function chooseIcon():Promise<void> {if(!desktop()||!theme.value)return;await run(async()=>{
  const path=await open({multiple:false,filters:[{name:t("图片"),extensions:["png","jpg","jpeg","webp"]}]});if(typeof path!=="string"||dismissed)return;
  await previewThemeThumbnail(path,true);if(!dismissed&&theme.value)theme.value.iconSourcePath=path;
});}
async function chooseSound(event:SoundEvent):Promise<void> {if(!desktop()||!theme.value)return;await run(async()=>{
  const path=await open({multiple:false,filters:[{name:"WAV",extensions:["wav"]}]});if(typeof path!=="string"||dismissed)return;
  await previewLocalSound(path);if(!dismissed&&theme.value)theme.value.sounds.sourcePaths[event]=path;
});}
async function auditionSound(event:SoundEvent):Promise<void> {const path=theme.value?.sounds.sourcePaths[event];if(!path)return;
  await run(async()=>{const asset=await previewLocalSound(path);if(!dismissed&&!await previewThemeSound(asset,theme.value?.sounds.volume??60))throw new Error(t("音效无法播放，请检查音量或重新选择文件。"));});
}
function removeSound(event:SoundEvent):void {stopSoundPreview();if(theme.value)theme.value.sounds.sourcePaths[event]=null;}
async function importSounds():Promise<void> {if(!desktop()||!theme.value)return;const id=theme.value.id;await run(async()=>{
  const path=await open({multiple:false,filters:[{name:t("声音包"),extensions:["zip"]}]});if(typeof path!=="string"||dismissed)return;
  const imported=await previewThemeSoundPackage(path);if(dismissed){await discardThemeImports([imported.stageId]);return;}
  stages.add(imported.stageId);const index=draft.value.themes.findIndex(item=>item.id===id);if(index>=0)draft.value.themes[index]=mergeThemeSounds(draft.value.themes[index]!,imported.sounds);
});}
watch(()=>theme.value?.iconSourcePath,async(path)=>{const request=++iconRequest;iconUrl.value="";if(!path||!isTauri())return;try{const url=await previewThemeThumbnail(path);if(request===iconRequest&&!dismissed)iconUrl.value=url;}catch{/* Default icon remains visible. */}},{immediate:true});
const thumbnailPaths=new Map<string,string>();
let thumbnailGeneration=0;
async function refreshThemeThumbnails(items:[string,string|undefined][]):Promise<void> {
  const generation=++thumbnailGeneration;
  if(!isTauri())return;
  const ids=new Set(items.map(([id])=>id));
  for(const id of thumbnailPaths.keys())if(!ids.has(id)){thumbnailPaths.delete(id);delete themeThumbnails.value[id];}
  let index=0;
  await Promise.all(Array.from({length:Math.min(4,items.length)},async()=>{
    while(!dismissed&&generation===thumbnailGeneration){
    const item=items[index++];if(!item)return;const [id,path]=item;
    if(!path){thumbnailPaths.delete(id);delete themeThumbnails.value[id];continue;}
    if(thumbnailPaths.get(id)===path)continue;
    thumbnailPaths.set(id,path);delete themeThumbnails.value[id];
    try{const url=await previewThemeThumbnail(path);if(!dismissed&&thumbnailPaths.get(id)===path)themeThumbnails.value[id]=url;}
    catch{if(thumbnailPaths.get(id)===path)thumbnailPaths.delete(id);}
    }
  }));
}
watch(()=>draft.value.themes.map(item=>[item.id,item.wallpaperSourcePaths[item.selectedWallpaperIndex]] as [string,string|undefined]),refreshThemeThumbnails,{immediate:true});
defineExpose({async acceptFileDrop(paths:string[],position:{x:number;y:number}):Promise<boolean>{
  if(disabled.value||draft.value.mode!=="custom")return false;
  const rect=themeLibrary.value?.getBoundingClientRect();
  if(rect&&position.x>=rect.left&&position.x<=rect.right&&position.y>=rect.top&&position.y<=rect.bottom){
    if(paths.length!==1||!paths[0]?.toLowerCase().endsWith(".zip")){error.value=t("请一次拖入一个 ZIP 主题包");return true;}
    if(desktop())await run(()=>stageTheme(paths[0]!));return true;
  }
  if(!gallery.value?.contains(position))return false;await addWallpapers(paths);return true;
},async discardImports():Promise<void>{dismissed=true;pendingImport.value=undefined;await discardThemeImports([...stages]);stages.clear();}});
onBeforeUnmount(()=>{dismissed=true;iconRequest++;stopSoundPreview();void discardThemeImports([...stages]).catch(()=>{});});
</script>
<template>
  <section class="personalization-settings">
    <div class="section-heading"><h3>{{ t('个性化') }}</h3><p>{{ t('选择纯色配色，或管理包含图片、图标与音效的定制主题。') }}</p></div>
    <section class="personalization-card"><h4>{{ t('主题类型') }}</h4><div class="personalization-row type-choices">
      <label><input type="radio" name="theme-mode" :checked="draft.mode==='solid'" :disabled="disabled" @change="setMode('solid')" />{{ t('纯色配色') }}</label>
      <label><input type="radio" name="theme-mode" :checked="draft.mode==='custom'" :disabled="disabled" @change="setMode('custom')" />{{ t('定制主题') }}</label>
    </div></section>
    <section v-if="draft.mode==='custom'" class="personalization-card" data-testid="theme-library"><h4 class="theme-library-heading"><span>{{ t('主题库') }}</span><button type="button" :disabled="disabled" @click="addTheme"><Plus :size="14" aria-hidden="true" />{{ t('新增主题') }}</button></h4>
      <div ref="themeLibrary" class="theme-list"><div v-for="item in draft.themes" :key="item.id" class="theme-item" :class="{selected:item.id===draft.selectedThemeId}">
        <button type="button" class="theme-select" :disabled="disabled" :aria-pressed="item.id===draft.selectedThemeId" @click="draft.selectedThemeId=item.id">
          <img v-if="themeThumbnails[item.id]" :src="themeThumbnails[item.id]" alt="" /><span v-else class="theme-placeholder" :style="{background:item.customAccent||'var(--primary)'}"></span><span :title="item.name">{{ item.name }}</span>
        </button><div class="theme-card-actions"><button type="button" :disabled="disabled" :aria-label="t('导出主题 {name}',{name:item.name})" :title="t('导出主题 {name}',{name:item.name})" @click="exportTheme(item.id)"><LogOut :size="14" aria-hidden="true" /></button><button type="button" class="theme-remove" :disabled="disabled" :aria-label="t('删除主题 {name}',{name:item.name})" :title="t('删除主题 {name}',{name:item.name})" @click="requestRemoveTheme(item.id)"><X :size="14" aria-hidden="true" /></button></div>
      </div><button type="button" class="theme-add" :disabled="disabled" @click="importTheme"><PackagePlus :size="22" aria-hidden="true" /><span>{{ t('点击选择或拖入主题包') }}</span></button></div>
      <label v-if="theme" class="personalization-row"><span>{{ t('主题名称') }}</span><input v-model="theme.name" type="text" :disabled="disabled" :aria-label="t('主题名称')" /></label>
    </section>
    <section v-if="appearance" class="personalization-card"><h4>{{ t('配色与显示') }}</h4>
      <div class="personalization-row"><span>{{ t('配色主题') }}</span><ThemedSelect :model-value="appearance.colorTheme" :options="colorOptions" :disabled="disabled" :label="t('配色主题')" @update:model-value="setColor" /></div>
      <label v-if="appearance.colorTheme==='custom'" class="personalization-row"><span>{{ t('强调色') }}</span><div class="accent-input"><input v-model="appearance.customAccent" type="color" :disabled="disabled" :aria-label="t('强调色')" /><input v-model="appearance.customAccent" type="text" :disabled="disabled" :aria-label="t('自定义颜色')" /></div></label>
      <div class="personalization-row"><span>{{ t('显示模式') }}</span><ThemedSelect :model-value="appearance.colorMode" :options="modeOptions" :disabled="disabled" :label="t('显示模式')" @update:model-value="setColorMode" /></div>
    </section>
    <template v-if="draft.mode==='custom'&&theme">
      <section class="personalization-card"><h4>{{ t('背景图片') }}</h4><p class="card-hint">{{ t('壁纸仅保存在本机，不会同步到云端。') }}</p>
        <WallpaperGallery ref="gallery" :paths="theme.wallpaperSourcePaths" :selected-index="theme.selectedWallpaperIndex" :busy="disabled" @add="chooseWallpapers" @remove="removeWallpaper" @select="theme.selectedWallpaperIndex=$event" />
        <div class="personalization-preview"><button type="button" :disabled="disabled" @click="emit('preview')"><Monitor :size="14" />{{ t('预览软件界面') }}</button></div>
        <div class="personalization-row"><span>{{ t('图片切换方式') }}</span><ThemedSelect :model-value="theme.wallpaperPlayback" :options="playbackOptions" :disabled="disabled" :label="t('图片切换方式')" @update:model-value="theme.wallpaperPlayback=$event as ThemeDraft['wallpaperPlayback']" /></div>
        <label v-if="theme.wallpaperPlayback==='intervalRandom'" class="personalization-row"><span>{{ t('切换间隔') }}</span><div class="interval-input"><input :value="theme.intervalSeconds/(intervalUnit==='minutes'?60:1)" type="number" min="1" :disabled="disabled" :aria-label="t('切换间隔')" @input="theme.intervalSeconds=Number(($event.target as HTMLInputElement).value)*(intervalUnit==='minutes'?60:1)" /><ThemedSelect v-model="intervalUnit" :options="[{value:'seconds',label:t('秒')},{value:'minutes',label:t('分钟')}]" :label="t('时间单位')" /></div></label>
        <p v-if="theme.wallpaperPlayback==='startupRandom'" class="card-hint">{{ t('每次完整启动软件时随机一张，运行期间不自动切换。') }}</p>
        <label class="personalization-slider">{{ t('面板透明度') }}<output>{{ theme.transparency }}%</output><input v-model.number="theme.transparency" type="range" min="0" max="45" :disabled="disabled" :aria-label="t('面板透明度')" /></label>
        <label class="personalization-slider">{{ t('磨砂强度') }}<output>{{ theme.blurPx }} px</output><input v-model.number="theme.blurPx" type="range" min="0" max="24" :disabled="disabled" :aria-label="t('磨砂强度')" /></label>
      </section>
      <section class="personalization-card"><h4>{{ t('应用图标') }}</h4><div class="personalization-row"><span>{{ t('关于页、窗口、任务栏与托盘；图标只保存在本机。') }}</span><div class="actions"><img class="icon-preview" :src="iconUrl||defaultIcon" :alt="t('图标预览')" /><button type="button" :disabled="disabled" @click="chooseIcon">{{ t('选择图标') }}</button><button type="button" :disabled="disabled" @click="theme.iconSourcePath=undefined">{{ t('默认图标') }}</button></div></div></section>
      <section class="personalization-card"><h4 id="sounds-card-title">{{ t('音效') }}</h4>
        <div class="personalization-row"><label><input v-model="theme.sounds.enabled" type="checkbox" role="switch" :disabled="disabled" />{{ t('启用主题音效') }}</label><button type="button" :disabled="disabled" @click="importSounds">{{ t('导入声音包') }}</button></div>
        <label class="personalization-slider">{{ t('音量') }}<output>{{ theme.sounds.volume }}%</output><input v-model.number="theme.sounds.volume" type="range" min="0" max="100" :disabled="disabled" :aria-label="t('音量')" /></label>
        <div v-for="event in soundEvents" :key="event" class="personalization-row sound-row"><span><b>{{ t(soundNames[event]) }}</b><small>{{ basename(theme.sounds.sourcePaths[event])||t('未选择音效') }}</small></span><div class="actions"><button type="button" :disabled="disabled" @click="chooseSound(event)">{{ t('选择音效') }}</button><button type="button" :disabled="disabled||!theme.sounds.sourcePaths[event]||theme.sounds.volume===0" :aria-label="t('试听{value1}音效',{value1:t(soundNames[event])})" @click="auditionSound(event)"><Play :size="14" />{{ t('试听') }}</button><button type="button" :disabled="disabled||!theme.sounds.sourcePaths[event]" :aria-label="t('移除{value1}音效',{value1:t(soundNames[event])})" @click="removeSound(event)">{{ t('移除') }}</button></div></div>
        <div class="personalization-row"><small>{{ t('支持 PCM WAV，单个文件最大 5 MiB、最长 30 秒。连续通知不会叠加播放。') }}</small><button type="button" @click="stopSoundPreview"><Square :size="14" />{{ t('停止试听') }}</button></div>
      </section>
    </template>
    <p v-if="working" class="personalization-feedback" role="status"><RefreshCw :size="14" class="spinning" />{{ t('正在处理主题…') }}</p><p v-if="error" class="personalization-error" role="alert">{{ error }}</p><p v-if="feedback" class="personalization-feedback" role="status">{{ feedback }}</p>
    <ConfirmDialog v-if="pendingImport" :title="t('选择主题内容')" :message="pendingImport.theme.name" :confirm-label="t('应用到预览')" @cancel="cancelImport" @confirm="confirmImport" @keydown.esc.stop.prevent="cancelImport">
      <template #body-extra><div class="theme-import-options"><label v-for="choice in importChoices" :key="choice.key"><input v-model="importParts[choice.key]" type="checkbox" :disabled="!choice.available" /><span>{{ choice.label }}</span><small v-if="!choice.available">{{ t('主题包未包含此内容') }}</small></label><p>{{ t('未勾选的内容不载入；颜色使用默认配置。预览后点击保存设置才生效。') }}</p></div></template>
    </ConfirmDialog>
    <ConfirmDialog v-if="removingTheme" :title="t('删除主题？')" :message="t('从主题库移除“{name}”？保存后生效，不删除原始主题包。',{name:removingTheme.name})" :confirm-label="t('删除')" destructive @cancel="cancelRemoveTheme" @confirm="confirmRemoveTheme" @keydown.esc.stop.prevent="cancelRemoveTheme" />
  </section>
</template>
<style scoped>
.section-heading{margin-bottom:20px}.section-heading h3{font-size:18px}.section-heading p,.card-hint{color:var(--text-3);font-size:11px;line-height:1.6;margin:6px 0 0}
.personalization-card{border:1px solid var(--border-2);border-radius:10px;margin:0 0 18px;overflow:hidden}.personalization-card h4{font-size:12px;padding:15px 16px;margin:0;border-bottom:1px solid var(--border);color:var(--text-2)}
.personalization-row{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:15px 16px;border-top:1px solid var(--border);font-size:12px;min-width:0}.personalization-card h4+.personalization-row{border-top:0}.personalization-row>span{flex:1;min-width:0}.personalization-row small{display:block;margin-top:5px;color:var(--text-3);font-size:11px;overflow-wrap:anywhere}
.card-hint{padding:0 16px 12px}.actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.type-choices{justify-content:flex-start;gap:26px}.type-choices label{display:flex;align-items:center;gap:8px}
button{display:inline-flex;align-items:center;justify-content:center;gap:6px;min-height:32px;padding:7px 10px;border:1px solid var(--border-2);border-radius:7px;background:var(--surface-2);color:var(--primary);font-size:11px;font-weight:650}button:hover{background:var(--hover)}button:disabled{opacity:.5;cursor:default}button:focus-visible,input:focus-visible{outline:2px solid var(--primary);outline-offset:2px}
input[type=text],input[type=number]{min-width:0;width:170px;color:var(--text);background:var(--field);border:1px solid var(--border-2);padding:8px 10px;border-radius:7px}input[type=radio],input[type=checkbox],input[type=range]{accent-color:var(--primary)}
.theme-library-heading{display:flex;align-items:center;justify-content:space-between;gap:12px}.theme-list{display:grid;grid-template-columns:repeat(auto-fill,minmax(138px,1fr));gap:10px;padding:16px}.theme-item{position:relative;min-width:0;border:1px solid var(--border-2);border-radius:9px;overflow:hidden;background:var(--surface-2)}.theme-item.selected{border-color:var(--primary);box-shadow:0 0 0 1px var(--primary)}
.theme-select{padding:0;border:0;border-radius:0;display:flex;flex-direction:column;min-width:0;width:100%;height:120px;overflow:hidden;gap:0;background:transparent;color:var(--text-2)}.theme-select img,.theme-placeholder{width:100%;height:94px;object-fit:cover;flex-shrink:0}.theme-select>span:last-child{max-width:100%;padding:6px 9px;font-size:11px;text-overflow:ellipsis;white-space:nowrap;overflow:hidden}
.theme-card-actions{position:absolute;right:5px;top:5px;display:flex;gap:4px}.theme-card-actions button{display:grid;place-items:center;min-height:27px;width:27px;height:27px;padding:0;background:var(--surface);color:var(--text-2);border:1px solid var(--border-2);border-radius:7px}.theme-card-actions button:hover{background:var(--hover);color:var(--primary)}.theme-card-actions .theme-remove:hover{background:var(--danger-soft);color:var(--danger)}
.theme-add{min-height:120px;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:8px;border:1px dashed var(--border-2);border-radius:9px;background:transparent;color:var(--text-3);padding:12px;font-size:11px}.theme-add:hover{border-color:var(--primary);color:var(--primary);background:transparent}
.theme-import-options{padding:0 22px 18px}.theme-import-options label{display:flex;align-items:center;gap:10px;min-height:38px;font-size:12px;color:var(--text-2)}.theme-import-options small{margin-left:auto;color:var(--text-3);font-size:11px}.theme-import-options p{color:var(--text-3);font-size:11px;line-height:1.6;margin:12px 0 0}
.personalization-preview{display:flex;justify-content:center;padding:0 16px 16px}.personalization-slider{display:grid;grid-template-columns:1fr auto;gap:9px;padding:14px 16px;font-size:11px;color:var(--text-3)}.personalization-slider input{grid-column:1/-1;width:100%}.accent-input,.interval-input{display:flex;align-items:center;gap:8px}.accent-input input[type=text]{width:95px}.accent-input input[type=color]{width:38px;height:32px;border:1px solid var(--border-2);border-radius:6px;background:var(--field)}.interval-input input{width:72px}.icon-preview{width:38px;height:38px;object-fit:contain}.personalization-error{color:var(--danger);font-size:12px;line-height:1.6}.personalization-feedback{display:flex;align-items:center;gap:8px;color:var(--text-3);font-size:11px;overflow-wrap:anywhere}
@media(max-width:1100px){.sound-row{align-items:flex-start;flex-direction:column}.personalization-row{flex-wrap:wrap}.personalization-row>span{flex-basis:40%}.type-choices{gap:15px}}
</style>
