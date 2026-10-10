import {ref} from "vue";
import {invoke,isTauri} from "@tauri-apps/api/core";
import {applyAppearance,colorModes,colorThemes,type Appearance} from "./appearance";
import {localSounds,previewLocalSound,stopThemeSounds,type SoundSaveRequest,type SoundState,type SoundAsset} from "./themeSounds";
import {localIcon} from "./localIcon";
import {applyWallpaper,currentWallpaper} from "./wallpaper";
import {t} from "./i18n";
export type WallpaperPlayback="fixed"|"intervalRandom"|"startupRandom";
export type AppearanceFields=Pick<Appearance,"colorTheme"|"customAccent"|"colorMode">;
export interface ThemeDraft extends AppearanceFields {id:string;name:string;transparency:number;blurPx:number;iconSourcePath?:string|null;
  wallpaperSourcePaths:string[];selectedWallpaperIndex:number;wallpaperPlayback:WallpaperPlayback;intervalSeconds:number;sounds:SoundSaveRequest}
export interface PersonalizationDraft {formatVersion:1;mode:"solid"|"custom";solid:AppearanceFields;selectedThemeId?:string|null;themes:ThemeDraft[]}
export interface PersonalizationState {draft:PersonalizationDraft;warnings:string[]}
export const savedPersonalization=ref<PersonalizationState>({draft:{formatVersion:1,mode:"solid",solid:{colorTheme:"teal",colorMode:"system"},themes:[]},warnings:[]});
export const activeRuntimeTheme=ref<ThemeDraft>();
export const runtimeWarnings=ref<string[]>([]);
const clone=<T>(value:T):T=>JSON.parse(JSON.stringify(value)) as T;
export function createTheme(name:string,source?:ThemeDraft):ThemeDraft {
  return {...(source?clone(source):{colorTheme:"teal",colorMode:"system",transparency:28,blurPx:12,wallpaperSourcePaths:[],selectedWallpaperIndex:0,
    wallpaperPlayback:"fixed",intervalSeconds:60,sounds:{enabled:false,volume:60,sourcePaths:{}}}),id:crypto.randomUUID(),name:name.trim()};
}
export interface ThemeImportParts {colors:boolean;background:boolean;icon:boolean;sounds:boolean}
export function selectThemeImportParts(theme:ThemeDraft,parts:ThemeImportParts):ThemeDraft {
  const selected=clone(theme);const defaults=createTheme(theme.name);
  if(!parts.colors){selected.colorTheme=defaults.colorTheme;selected.colorMode=defaults.colorMode;delete selected.customAccent;}
  if(!parts.background){selected.wallpaperSourcePaths=[];selected.selectedWallpaperIndex=0;selected.wallpaperPlayback=defaults.wallpaperPlayback;
    selected.intervalSeconds=defaults.intervalSeconds;selected.transparency=defaults.transparency;selected.blurPx=defaults.blurPx;}
  if(!parts.icon)delete selected.iconSourcePath;
  if(!parts.sounds)selected.sounds=defaults.sounds;
  return selected;
}
export function createPersonalizationDraft():PersonalizationDraft {return clone(savedPersonalization.value.draft);}
export function selectedTheme(draft:PersonalizationDraft):ThemeDraft|undefined {return draft.themes.find(theme=>theme.id===draft.selectedThemeId);}
export function activeAppearance(draft=savedPersonalization.value.draft):AppearanceFields {return draft.mode==="custom"?selectedTheme(draft)??draft.solid:draft.solid;}
export async function loadPersonalization(legacyAppearance:AppearanceFields):Promise<void> {
  if(isTauri()) {
    savedPersonalization.value=await invoke<PersonalizationState>("load_personalization",{legacyAppearance});
    const loaded=savedPersonalization.value;
    void invoke<string[]>("apply_personalization_icon").then(warnings=>{
      if(savedPersonalization.value===loaded)loaded.warnings.push(...warnings);
    }).catch(error=>{if(savedPersonalization.value===loaded)loaded.warnings.push(String(error));});
  }
  else savedPersonalization.value={draft:{formatVersion:1,mode:"solid",solid:clone(legacyAppearance),themes:[]},warnings:[]};
}
export async function savePersonalization(draft:PersonalizationDraft):Promise<void> {
  const errors=validatePersonalizationDraft(draft);if(errors.length)throw new Error(errors[0]);
  const request=clone(draft);for(const theme of request.themes)theme.name=theme.name.trim();
  const saved=isTauri()?await invoke<PersonalizationState>("save_personalization",{request}):{draft:request,warnings:[]};
  savedPersonalization.value=saved;
}
export function validatePersonalizationDraft(draft:PersonalizationDraft):string[] {
  const errors:string[]=[];
  const appearance=(value:AppearanceFields)=>{if(!colorThemes.includes(value.colorTheme)||!colorModes.includes(value.colorMode)
    ||(value.colorTheme==="custom"&&!/^#[0-9a-f]{6}$/i.test(value.customAccent??"")))errors.push(t("请输入六位十六进制颜色，例如 #B83E49"));};
  appearance(draft.solid);
  const ids=new Set<string>();
  for(const theme of draft.themes){
    appearance(theme);
    if(!theme.name.trim()||[...theme.name.trim()].length>64)errors.push(t("主题名称须为 1–64 个字符"));
    if(ids.has(theme.id))errors.push(t("主题标识重复"));ids.add(theme.id);
    if(!Number.isInteger(theme.intervalSeconds)||theme.intervalSeconds<1||theme.intervalSeconds>86400)errors.push(t("切换间隔须为 1–86400 秒的整数"));
    if(theme.wallpaperSourcePaths.length>32)errors.push(t("每个主题最多添加 32 张图片"));
    if(!Number.isInteger(theme.selectedWallpaperIndex)||theme.selectedWallpaperIndex<0||theme.selectedWallpaperIndex>=Math.max(1,theme.wallpaperSourcePaths.length))errors.push(t("请选择有效的图片"));
    if(!["fixed","intervalRandom","startupRandom"].includes(theme.wallpaperPlayback)||!Number.isInteger(theme.transparency)||theme.transparency<0||theme.transparency>45
      ||!Number.isInteger(theme.blurPx)||theme.blurPx<0||theme.blurPx>24)errors.push(t("主题背景配置无效"));
    if(!Number.isInteger(theme.sounds.volume)||theme.sounds.volume<0||theme.sounds.volume>100)errors.push(t("音量须为 0–100"));
  }
  if(draft.mode==="custom"&&!selectedTheme(draft))errors.push(t("请选择一个主题"));
  return errors;
}
export function mergeImportedTheme(draft:PersonalizationDraft,theme:ThemeDraft):PersonalizationDraft {
  const next=clone(draft);const imported=clone(theme);const base=imported.name.trim();let suffix=1;
  while(next.themes.some(item=>item.name===imported.name)) {suffix++;const tail=` (${suffix})`;imported.name=[...base].slice(0,64-tail.length).join("")+tail;}
  if(next.themes.some(item=>item.id===imported.id))imported.id=crypto.randomUUID();
  next.themes.push(imported);next.mode="custom";next.selectedThemeId=imported.id;return next;
}
export function mergeThemeSounds(theme:ThemeDraft,sounds:SoundSaveRequest):ThemeDraft {
  const next=clone(theme);next.sounds={...sounds,sourcePaths:{...next.sounds.sourcePaths,...sounds.sourcePaths}};return next;
}
export function removeSelected(paths:string[],removedIndex:number,selectedIndex=removedIndex):{paths:string[];selectedIndex:number} {
  const next=paths.filter((_,index)=>index!==removedIndex);
  return {paths:next,selectedIndex:Math.max(0,Math.min(next.length-1,selectedIndex>removedIndex?selectedIndex-1:selectedIndex))};
}
export interface ImportedTheme {stageId:string;theme:ThemeDraft}
export interface ImportedSounds {stageId:string;sounds:SoundSaveRequest}
export function previewThemePackage(path:string):Promise<ImportedTheme> {return invoke("preview_theme_package",{path});}
export function previewThemeSoundPackage(path:string):Promise<ImportedSounds> {return invoke("preview_theme_sound_package",{path});}
export function discardThemeImports(stageIds:string[]):Promise<void> {return stageIds.length&&isTauri()?invoke("discard_theme_imports",{stageIds}):Promise.resolve();}
// Shared across settings mounts and the theme/wallpaper galleries. Explicit selections revalidate files.
const themeThumbnails=new Map<string,Promise<string>>();
export function previewThemeThumbnail(path:string,refresh=false):Promise<string> {
  const cached=refresh?undefined:themeThumbnails.get(path);
  if(cached){themeThumbnails.delete(path);themeThumbnails.set(path,cached);return cached;}
  const pending=invoke<string>("preview_theme_thumbnail",{path});themeThumbnails.set(path,pending);
  void pending.catch(()=>{if(themeThumbnails.get(path)===pending)themeThumbnails.delete(path);});
  while(themeThumbnails.size>64)themeThumbnails.delete(themeThumbnails.keys().next().value!);
  return pending;
}
export function exportThemePackage(path:string,theme:ThemeDraft,overwrite=false):Promise<string> {return invoke("export_theme_package",{path,theme:clone(theme),overwrite});}
export function themeExportFilename(name:string):string {
  let safe=name.replace(/[<>:"/\\|?*\x00-\x1f]/g,"_").replace(/[. ]+$/g,"")||"theme";
  if(/^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/i.test(safe))safe="_"+safe;
  return `${safe}.zip`;
}
let runtimeGeneration=0;
let appliedSoundSignature: string | undefined;
// Only retain the active theme's media, including in-flight reads shared by slider/mode updates.
const runtimeIcons=new Map<string,Promise<string>>();
const runtimeSounds=new Map<string,Promise<SoundAsset>>();
function loadRuntimeMedia<T>(cache:Map<string,Promise<T>>,key:string,load:()=>Promise<T>):Promise<T> {
  let pending=cache.get(key);
  if(!pending){pending=load();cache.set(key,pending);void pending.catch(()=>{if(cache.get(key)===pending)cache.delete(key);});}
  return pending;
}
export async function applyPersonalizationRuntime(draft:PersonalizationDraft,preview=false):Promise<void> {
  const generation=++runtimeGeneration;const theme=draft.mode==="custom"?selectedTheme(draft):undefined;
  runtimeWarnings.value=[];if(typeof document!=="undefined")applyAppearance(activeAppearance(draft));
  if(JSON.stringify(activeRuntimeTheme.value)!==JSON.stringify(theme))activeRuntimeTheme.value=theme?clone(theme):undefined;
  if(!theme?.wallpaperSourcePaths.length)applyWallpaper({mode:"color",transparency:theme?.transparency??28,blurPx:theme?.blurPx??12});
  else applyWallpaper({...currentWallpaper.value,transparency:theme.transparency,blurPx:theme.blurPx});
  for(const key of runtimeIcons.keys())if(key!==theme?.iconSourcePath)runtimeIcons.delete(key);
  const soundPaths=new Set(Object.values(theme?.sounds.sourcePaths??{}));
  for(const key of runtimeSounds.keys())if(!soundPaths.has(key))runtimeSounds.delete(key);
  if(!theme?.iconSourcePath)localIcon.value={mode:"color"};
  const soundSignature=JSON.stringify([theme?.id,preview,theme?.sounds]);
  const soundsChanged=soundSignature!==appliedSoundSignature;
  if(soundsChanged){stopThemeSounds();localSounds.value={enabled:false,volume:theme?.sounds.volume??60,files:{}};}
  if(!theme){appliedSoundSignature=soundSignature;return;}
  const warn=(error:unknown)=>{if(generation===runtimeGeneration)runtimeWarnings.value.push(String(error));};
  if(theme.iconSourcePath)try {
    const imageDataUrl=await loadRuntimeMedia(runtimeIcons,theme.iconSourcePath,()=>previewThemeThumbnail(theme.iconSourcePath!));
    if(generation===runtimeGeneration&&localIcon.value.imageDataUrl!==imageDataUrl)localIcon.value={mode:"image",imageDataUrl};
  }catch(error){if(generation===runtimeGeneration)localIcon.value={mode:"color"};warn(error);}
  if(generation!==runtimeGeneration)return;
  if(!soundsChanged)return;
  const sounds:SoundState={enabled:!preview&&theme.sounds.enabled,volume:theme.sounds.volume,files:{}};
  for(const [event,path] of Object.entries(theme.sounds.sourcePaths)){
    if(generation!==runtimeGeneration)return;
    if(path)try {sounds.files[event as keyof SoundState["files"]]=await loadRuntimeMedia(runtimeSounds,path,()=>previewLocalSound(path));}catch(error){warn(error);}
  }
  if(generation===runtimeGeneration){localSounds.value=sounds;appliedSoundSignature=runtimeWarnings.value.length?undefined:soundSignature;}
}
export function personalizationErrorLabel(error:unknown):string {const message=error instanceof Error?error.message:String(error);return t(message);}
