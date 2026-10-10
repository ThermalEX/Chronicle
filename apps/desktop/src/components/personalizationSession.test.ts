import "fake-indexeddb/auto";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
vi.mock("@tauri-apps/api/core",()=>({isTauri:()=>false,invoke:vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn(),save:vi.fn()}));
import {createSSRApp} from "vue";
import {renderToString} from "@vue/server-renderer";
import SettingsDialog from "./SettingsDialog.vue";
import {savedPersonalization,createTheme} from "../services/personalization";
import {resetAppSettings} from "../services/settings";
import {setLocale} from "../services/i18n";
beforeEach(()=>{vi.stubGlobal("localStorage",{getItem:()=>null,setItem:vi.fn(),removeItem:vi.fn()});resetAppSettings();setLocale("zh-CN");const theme=createTheme("牧濑红莉栖");savedPersonalization.value={draft:{formatVersion:1,mode:"custom",solid:{colorTheme:"teal",colorMode:"system"},selectedThemeId:theme.id,themes:[theme]},warnings:[]};});
afterEach(()=>vi.unstubAllGlobals());
async function dialog(){let state:any;let closed=0;const component={...SettingsDialog,setup(p:unknown,c:unknown){state=(SettingsDialog as any).setup(p,c);return state;}};const html=await renderToString(createSSRApp(component,{onClose:()=>closed++}));return {state,html,closed:()=>closed};}
it("has a personalization tab and keeps its draft alive when another tab is selected",async()=>{const {state,html}=await dialog();expect(html).toContain("个性化");expect(state.personalizationDraft.value.themes[0].name).toBe("牧濑红莉栖");state.activeSection.value="personalization";state.personalizationDraft.value.themes[0].name="编辑中";state.activeSection.value="software";expect(state.personalizationDraft.value.themes[0].name).toBe("编辑中");expect(savedPersonalization.value.draft.themes[0].name).toBe("牧濑红莉栖");});
it("asks before discarding theme changes and saves the selected draft using the common footer",async()=>{const {state,closed}=await dialog();state.personalizationDraft.value.themes[0].name="新名称";state.requestClose();expect(state.closeConfirmOpen.value).toBe(true);await state.save();expect(savedPersonalization.value.draft.themes[0].name).toBe("新名称");expect(closed()).toBe(1);});
it("keeps the dialog open when a theme draft is invalid",async()=>{const {state,closed}=await dialog();state.personalizationDraft.value.themes[0].intervalSeconds=0;await state.save();expect(state.saveError.value).toContain("1–86400");expect(closed()).toBe(0);});
