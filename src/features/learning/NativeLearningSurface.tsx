import {useLayoutEffect,useRef} from "react";
import {learningApi} from "./api";
import type {PlaybackSnapshot,NativeViewport} from "./types";
import type {VideoPointer} from "./playerInteraction";
import {hasWorkspaceOverlay} from "../../lib/workspace";

export const hasLearningOverlay=hasWorkspaceOverlay;

export function NativeLearningSurface({snapshot,hidden,onBlockedChange,onPointer,onError}:{snapshot:PlaybackSnapshot;hidden:boolean;onBlockedChange:(value:boolean)=>void;onPointer:(event:VideoPointer)=>void;onError:(error:string)=>void}){
  const surface=useRef<HTMLDivElement>(null);
  const queue=useRef(Promise.resolve());
  const callbacks=useRef({onBlockedChange,onError});callbacks.current={onBlockedChange,onError};
  useLayoutEffect(()=>{
    let active=true,frame=0,last="";
    const update=()=>{
      frame=0;if(!active||!surface.current)return;
      const blocked=hidden||hasLearningOverlay()||document.hidden;callbacks.current.onBlockedChange(blocked);
      const rect=surface.current.getBoundingClientRect();
      const viewport:NativeViewport={left:rect.left,top:rect.top,width:rect.width,height:rect.height,visible:!blocked&&snapshot.ready&&!snapshot.error&&rect.width>0&&rect.height>0};
      const key=JSON.stringify(viewport);if(key===last)return;last=key;
      queue.current=queue.current.then(()=>active?learningApi.setViewport(snapshot.libraryId,snapshot.sessionId,viewport):undefined).catch(error=>{if(active)callbacks.current.onError(String(error));});
    };
    const schedule=()=>{if(!frame)frame=requestAnimationFrame(update);};
    const resize=new ResizeObserver(schedule);if(surface.current)resize.observe(surface.current);
    const mutation=new MutationObserver(schedule);mutation.observe(document.body,{childList:true,subtree:true,attributes:true,attributeFilter:["class","style","hidden","aria-hidden","aria-modal"]});
    window.addEventListener("resize",schedule);window.addEventListener("scroll",schedule,true);document.addEventListener("visibilitychange",schedule);
    update();
    return()=>{active=false;cancelAnimationFrame(frame);resize.disconnect();mutation.disconnect();window.removeEventListener("resize",schedule);window.removeEventListener("scroll",schedule,true);document.removeEventListener("visibilitychange",schedule);queue.current=queue.current.then(()=>learningApi.setViewport(snapshot.libraryId,snapshot.sessionId,{left:0,top:0,width:0,height:0,visible:false})).catch(()=>undefined);};
  },[snapshot.libraryId,snapshot.sessionId,snapshot.ready,snapshot.error,hidden]);
  return <div ref={surface} className="learning-native-surface" aria-label="视频画面" onClick={()=>onPointer({kind:"click",inside:true})} onDoubleClick={()=>onPointer({kind:"doubleClick",inside:true})} onWheel={event=>{event.preventDefault();onPointer({kind:"wheel",inside:true,deltaY:event.deltaY});}} onContextMenu={event=>{event.preventDefault();onPointer({kind:"context",inside:true});}}/>;
}
