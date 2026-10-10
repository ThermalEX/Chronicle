import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {createWallpaperPlayer,type WallpaperFrame} from "./wallpaperPlayer";
import {createTheme} from "./personalization";
beforeEach(()=>vi.useFakeTimers());afterEach(()=>vi.useRealTimers());
function setup(mode:"fixed"|"intervalRandom"|"startupRandom"="fixed",paths=["a","b","c"]) {
  const theme=createTheme("测试");theme.wallpaperPlayback=mode;theme.wallpaperSourcePaths=paths;
  const frames:WallpaperFrame[]=[];const warnings:string[]=[];
  const player=createWallpaperPlayer({load:async(path)=>`image:${path}`,random:()=>0.5,onFrame:frame=>frames.push(frame),onWarning:message=>warnings.push(message)});
  return {theme,player,frames,warnings};
}
it("changes at custom intervals but not before, then cleans timers",async()=>{
  const {theme,player,frames}=setup("intervalRandom");player.configure(theme);await vi.advanceTimersByTimeAsync(0);
  expect(frames.at(-1)?.current).toBe("image:a");await vi.advanceTimersByTimeAsync(59999);expect(frames.at(-1)?.current).toBe("image:a");
  await vi.advanceTimersByTimeAsync(1);expect(frames.at(-1)).toEqual({previous:"image:a",current:"image:c",transitionMs:700});
  await vi.advanceTimersByTimeAsync(700);expect(frames.at(-1)?.previous).toBeUndefined();player.dispose();expect(vi.getTimerCount()).toBe(0);
});
it("fixed and single-image playback never schedule rotation",async()=>{
  for(const mode of ["fixed","intervalRandom"] as const){const {theme,player,frames}=setup(mode,["a"]);player.configure(theme);await vi.advanceTimersByTimeAsync(0);
    expect(vi.getTimerCount()).toBe(0);await vi.advanceTimersByTimeAsync(120000);expect(frames.at(-1)?.current).toBe("image:a");player.dispose();}
});
it("startup selection happens once despite pause, save, or reconfiguration",async()=>{
  const {theme,player,frames}=setup("startupRandom");player.configure(theme);await vi.advanceTimersByTimeAsync(0);const chosen=frames.at(-1)?.current;
  player.setPaused(true);player.setPaused(false);player.configure({...theme,colorMode:"dark"});await vi.advanceTimersByTimeAsync(120000);
  expect(frames.at(-1)?.current).toBe(chosen);expect(vi.getTimerCount()).toBe(0);player.dispose();
});
it.each([
  {selected:0,random:0.1,expected:"image:a"},
  {selected:0,random:0.9,expected:"image:b"},
  {selected:1,random:0.1,expected:"image:a"},
  {selected:1,random:0.9,expected:"image:b"},
])("startup random can choose either image independently of the saved selection ($selected, $random)",async({selected,random,expected})=>{
  const theme=createTheme("启动随机");theme.wallpaperPlayback="startupRandom";theme.wallpaperSourcePaths=["a","b"];theme.selectedWallpaperIndex=selected;
  const frames:WallpaperFrame[]=[];
  const player=createWallpaperPlayer({load:async path=>`image:${path}`,random:()=>random,onFrame:frame=>frames.push(frame),onWarning:()=>{}});
  player.configure(theme);await vi.advanceTimersByTimeAsync(0);
  expect(frames.at(-1)?.current).toBe(expected);player.dispose();
});
it("late image loads cannot replace the newest selection",async()=>{
  const pending=new Map<string,(url:string)=>void>();const frames:WallpaperFrame[]=[];const theme=createTheme("竞态");theme.wallpaperSourcePaths=["a","b","c"];
  const player=createWallpaperPlayer({load:path=>new Promise(resolve=>pending.set(path,resolve)),random:()=>0,onFrame:frame=>frames.push(frame),onWarning:()=>{}});
  player.configure(theme);pending.get("a")!("a");await vi.advanceTimersByTimeAsync(0);
  const b=player.select(1);const c=player.select(2);pending.get("c")!("c");await c;pending.get("b")!("b");await b;
  expect(frames.at(-1)?.current).toBe("c");player.dispose();
});
it("broken image keeps the readable previous image and reports it",async()=>{
  const frames:WallpaperFrame[]=[];const warnings:string[]=[];const theme=createTheme("坏图");theme.wallpaperSourcePaths=["a","bad"];
  const player=createWallpaperPlayer({load:async(path)=>{if(path==="bad")throw new Error("broken");return path;},random:()=>0,onFrame:f=>frames.push(f),onWarning:w=>warnings.push(w)});
  player.configure(theme);await vi.advanceTimersByTimeAsync(0);await player.select(1);expect(frames.at(-1)?.current).toBe("a");expect(warnings[0]).toContain("bad");player.dispose();
});
it("preloads a readable fallback without changing stored selections",async()=>{
  const theme=createTheme("fallback");theme.wallpaperSourcePaths=["bad","good"];
  const original=JSON.stringify(theme);const frames:WallpaperFrame[]=[];
  const player=createWallpaperPlayer({load:async(path)=>{if(path==="bad")throw new Error("bad");return path;},random:()=>0,onFrame:f=>frames.push(f),onWarning:()=>{}});
  player.configure(theme);await vi.advanceTimersByTimeAsync(0);expect(frames.at(-1)?.current).toBe("good");expect(JSON.stringify(theme)).toBe(original);player.dispose();
});
it("paused playback does not replay accumulated intervals",async()=>{
  const {theme,player,frames}=setup("intervalRandom");theme.intervalSeconds=1;player.configure(theme);await vi.advanceTimersByTimeAsync(0);
  player.setPaused(true);await vi.advanceTimersByTimeAsync(10000);expect(frames.at(-1)?.current).toBe("image:a");
  player.setPaused(false);await vi.advanceTimersByTimeAsync(999);expect(frames.at(-1)?.current).toBe("image:a");await vi.advanceTimersByTimeAsync(1);expect(frames.at(-1)?.current).not.toBe("image:a");player.dispose();
});
it("reduced motion switches without fading",async()=>{
  vi.stubGlobal("window",{matchMedia:()=>({matches:true})});
  const {theme,player,frames}=setup();player.configure(theme);await vi.advanceTimersByTimeAsync(0);await player.select(1);expect(frames.at(-1)?.transitionMs).toBe(0);player.dispose();vi.unstubAllGlobals();
});
it("tries every fallback and returns to a solid background when none are readable",async()=>{
  const theme=createTheme("fallbacks");theme.wallpaperSourcePaths=["bad1","bad2","good"];
  const frames:WallpaperFrame[]=[];
  const player=createWallpaperPlayer({load:async(path)=>{if(path!=="good")throw new Error("bad");return path;},random:()=>0,onFrame:f=>frames.push(f),onWarning:()=>{}});
  player.configure(theme);await vi.advanceTimersByTimeAsync(0);expect(frames.at(-1)?.current).toBe("good");
  player.configure({...theme,wallpaperSourcePaths:["bad1","bad2"]});await vi.advanceTimersByTimeAsync(0);expect(frames.at(-1)?.current).toBeUndefined();player.dispose();
});
it("releases previous images only after the fade and current images on disposal",async()=>{
  const theme=createTheme("ownership");theme.wallpaperSourcePaths=["a","b"];const release=vi.fn();
  const player=createWallpaperPlayer({load:async path=>path,release,random:()=>0,onFrame:()=>{},onWarning:()=>{}});
  player.configure(theme);await vi.advanceTimersByTimeAsync(0);await player.select(1);expect(release).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(700);expect(release).toHaveBeenCalledWith("a");
  player.dispose();expect(release).toHaveBeenCalledWith("b");expect(release).toHaveBeenCalledTimes(2);
});
it("releases stale loads without changing the current image",async()=>{
  const theme=createTheme("late ownership");theme.wallpaperSourcePaths=["a"];const release=vi.fn();let finish!:(url:string)=>void;
  const player=createWallpaperPlayer({load:()=>new Promise(resolve=>finish=resolve),release,random:()=>0,onFrame:()=>{},onWarning:()=>{}});
  player.configure(theme);player.dispose();finish("late");await vi.advanceTimersByTimeAsync(0);
  expect(release).toHaveBeenCalledWith("late");
});
