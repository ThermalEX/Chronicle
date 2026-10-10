import {beforeEach,expect,it,vi} from "vitest";
vi.mock("@tauri-apps/api/core",()=>({isTauri:vi.fn(()=>false),invoke:vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn(),save:vi.fn()}));
import {createSSRApp} from "vue";import {renderToString} from "@vue/server-renderer";
import PersonalizationSettings from "./PersonalizationSettings.vue";
import {createTheme,validatePersonalizationDraft,type PersonalizationDraft} from "../services/personalization";
import {isTauri,invoke} from "@tauri-apps/api/core";
import {open,save} from "@tauri-apps/plugin-dialog";
beforeEach(()=>{vi.mocked(isTauri).mockReturnValue(false);vi.mocked(invoke).mockReset();vi.mocked(open).mockReset();vi.mocked(save).mockReset();});
async function editor(busy=false,initial=draft("custom")){let state:any;let exposed:any;const warnings:string[]=[];const component={...PersonalizationSettings,setup(p:unknown,c:any){state=(PersonalizationSettings as any).setup(p,{...c,expose(value:any){exposed=value;c.expose(value);}});return state;}};await renderToString(createSSRApp(component,{modelValue:initial,busy,onWarning:(message:string)=>warnings.push(message)}));return {state,exposed,warnings};}
function draft(mode:"solid"|"custom"):PersonalizationDraft {const theme=createTheme("牧濑红莉栖");return {formatVersion:1,mode,solid:{colorTheme:"teal",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]};}
function emptyDraft():PersonalizationDraft {return {formatVersion:1,mode:"solid",solid:{colorTheme:"teal",colorMode:"system"},themes:[]};}
it("leaves an empty library unchanged when switching to custom mode",async()=>{
  const {state}=await editor(false,emptyDraft());
  state.setMode("custom");state.setMode("solid");state.setMode("custom");
  expect(state.draft.value.mode).toBe("custom");expect(state.draft.value.themes).toEqual([]);
  expect(state.theme.value).toBeUndefined();expect(state.draft.value.selectedThemeId).toBeUndefined();
  expect(validatePersonalizationDraft(state.draft.value)).toEqual(["请选择一个主题"]);
});
it("creates a theme only when explicitly added to the empty library",async()=>{
  const {state}=await editor(false,emptyDraft());state.setMode("custom");state.addTheme();
  expect(state.draft.value.themes).toHaveLength(1);expect(state.theme.value.name).toBe("新主题");
  expect(validatePersonalizationDraft(state.draft.value)).toEqual([]);
});
it("imports into the empty library without adding a placeholder theme",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/kurisu.zip");
  vi.mocked(invoke).mockResolvedValue({stageId:crypto.randomUUID(),theme:createTheme("牧濑红莉栖")});
  const {state}=await editor(false,emptyDraft());state.setMode("custom");await state.importTheme();
  expect(state.draft.value.themes).toEqual([]);state.confirmImport();
  expect(state.draft.value.themes.map((item:any)=>item.name)).toEqual(["牧濑红莉栖"]);
  expect(state.theme.value.name).toBe("牧濑红莉栖");
});
it("hides the library in solid mode and shows only solid controls",async()=>{
  const html=await renderToString(createSSRApp(PersonalizationSettings,{modelValue:draft("solid"),busy:false}));
  expect(html).not.toContain('data-testid="theme-library"');expect(html).toContain("纯色配色");expect(html).not.toContain("导入声音包");
});
it("adds and edits multiple themes without replacing the first draft",async()=>{const {state}=await editor();state.theme.value.name="最初";state.addTheme();state.theme.value.name="第二个";expect(state.draft.value.themes.map((item:any)=>item.name)).toEqual(["最初","第二个"]);state.draft.value.selectedThemeId=state.draft.value.themes[0].id;expect(state.theme.value.name).toBe("最初");state.setMode("solid");expect(state.appearance.value).toEqual(state.draft.value.solid);expect(state.draft.value.themes).toHaveLength(2);});
it("keeps valid images and reports invalid and duplicate paths individually",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockImplementation(async(_command,args:any)=>{if(args.path.includes("bad"))throw Error("invalid image");return "thumbnail";});const {state,warnings}=await editor();await state.addWallpapers(["C:/good.png","C:/bad.png","c:\\good.png","C:/other.png"]);expect(state.theme.value.wallpaperSourcePaths).toEqual(["C:/good.png","C:/other.png"]);expect(warnings).toHaveLength(2);expect(state.theme.value.selectedWallpaperIndex).toBe(1);});
it("only removes the selected reference and never deletes a source file",async()=>{const {state}=await editor();state.theme.value.wallpaperSourcePaths=["a.png","b.png","c.png"];state.theme.value.selectedWallpaperIndex=1;state.removeWallpaper(1);expect(state.theme.value.wallpaperSourcePaths).toEqual(["a.png","c.png"]);expect(state.theme.value.selectedWallpaperIndex).toBe(1);expect(invoke).not.toHaveBeenCalled();});
it("validates audio before editing and ignores a late selection after dismissal",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/sound.wav");vi.mocked(invoke).mockRejectedValueOnce(Error("Invalid WAV"));const {state,exposed}=await editor();await state.chooseSound("notification");expect(state.error.value).toBe("Invalid WAV");expect(state.theme.value.sounds.sourcePaths).toEqual({});let complete!:(value:unknown)=>void;vi.mocked(invoke).mockReturnValueOnce(new Promise(resolve=>complete=resolve));const choosing=state.chooseSound("notification");await Promise.resolve();await exposed.discardImports();complete({name:"sound.wav",dataUrl:"data:audio/wav;base64,AA=="});await choosing;expect(state.theme.value.sounds.sourcePaths).toEqual({});});
it("exports the selected edited theme ZIP without saving unrelated settings",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(save).mockResolvedValue("C:/exports/编辑中.zip");vi.mocked(invoke).mockResolvedValue("C:/exports/编辑中.zip");const {state}=await editor();state.theme.value.name="编辑中";state.theme.value.colorTheme="custom";state.theme.value.customAccent="#d93145";state.theme.value.sounds.sourcePaths={notification:"C:/sound.wav",default:null};await state.exportTheme();expect(save).toHaveBeenCalledWith(expect.objectContaining({defaultPath:"编辑中.zip"}));expect(invoke).toHaveBeenCalledWith("export_theme_package",expect.objectContaining({theme:expect.objectContaining({name:"编辑中",customAccent:"#d93145",sounds:expect.objectContaining({sourcePaths:{notification:"C:/sound.wav",default:null}})})}));expect(invoke).not.toHaveBeenCalledWith("save_personalization",expect.anything());});
it("does not export when the save picker is cancelled or the color is invalid",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(save).mockResolvedValue(null);const {state}=await editor();await state.exportTheme();expect(invoke).not.toHaveBeenCalled();state.theme.value.colorTheme="custom";state.theme.value.customAccent="#bad";await state.exportTheme();expect(save).toHaveBeenCalledTimes(1);expect(state.error.value).not.toBe("");});
it("keeps edited values after an export failure and permits retry",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(save).mockResolvedValue("C:/theme.zip");vi.mocked(invoke).mockRejectedValueOnce(Error("Disk full")).mockResolvedValueOnce("C:/theme.zip");const {state}=await editor();state.theme.value.name="编辑中";await state.exportTheme();expect(state.error.value).toBe("Disk full");expect(state.working.value).toBe(false);await state.exportTheme();expect(state.error.value).toBe("");expect(state.feedback.value).toContain("theme.zip");expect(state.theme.value.name).toBe("编辑中");});
it("does not open resource pickers during settings save",async()=>{vi.mocked(isTauri).mockReturnValue(true);const {state}=await editor(true);await state.importTheme();await state.exportTheme();await state.chooseSound("notification");await state.chooseWallpapers();expect(open).not.toHaveBeenCalled();expect(save).not.toHaveBeenCalled();});
it("merges sound-pack slots without clearing other sounds",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/sounds.zip");vi.mocked(invoke).mockResolvedValue({stageId:crypto.randomUUID(),sounds:{enabled:true,volume:40,sourcePaths:{connected:"C:/new.wav"}}});const {state}=await editor();state.theme.value.sounds.sourcePaths={notification:"C:/old.wav"};await state.importSounds();expect(state.theme.value.sounds.sourcePaths).toEqual({notification:"C:/old.wav",connected:"C:/new.wav"});expect(state.theme.value.sounds.volume).toBe(40);});
it("keeps all edits when theme validation fails",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/bad.zip");vi.mocked(invoke).mockRejectedValue(Error("Invalid image"));const {state}=await editor();state.theme.value.transparency=40;await state.importTheme();expect(state.error.value).toBe("Invalid image");expect(state.theme.value.transparency).toBe(40);expect(state.draft.value.themes).toHaveLength(1);});
it("discards late import stages instead of updating a closed editor",async()=>{vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/theme.zip");let complete!:(value:unknown)=>void;vi.mocked(invoke).mockReturnValueOnce(new Promise(resolve=>complete=resolve));const {state,exposed}=await editor();const importing=state.importTheme();await Promise.resolve();await Promise.resolve();await exposed.discardImports();const stageId=crypto.randomUUID();vi.mocked(invoke).mockResolvedValue(undefined);complete({stageId,theme:createTheme("late")});await importing;expect(state.draft.value.themes).toHaveLength(1);expect(invoke).toHaveBeenCalledWith("discard_theme_imports",{stageIds:[stageId]});});
it("accepts drops only within the visible gallery bounds",async()=>{const {state,exposed}=await editor();state.gallery.value={contains:({x,y}:{x:number;y:number})=>x>=10&&x<=110&&y>=20&&y<=120};expect(await exposed.acceptFileDrop([], {x:12,y:22})).toBe(true);expect(await exposed.acceptFileDrop([], {x:9,y:22})).toBe(false);state.setMode("solid");expect(await exposed.acceptFileDrop([], {x:12,y:22})).toBe(false);});
it("previews only chosen imported parts without inheriting the current theme",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/kurisu.zip");
  const imported=createTheme("牧濑红莉栖");Object.assign(imported,{colorTheme:"custom",customAccent:"#B83E49",colorMode:"dark",iconSourcePath:"new-icon.png",wallpaperSourcePaths:["new.jpg"],sounds:{enabled:true,volume:45,sourcePaths:{notification:"new.wav"}}});
  vi.mocked(invoke).mockResolvedValue({stageId:crypto.randomUUID(),theme:imported});
  const {state}=await editor();Object.assign(state.theme.value,{iconSourcePath:"old-icon.png",wallpaperSourcePaths:["old.jpg"],sounds:{enabled:true,volume:60,sourcePaths:{notification:"old.wav"}}});
  await state.importTheme();expect(state.draft.value.themes).toHaveLength(1);expect(state.pendingImport.value.theme.name).toBe("牧濑红莉栖");
  state.importParts.value={colors:true,background:false,icon:false,sounds:false};state.confirmImport();
  expect(state.theme.value.customAccent).toBe("#B83E49");expect(state.theme.value.wallpaperSourcePaths).toEqual([]);
  expect(state.theme.value.iconSourcePath).toBeUndefined();expect(state.theme.value.sounds).toEqual({enabled:false,volume:60,sourcePaths:{}});
  expect(state.draft.value.themes[0].wallpaperSourcePaths).toEqual(["old.jpg"]);
});
it("cancels import choices without adding a theme",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(open).mockResolvedValue("C:/theme.zip");
  vi.mocked(invoke).mockResolvedValue({stageId:crypto.randomUUID(),theme:createTheme("导入")});
  const {state}=await editor();await state.importTheme();await state.cancelImport();
  expect(state.pendingImport.value).toBeUndefined();expect(state.draft.value.themes.map((item:any)=>item.name)).toEqual(["牧濑红莉栖"]);
});
it("exports an unselected card without switching the active theme",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(save).mockResolvedValue("C:/other.zip");vi.mocked(invoke).mockResolvedValue("C:/other.zip");
  const initial=draft("custom");const other=createTheme("另一个主题");other.customAccent="#B83E49";initial.themes.push(other);
  const {state}=await editor(false,initial);await state.exportTheme(other.id);
  expect(save).toHaveBeenCalledWith(expect.objectContaining({defaultPath:"另一个主题.zip"}));
  expect(invoke).toHaveBeenCalledWith("export_theme_package",expect.objectContaining({theme:expect.objectContaining({id:other.id,name:"另一个主题"})}));
  expect(state.draft.value.selectedThemeId).toBe(initial.selectedThemeId);
});
it("exports a valid card independently of invalid unselected solid colors",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(save).mockResolvedValue("C:/valid.zip");vi.mocked(invoke).mockResolvedValue("C:/valid.zip");
  const initial=draft("custom");initial.solid={colorTheme:"custom",customAccent:"#bad",colorMode:"system"};
  const {state}=await editor(false,initial);await state.exportTheme();
  expect(save).toHaveBeenCalledTimes(1);expect(state.error.value).toBe("");
});
it("confirms draft deletion and falls back to solid after the last theme",async()=>{
  const initial=draft("custom");const {state}=await editor(false,initial);const id=state.theme.value.id;
  state.requestRemoveTheme(id);expect(state.draft.value.themes).toHaveLength(1);state.cancelRemoveTheme();expect(state.draft.value.themes).toHaveLength(1);
  state.requestRemoveTheme(id);state.confirmRemoveTheme();expect(state.draft.value.themes).toEqual([]);
  expect(state.draft.value.mode).toBe("solid");expect(state.draft.value.selectedThemeId).toBeNull();expect(initial.themes).toHaveLength(1);expect(invoke).not.toHaveBeenCalled();
});
it("deleting an inactive card preserves the active theme",async()=>{
  const initial=draft("custom");const other=createTheme("另一个主题");initial.themes.push(other);const {state}=await editor(false,initial);
  state.requestRemoveTheme(other.id);state.confirmRemoveTheme();expect(state.draft.value.selectedThemeId).toBe(initial.selectedThemeId);expect(state.draft.value.themes).toHaveLength(1);
});
it("opens import choices only for ZIP dropped onto the theme library",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockResolvedValue({stageId:crypto.randomUUID(),theme:createTheme("牧濑红莉栖")});
  const {state,exposed}=await editor();state.themeLibrary.value={getBoundingClientRect:()=>({left:200,right:400,top:20,bottom:160})};
  expect(await exposed.acceptFileDrop(["C:/kurisu.zip"],{x:199,y:40})).toBe(false);
  expect(await exposed.acceptFileDrop(["C:/kurisu.zip"],{x:220,y:40})).toBe(true);expect(state.pendingImport.value.theme.name).toBe("牧濑红莉栖");expect(state.draft.value.themes).toHaveLength(1);
});
it("renders card actions and a click-or-drop import tile instead of a bottom toolbar",async()=>{
  const html=await renderToString(createSSRApp(PersonalizationSettings,{modelValue:draft("custom"),busy:false}));
  expect(html).toContain('aria-label="导出主题 牧濑红莉栖"');expect(html).toContain('aria-label="删除主题 牧濑红莉栖"');
  expect(html).toContain("点击选择或拖入主题包");expect(html).not.toContain("导出所选主题");expect(html).not.toContain("导入主题</button>");
});
it("clears obsolete thumbnail previews and reuses unchanged thumbnails",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockResolvedValue("thumbnail");
  const {state}=await editor();const id=state.theme.value.id;
  await state.refreshThemeThumbnails([[id,"thumb.png"]]);expect(state.themeThumbnails.value[id]).toBe("thumbnail");
  vi.mocked(invoke).mockClear();await state.refreshThemeThumbnails([[id,"thumb.png"]]);expect(invoke).not.toHaveBeenCalled();
  await state.refreshThemeThumbnails([[id,undefined]]);expect(state.themeThumbnails.value[id]).toBeUndefined();
  vi.mocked(invoke).mockRejectedValue(Error("broken"));await state.refreshThemeThumbnails([[id,"broken.png"]]);expect(state.themeThumbnails.value[id]).toBeUndefined();
});
it("loads a ready card while another thumbnail is still decoding",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);let finish!:(url:string)=>void;
  vi.mocked(invoke).mockImplementation(async(_command,args:any)=>args.path==="slow-card.png"?await new Promise<string>(resolve=>{finish=resolve;}):"ready-thumbnail");
  const {state}=await editor();const loading=state.refreshThemeThumbnails([["slow","slow-card.png"],["ready","ready-card.png"]]);
  await Promise.resolve();await Promise.resolve();
  expect(state.themeThumbnails.value.ready).toBe("ready-thumbnail");
  finish("slow-thumbnail");await loading;expect(state.themeThumbnails.value.slow).toBe("slow-thumbnail");
});
it("does not start obsolete later batches after the selected wallpaper changes",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);let finish!:(url:string)=>void;
  vi.mocked(invoke).mockImplementation(async(_command,args:any)=>args.path==="batch-slow.png"?await new Promise<string>(resolve=>{finish=resolve;}):args.path);
  const {state}=await editor();
  const unchanged:[string,string][]=[["one","batch-slow.png"],["two","batch-2.png"],["three","batch-3.png"],["four","batch-4.png"]];
  const old=state.refreshThemeThumbnails([...unchanged,["five","batch-old.png"]]);
  await state.refreshThemeThumbnails([...unchanged,["five","batch-new.png"]]);
  expect(state.themeThumbnails.value.five).toBe("batch-new.png");
  finish("first-thumbnail");await old;
  expect(state.themeThumbnails.value.five).toBe("batch-new.png");
});
it("continues loading later cards when a slow image occupies only one loading slot",async()=>{
  vi.mocked(isTauri).mockReturnValue(true);let finish!:(url:string)=>void;
  vi.mocked(invoke).mockImplementation(async(_command,args:any)=>args.path==="slot-slow.png"?await new Promise<string>(resolve=>{finish=resolve;}):args.path);
  const {state}=await editor();const loading=state.refreshThemeThumbnails([["one","slot-slow.png"],["two","slot-2.png"],["three","slot-3.png"],["four","slot-4.png"],["five","slot-5.png"]]);
  for(let index=0;index<8;index++)await Promise.resolve();
  expect(state.themeThumbnails.value.five).toBe("slot-5.png");
  finish("slow-thumbnail");await loading;
});
it("shows named themes, sound import, and centered real workspace preview",async()=>{
  const html=await renderToString(createSSRApp(PersonalizationSettings,{modelValue:draft("custom"),busy:false}));
  expect(html).toContain('data-testid="theme-library"');expect(html).toContain("牧濑红莉栖");expect(html).toContain("导入声音包");expect(html).toContain("personalization-preview");
});
