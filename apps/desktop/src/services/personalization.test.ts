import {beforeEach,expect,it,vi} from "vitest";
vi.mock("@tauri-apps/api/core",()=>({isTauri:()=>true,invoke:vi.fn()}));
import {invoke} from "@tauri-apps/api/core";
import {createTheme,createPersonalizationDraft,savedPersonalization,savePersonalization,validatePersonalizationDraft,
  mergeImportedTheme,mergeThemeSounds,removeSelected,exportThemePackage,applyPersonalizationRuntime,selectThemeImportParts,previewThemeThumbnail,loadPersonalization} from "./personalization";
import {localIcon,currentAppIcon} from "./localIcon";
import {localSounds, playThemeSound, stopThemeSounds} from "./themeSounds";
import {currentWallpaper} from "./wallpaper";
beforeEach(()=>{
  vi.resetAllMocks();
  const theme=createTheme("牧濑红莉栖");
  savedPersonalization.value={draft:{formatVersion:1,mode:"custom",solid:{colorTheme:"teal",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]},warnings:[]};
});
it("returns saved colors without waiting for native icon work", async () => {
  const stored=structuredClone(JSON.parse(JSON.stringify(savedPersonalization.value)));
  stored.draft.themes[0].colorTheme="rose";
  let finish!: (value:string[])=>void;
  vi.mocked(invoke).mockImplementation(async command => {
    if(command==="load_personalization")return stored;
    if(command==="apply_personalization_icon")return await new Promise<string[]>(resolve=>{finish=resolve;});
    throw new Error(String(command));
  });
  await loadPersonalization({colorTheme:"indigo",colorMode:"dark"});
  expect(savedPersonalization.value.draft.themes[0]!.colorTheme).toBe("rose");
  expect(invoke).toHaveBeenCalledWith("apply_personalization_icon");
  finish([]);
});
it("shares pending and completed thumbnails between settings openings",async()=>{
  let finish!:(url:string)=>void;
  vi.mocked(invoke).mockReturnValue(new Promise<string>(resolve=>{finish=resolve;}));
  const first=previewThemeThumbnail("shared-thumbnail.png");const second=previewThemeThumbnail("shared-thumbnail.png");
  finish("data:image/png;base64,thumbnail");
  expect(await first).toBe("data:image/png;base64,thumbnail");expect(await second).toBe(await first);
  expect(await previewThemeThumbnail("shared-thumbnail.png")).toBe(await first);
  expect(invoke).toHaveBeenCalledTimes(1);
});
it("retries failed thumbnail reads instead of keeping a rejected cache entry",async()=>{
  vi.mocked(invoke).mockRejectedValueOnce(Error("Unreadable")).mockResolvedValueOnce("recovered");
  await expect(previewThemeThumbnail("retry-thumbnail.png")).rejects.toThrow("Unreadable");
  expect(await previewThemeThumbnail("retry-thumbnail.png")).toBe("recovered");
  expect(invoke).toHaveBeenCalledTimes(2);
});
it("revalidates explicitly reselected files even when their thumbnail was cached",async()=>{
  vi.mocked(invoke).mockResolvedValueOnce("original").mockRejectedValueOnce(Error("Changed file is invalid"));
  expect(await previewThemeThumbnail("reselected-thumbnail.png")).toBe("original");
  await expect(previewThemeThumbnail("reselected-thumbnail.png",true)).rejects.toThrow("Changed file is invalid");
  expect(invoke).toHaveBeenCalledTimes(2);
});
it("bounds thumbnail memory and reloads an evicted image",async()=>{
  vi.mocked(invoke).mockResolvedValue("thumbnail");
  for(let index=0;index<65;index++)await previewThemeThumbnail(`bounded-${index}.png`);
  vi.mocked(invoke).mockClear();await previewThemeThumbnail("bounded-0.png");
  expect(invoke).toHaveBeenCalledTimes(1);
});
it("removes all unchecked imported resources and resets color configuration",()=>{
  const theme=createTheme("主题包");Object.assign(theme,{colorTheme:"custom",customAccent:"#B83E49",colorMode:"dark",wallpaperSourcePaths:["a.png"],iconSourcePath:"icon.png",transparency:40,blurPx:20,sounds:{enabled:true,volume:40,sourcePaths:{notification:"notice.wav"}}});
  const filtered=selectThemeImportParts(theme,{colors:false,background:false,icon:false,sounds:false});
  expect(filtered).toMatchObject({id:theme.id,name:"主题包",colorTheme:"teal",colorMode:"system",wallpaperSourcePaths:[],selectedWallpaperIndex:0,transparency:28,blurPx:12,sounds:{enabled:false,volume:60,sourcePaths:{}}});
  expect(filtered.customAccent).toBeUndefined();expect(filtered.iconSourcePath).toBeUndefined();expect(theme.wallpaperSourcePaths).toEqual(["a.png"]);
});
it("keeps selected package parts intact and independent",()=>{
  const theme=createTheme("主题包");theme.wallpaperSourcePaths=["a.png","b.png"];theme.selectedWallpaperIndex=1;theme.wallpaperPlayback="intervalRandom";theme.intervalSeconds=120;theme.sounds={enabled:true,volume:40,sourcePaths:{notification:"notice.wav"}};
  const filtered=selectThemeImportParts(theme,{colors:true,background:true,icon:true,sounds:true});expect(filtered).toEqual(theme);
  filtered.wallpaperSourcePaths.push("c.png");filtered.sounds.sourcePaths.notification="changed.wav";
  expect(theme.wallpaperSourcePaths).toEqual(["a.png","b.png"]);expect(theme.sounds.sourcePaths.notification).toBe("notice.wav");
});
it("isolates edits across themes and saved state",()=>{
  const draft=createPersonalizationDraft();draft.themes[0]!.name="编辑中";
  draft.themes[0]!.wallpaperSourcePaths.push("C:/a.png");
  expect(savedPersonalization.value.draft.themes[0]!.name).toBe("牧濑红莉栖");
  expect(savedPersonalization.value.draft.themes[0]!.wallpaperSourcePaths).toEqual([]);
});
it("retains saved state after rejected persistence",async()=>{
  const before=createPersonalizationDraft();vi.mocked(invoke).mockRejectedValue(new Error("disk full"));
  const draft=createPersonalizationDraft();draft.themes[0]!.name="新名称";
  await expect(savePersonalization(draft)).rejects.toThrow("disk full");
  expect(savedPersonalization.value.draft).toEqual(before);
});
it("validates exact name interval image and selection boundaries",()=>{
  const draft=createPersonalizationDraft();const theme=draft.themes[0]!;
  theme.name="栖".repeat(64);theme.intervalSeconds=86400;
  expect(validatePersonalizationDraft(draft)).toEqual([]);
  for(const interval of [0,86401,1.5,NaN]) {theme.intervalSeconds=interval;expect(validatePersonalizationDraft(draft).length).toBeGreaterThan(0);}
  theme.intervalSeconds=1;theme.name="栖".repeat(65);expect(validatePersonalizationDraft(draft).length).toBeGreaterThan(0);
  theme.name="栖";theme.wallpaperSourcePaths=Array.from({length:33},(_,i)=>`${i}.png`);expect(validatePersonalizationDraft(draft).length).toBeGreaterThan(0);
});
it("adds duplicate imports as distinct identities and names",()=>{
  const draft=createPersonalizationDraft();const imported=createTheme("牧濑红莉栖");
  const next=mergeImportedTheme(draft,imported);
  expect(next.themes.map(t=>t.name)).toEqual(["牧濑红莉栖","牧濑红莉栖 (2)"]);
  expect(next.themes[0]!.id).not.toBe(next.themes[1]!.id);expect(draft.themes).toHaveLength(1);
});
it("merges only sound slots present in a sound package",()=>{
  const theme=createTheme("主题");theme.sounds.sourcePaths={notification:"old.wav",connected:"old-connected.wav"};
  const updated=mergeThemeSounds(theme,{enabled:true,volume:40,sourcePaths:{connected:"new.wav"}});
  expect(updated.sounds.sourcePaths).toEqual({notification:"old.wav",connected:"new.wav"});
  expect(updated.name).toBe("主题");expect(updated.sounds.volume).toBe(40);
});
it("removes only the chosen item and selects its adjacent item",()=>{
  expect(removeSelected(["a","b","c"],1)).toEqual({paths:["a","c"],selectedIndex:1});
  expect(removeSelected(["a"],0)).toEqual({paths:[],selectedIndex:0});
  expect(removeSelected(["a","b","c"],0,2)).toEqual({paths:["b","c"],selectedIndex:1});
});
it("exports an edited theme without persisting settings",async()=>{
  vi.mocked(invoke).mockImplementation(async(command)=>{if(command==="export_theme_package")return "edited.zip";throw new Error("unexpected save");});
  const theme=createTheme("编辑中");const before=createPersonalizationDraft();
  expect(await exportThemePackage("edited.zip",theme,false)).toBe("edited.zip");
  expect(savedPersonalization.value.draft).toEqual(before);
});
it("solid mode removes runtime theme resources without deleting stored themes",async()=>{
  localIcon.value={mode:"image",imageDataUrl:"custom"};localSounds.value={enabled:true,volume:50,files:{}};
  const draft=createPersonalizationDraft();draft.mode="solid";
  await applyPersonalizationRuntime(draft);
  expect(currentAppIcon.value).not.toBe("custom");expect(localSounds.value.enabled).toBe(false);
  expect(currentWallpaper.value.mode).toBe("color");expect(draft.themes).toHaveLength(1);
});
it("keeps draft sounds muted until settings are saved",async()=>{
  const draft=createPersonalizationDraft();draft.themes[0]!.sounds.enabled=true;
  await applyPersonalizationRuntime(draft,true);
  expect(localSounds.value.enabled).toBe(false);
  await applyPersonalizationRuntime(draft);
  expect(localSounds.value.enabled).toBe(true);
});
it("does not reload icon or sound assets when only appearance changes",async()=>{
  const draft=createPersonalizationDraft();const theme=draft.themes[0]!;
  theme.iconSourcePath="mode-icon.png";theme.sounds={enabled:true,volume:60,sourcePaths:{notification:"mode-notice.wav"}};
  vi.mocked(invoke).mockImplementation(async(command)=>command==="preview_local_sound"?{name:"notice.wav",dataUrl:"sound"}:"loaded");
  await applyPersonalizationRuntime(draft);vi.mocked(invoke).mockClear();
  theme.colorMode="light";theme.customAccent="#B83E49";
  await applyPersonalizationRuntime(draft);
  expect(invoke).not.toHaveBeenCalled();expect(localIcon.value.imageDataUrl).toBe("loaded");
  expect(localSounds.value.files.notification).toEqual({name:"notice.wav",dataUrl:"sound"});
});
it("uses a thumbnail rather than sending the full icon image to the webview", async () => {
  const draft=createPersonalizationDraft();draft.themes[0]!.iconSourcePath="large-ui-icon.png";
  vi.mocked(invoke).mockResolvedValue("small-icon");
  await applyPersonalizationRuntime(draft);
  expect(invoke).toHaveBeenCalledWith("preview_theme_thumbnail",{path:"large-ui-icon.png"});
  expect(localIcon.value.imageDataUrl).toBe("small-icon");
});
it("updates sound preview and volume without rereading unchanged files",async()=>{
  const draft=createPersonalizationDraft();const theme=draft.themes[0]!;
  theme.sounds={enabled:true,volume:60,sourcePaths:{notification:"volume-notice.wav"}};
  vi.mocked(invoke).mockResolvedValue({name:"notice.wav",dataUrl:"sound-data"});
  await applyPersonalizationRuntime(draft,true);vi.mocked(invoke).mockClear();theme.sounds.volume=30;
  await applyPersonalizationRuntime(draft);
  expect(invoke).not.toHaveBeenCalled();expect(localSounds.value).toMatchObject({enabled:true,volume:30,files:{notification:{name:"notice.wav",dataUrl:"sound-data"}}});
});
it("stops loading obsolete theme sounds after switching away during icon loading",async()=>{
  const draft=createPersonalizationDraft();const theme=draft.themes[0]!;
  theme.iconSourcePath="slow-icon.png";theme.sounds.sourcePaths={notification:"obsolete.wav"};
  let finish!:(value:string)=>void;
  vi.mocked(invoke).mockImplementation(async(command)=>command==="preview_theme_thumbnail"?await new Promise<string>(resolve=>{finish=resolve;}):"sound");
  const loading=applyPersonalizationRuntime(draft);await applyPersonalizationRuntime({...draft,mode:"solid"});finish("old-icon");await loading;
  expect(invoke).toHaveBeenCalledTimes(1);expect(localIcon.value.mode).toBe("color");expect(localSounds.value.files).toEqual({});
});
it("does not stop active audio or replace loaded media when only colors change", async () => {
  const draft=createPersonalizationDraft();const theme=draft.themes[0]!;
  theme.sounds={enabled:true,volume:60,sourcePaths:{notification:"playing-notice.wav"}};
  vi.mocked(invoke).mockResolvedValue({name:"notice.wav",dataUrl:"sound-data"});
  const pause=vi.fn();
  vi.stubGlobal("Audio", class {volume=1;paused=false;ended=false;async play(){} pause=pause;});
  try {
    await applyPersonalizationRuntime(draft);
    expect(await playThemeSound("notification")).toBe(true);
    const loaded=localSounds.value;
    theme.colorMode="light";
    await applyPersonalizationRuntime(draft);
    expect(pause).not.toHaveBeenCalled();
    expect(localSounds.value).toBe(loaded);
  } finally { stopThemeSounds(); vi.unstubAllGlobals(); }
});
it("does not reapply the active theme when editing an inactive theme", async () => {
  const draft=createPersonalizationDraft();
  const extra=createTheme("unused");draft.themes.push(extra);
  await applyPersonalizationRuntime(draft);
  const loaded=localSounds.value;
  extra.colorTheme="rose";extra.name="renamed";
  await applyPersonalizationRuntime(draft,true);
  const preview=localSounds.value;
  extra.name="renamed again";
  await applyPersonalizationRuntime(draft,true);
  expect(localSounds.value).toBe(preview);
  expect(loaded.files).toEqual({});
});
