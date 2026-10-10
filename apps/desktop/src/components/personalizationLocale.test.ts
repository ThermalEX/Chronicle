import {readFileSync} from "node:fs";
import {afterEach,expect,it,vi} from "vitest";
vi.mock("@tauri-apps/api/core",()=>({isTauri:()=>false,invoke:vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn(),save:vi.fn()}));
import {createSSRApp} from "vue";import {renderToString} from "@vue/server-renderer";
import {t,setLocale} from "../services/i18n";
import {createTheme,personalizationErrorLabel,type PersonalizationDraft} from "../services/personalization";
import PersonalizationSettings from "./PersonalizationSettings.vue";import WallpaperGallery from "./WallpaperGallery.vue";
afterEach(()=>setLocale("zh-CN"));
it("translates all static personalization labels without changing user names",async()=>{
  setLocale("en");expect(t("个性化")).toBe("Personalization");expect(t("启动随机")).toBe("Random on startup");
  const theme=createTheme("牧濑红莉栖");const draft:PersonalizationDraft={formatVersion:1,mode:"custom",solid:{colorTheme:"teal",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]};
  const html=await renderToString(createSSRApp(PersonalizationSettings,{modelValue:draft,busy:false}));expect(html).toContain("牧濑红莉栖");expect(html).toContain("Import sound pack");expect(html).not.toContain("新增主题");
  const gallery=await renderToString(createSSRApp(WallpaperGallery,{paths:["C:/红莉栖.png"],selectedIndex:0}));expect(gallery).toContain('aria-label="Remove image 红莉栖.png"');
});
it("has English mappings for every new static UI and validation key",()=>{
  setLocale("en");for(const path of ["PersonalizationSettings.vue","WallpaperGallery.vue","../services/personalization.ts"]){const source=readFileSync(new URL(path,import.meta.url),"utf8");for(const match of source.matchAll(/\bt\(["']([^"']+)["']/g)){const key=match[1]!;if(/[\u4e00-\u9fff]/.test(key))expect(t(key),key).not.toBe(key);}}
});
it("localizes owned error messages but preserves external diagnostics",()=>{setLocale("en");expect(personalizationErrorLabel("主题资源不存在")).toBe("A theme resource is missing.");expect(personalizationErrorLabel("Disk full: C:/主题.zip")).toBe("Disk full: C:/主题.zip");});
