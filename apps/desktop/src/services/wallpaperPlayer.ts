import type {ThemeDraft} from "./personalization";
export interface WallpaperFrame {previous?:string;current?:string;transitionMs:number}
export interface WallpaperPlayer {configure(theme?:ThemeDraft):void;select(index:number):Promise<void>;setPaused(paused:boolean):void;dispose():void}
export interface WallpaperPlayerDeps {load:(path:string)=>Promise<string>;release?:(url:string)=>void;random:()=>number;onFrame:(frame:WallpaperFrame)=>void;onWarning:(message:string)=>void}
const startupChoices=new Map<string,string>();
export function createWallpaperPlayer(deps:WallpaperPlayerDeps):WallpaperPlayer {
  let theme:ThemeDraft|undefined;let signature="";let generation=0;let index=0;let current:string|undefined;
  let fadingPrevious:string|undefined;
  let paused=false;let disposed=false;let rotation:ReturnType<typeof setTimeout>|undefined;let fade:ReturnType<typeof setTimeout>|undefined;
  const failed=new Set<string>();
  const clearRotation=()=>{clearTimeout(rotation);rotation=undefined;};
  const emit=(frame:WallpaperFrame)=>{if(!disposed)deps.onFrame(frame);};
  const release=(url?:string)=>{if(url)deps.release?.(url);};
  const clearImages=()=>{release(current);release(fadingPrevious);current=undefined;fadingPrevious=undefined;};
  function nextIndex():number {
    const candidates=theme!.wallpaperSourcePaths.map((_,i)=>i).filter(i=>i!==index&&!failed.has(theme!.wallpaperSourcePaths[i]!));
    return candidates.length?candidates[Math.min(candidates.length-1,Math.floor(deps.random()*candidates.length))]!:index;
  }
  function schedule():void {
    clearRotation();
    if(disposed||paused||theme?.wallpaperPlayback!=="intervalRandom"||theme.wallpaperSourcePaths.length<2
      ||!Number.isInteger(theme.intervalSeconds)||theme.intervalSeconds<1||theme.intervalSeconds>86400)return;
    rotation=setTimeout(()=>{rotation=undefined;void select(nextIndex()).finally(schedule);},theme.intervalSeconds*1000);
  }
  async function select(next:number):Promise<void> {
    const path=theme?.wallpaperSourcePaths[next];if(disposed||!path)return;
    const request=++generation;
    try {
      const url=await deps.load(path);if(disposed||request!==generation){release(url);return;}
      release(fadingPrevious);fadingPrevious=undefined;
      const previous=current;index=next;current=url;failed.delete(path);clearTimeout(fade);
      const reduced=typeof window!=="undefined"&&window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
      const transitionMs=previous&&previous!==url&&!reduced?700:0;
      if(transitionMs)fadingPrevious=previous;else release(previous);
      emit({...(transitionMs?{previous}:{}),current,transitionMs});
      if(transitionMs)fade=setTimeout(()=>{fade=undefined;release(fadingPrevious);fadingPrevious=undefined;if(current===url)emit({current,transitionMs:0});},transitionMs);
    }catch(error){if(request===generation&&!disposed){failed.add(path);deps.onWarning(`${path}: ${String(error)}`);}}
  }
  function configure(next?:ThemeDraft):void {
    const nextSignature=JSON.stringify(next&&[next.id,next.wallpaperSourcePaths,next.selectedWallpaperIndex,next.wallpaperPlayback,next.intervalSeconds]);
    if(disposed||nextSignature===signature)return;
    signature=nextSignature;generation++;clearRotation();clearTimeout(fade);failed.clear();
    theme=next?JSON.parse(JSON.stringify(next)) as ThemeDraft:undefined;
    if(!theme?.wallpaperSourcePaths.length){clearImages();emit({transitionMs:0});return;}
    index=theme.selectedWallpaperIndex;
    if(theme.wallpaperPlayback==="startupRandom"){
      const chosen=startupChoices.get(theme.id);
      if(chosen){const found=theme.wallpaperSourcePaths.indexOf(chosen);if(found>=0)index=found;}
      else {index=Math.min(theme.wallpaperSourcePaths.length-1,Math.floor(deps.random()*theme.wallpaperSourcePaths.length));startupChoices.set(theme.id,theme.wallpaperSourcePaths[index]!);}
    }
    const candidates=[index,...theme.wallpaperSourcePaths.map((_,i)=>i).filter(i=>i!==index)];
    void (async()=>{
      let readable=false;
      for(const attempt of candidates){
        if(disposed||signature!==nextSignature)return;
        const path=theme!.wallpaperSourcePaths[attempt]!;const expected=generation+1;
        await select(attempt);
        if(disposed||generation!==expected)return;
        if(!failed.has(path)){readable=true;break;}
      }
      if(!readable){clearImages();emit({transitionMs:0});}
      if(!disposed&&signature===nextSignature)schedule();
    })();
  }
  return {configure,select,setPaused(value){if(paused===value||disposed)return;paused=value;clearRotation();if(!paused)schedule();},
    dispose(){disposed=true;generation++;clearRotation();clearTimeout(fade);clearImages();}};
}
