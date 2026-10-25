import {useCallback,useEffect,useRef,useState} from "react";
import {listen} from "@tauri-apps/api/event";
import {ArrowLeft,Keyboard,Captions,ListVideo,Sparkles} from "lucide-react";
import {WorkspacePanel} from "../../components/WorkspacePanel";
import {api} from "../../lib/api";
import {normalizePanel,persistPanels,setLayoutPanel,togglePanelFocus,type PanelLayout} from "../../lib/workspacePanels";
import {ContextMenu} from "../../components/ContextMenu";
import {LoadFeedback} from "../../components/LoadFeedback";
import {flushWorkspaceEdits,registerWorkspaceLeaveGuard} from "../../lib/noteSaveGuard";
import {useRequestGate} from "../../lib/workspace";
import {shortcutMatches} from "../../lib/personalization";
import {learningApi} from "./api";
import {defaultLearningBindings,learningBindingLabels,nativeBindings} from "./nativeBindings";
import {classifyVideoPointer,type VideoPointer} from "./playerInteraction";
import {NativeLearningSurface,hasLearningOverlay} from "./NativeLearningSurface";
import {LearningControls} from "./LearningControls";
import {TranscriptPanel} from "./TranscriptPanel";
import {LearningSeries} from "./LearningSeries";
import {CourseAnalysisPanel} from "./CourseAnalysisPanel";
import type {PlaybackAction,PlaybackSnapshot,ScopedPlaybackEvent,NativePointerEvent} from "./types";
import "./LearningWorkspace.css";

export interface LearningWorkspaceProps {libraryId:string;courseId:string;projectId?:string;initialPositionMs?:number;sourceFingerprint?:string;backLabel?:string;onBack:()=>void;notify:(message:string,error?:boolean)=>void}
const seriesLimits={min:220,max:440,defaultWidth:300},detailLimits={min:260,max:560,defaultWidth:340};
const initialPanels=():PanelLayout=>({left:{mode:"closed",width:300,tab:"series"},right:{mode:"closed",width:340,tab:"transcript"},focusSnapshot:null});
function restoredPanels(raw:unknown):PanelLayout {
  const value=(raw&&typeof raw==="object"?raw:{}) as Record<string,unknown>;
  const fallback=initialPanels();
  const panels={left:normalizePanel(value.left,fallback.left,seriesLimits,["series"]),right:normalizePanel(value.right,fallback.right,detailLimits,["transcript","analysis"]),focusSnapshot:null};
  return {...persistPanels(panels),focusSnapshot:null};
}
export function LearningWorkspace({libraryId,courseId:initialCourseId,projectId,initialPositionMs,sourceFingerprint,backLabel,onBack,notify}:LearningWorkspaceProps){
  const [courseId,setCourseId]=useState(initialCourseId),[title,setTitle]=useState("学习视频"),[snapshot,setSnapshot]=useState<PlaybackSnapshot|null>(null),[error,setError]=useState(""),[retry,setRetry]=useState(0),[leaving,setLeaving]=useState(false);
  const [focus,setFocus]=useState<"learning"|"editor">("learning"),[blocked,setBlocked]=useState(false),[composing,setComposing]=useState(false),[menu,setMenu]=useState<{x:number;y:number}|null>(null),[help,setHelp]=useState(false);
  const [panels,setPanels]=useState(initialPanels),[panelReady,setPanelReady]=useState(false),[panelError,setPanelError]=useState(""),[panelRead,setPanelRead]=useState(0);
  const panelRef=useRef(panels);panelRef.current=panels;
  const panelDirty=useRef(false),panelWrites=useRef(Promise.resolve());
  const panelScope=useRef<{libraryId:string;enabled:boolean;value:Record<string,unknown>}|null>(null);
  const changePanels=(change:(value:PanelLayout)=>PanelLayout)=>{panelDirty.current=true;setPanels(change);};
  useEffect(()=>{panelDirty.current=false;panelScope.current=null;setPanelReady(false);setPanelError("");setPanels(initialPanels());},[libraryId]);
  useEffect(()=>{
    let active=true;setPanelReady(false);setPanelError("");panelScope.current=null;
    void api.getWorkspaceContext("learning").then(context=>{
      if(!active)return;
      if(context.libraryId!==libraryId)throw new Error("素材库已切换，未恢复其他库的布局");
      const value=(context.value&&typeof context.value==="object"?context.value:{}) as Record<string,unknown>;
      panelScope.current={libraryId,enabled:context.enabled,value};
      if(!panelDirty.current)setPanels(restoredPanels(value.panels));
      setPanelReady(true);
    }).catch(reason=>{if(active)setPanelError(`布局读取失败：${String(reason)}；当前操作保留，尚未覆盖布局设置。`);});
    return()=>{active=false;};
  },[libraryId,panelRead]);
  const savePanels=useCallback((layout:PanelLayout)=>{
    const scope=panelScope.current;if(!scope?.enabled||scope.libraryId!==libraryId)return;
    const value={...scope.value,panels:persistPanels(layout)};
    panelWrites.current=panelWrites.current.then(async()=>{
      if(!await api.saveWorkspaceContext(libraryId,"learning",value))throw new Error("素材库已切换");
      if(panelScope.current===scope)setPanelError("");
    }).catch(reason=>{if(panelScope.current===scope)setPanelError(`布局保存失败：${String(reason)}；可重新读取后重试。`);});
  },[libraryId]);
  useEffect(()=>{if(!panelReady)return;const timer=window.setTimeout(()=>savePanels(panels),500);return()=>window.clearTimeout(timer);},[panels,panelReady,savePanels]);
  useEffect(()=>()=>{savePanels(panelRef.current);panelScope.current=null;},[savePanels]);
  const [completed,setCompleted]=useState(false);
  const seriesOrder=useRef<string[]>([]);
  useEffect(()=>{seriesOrder.current=[];},[libraryId]);
  const root=useRef<HTMLElement>(null),current=useRef(snapshot),busy=useRef(false),stopped=useRef(false),click=useRef<ReturnType<typeof setTimeout>|null>(null),queue=useRef(Promise.resolve());current.current=snapshot;
  const gate=useRequestGate(),adjacentGate=useRequestGate();
  const helpButton=useRef<HTMLButtonElement>(null),helpDialog=useRef<HTMLDivElement>(null);
  const start=useRef<{key:string;promise:Promise<PlaybackSnapshot>}|null>(null);
  const interaction=useRef({blocked,composing,focus});interaction.current={blocked,composing,focus};
  useEffect(()=>setCourseId(initialCourseId),[initialCourseId]);
  const accept=useCallback((value:PlaybackSnapshot)=>{current.current=value;setSnapshot(value);},[]);
  const control=useCallback((action:PlaybackAction|((value:PlaybackSnapshot)=>PlaybackAction))=>{
    const value=current.current;if(!value||!value.ready||stopped.current||busy.current)return;
    const sid=value.sessionId;
    queue.current=queue.current.then(async()=>{
      if(current.current?.sessionId!==sid||stopped.current)return;
      const command=typeof action==="function"?action(current.current):action;
      const next=await learningApi.controlPlayback(libraryId,sid,command);
      if(current.current?.sessionId===sid&&next.libraryId===libraryId&&next.courseId===courseId){accept(next);if(command.type==="completed")setCompleted(command.completed);}
    }).catch(reason=>{if(current.current?.sessionId===sid){setError(String(reason));notify(String(reason),true);}});
  },[libraryId,courseId,accept,notify,gate]);
  const initialSeek=useRef("");
  useEffect(()=>{
    if(!snapshot?.ready||courseId!==initialCourseId||initialPositionMs==null)return;
    const key=`${libraryId}:${courseId}:${snapshot.sessionId}:${initialPositionMs}`;
    if(initialSeek.current===key)return;
    initialSeek.current=key;
    control({type:"pause",paused:true});control({type:"seek",positionMs:initialPositionMs});
  },[snapshot?.ready,snapshot?.sessionId,libraryId,courseId,initialCourseId,initialPositionMs,control]);
  const stop=useCallback(async()=>{
    if(stopped.current)return;
    busy.current=true;setLeaving(true);if(click.current){clearTimeout(click.current);click.current=null;}
    try{
      await queue.current;
      const value=current.current||await start.current?.promise.catch(()=>null);
      if(value){
        if(value.fullscreen)await learningApi.controlPlayback(libraryId,value.sessionId,{type:"fullscreen",enabled:false});
        await learningApi.stopPlayback(libraryId,value.sessionId);
      }
      stopped.current=true;
    }finally{busy.current=false;setLeaving(false);}
  },[libraryId]);
  useEffect(()=>registerWorkspaceLeaveGuard(stop),[stop]);
  const back=async()=>{if(busy.current)return;setError("");try{await flushWorkspaceEdits();onBack();}catch(reason){setError(String(reason));}};
  const adjacent=async(delta:number)=>{
    if(busy.current)return;
    const ticket=adjacentGate.begin();
    try{
      const index=seriesOrder.current.indexOf(courseId);
      if(index>=0){const target=seriesOrder.current[index+delta];if(!target){notify(delta<0?"已经是系列第一课":"已经是系列最后一课");return;}await flushWorkspaceEdits();if(adjacentGate.isCurrent(ticket))setCourseId(target);return;}
      let previous:string|undefined,found=false,next:string|undefined;
      for(let offset=0;!next;){
        const page=await learningApi.listCourses(libraryId,offset,80);
        if(!adjacentGate.isCurrent(ticket))return;
        for(const course of page.items){if(found){next=course.id;break;}if(course.id===courseId){found=true;if(delta<0){next=previous;break;}}previous=course.id;}
        if(next||(delta<0&&found)||!page.items.length||offset+page.items.length>=page.total)break;
        offset+=page.items.length;
      }
      if(!next){notify(delta<0?"已经是第一节课程":"没有下一节课程");return;}
      await flushWorkspaceEdits();if(adjacentGate.isCurrent(ticket))setCourseId(next);
    }catch(reason){if(adjacentGate.isCurrent(ticket))setError(String(reason));}
  };
  const actionRef=useRef<(id:string)=>void>(()=>{});
  actionRef.current=id=>{
    const value=current.current;if(!value?.ready||busy.current)return;
    const seek=(offset:number)=>control(value=>({type:"seek",positionMs:Math.max(0,Math.min(value.positionMs+offset,value.durationMs??Number.MAX_SAFE_INTEGER))}));
    switch(id){
      case "player.toggle":control(value=>({type:"pause",paused:!value.paused}));break;
      case "player.seek-back":case "player.seek-back-arrow":seek(-5000);break;
      case "player.seek-forward":case "player.seek-forward-arrow":seek(5000);break;
      case "player.seek-back-large":seek(-30000);break;case "player.seek-forward-large":seek(30000);break;
      case "player.fullscreen":case "player.fullscreen-legacy":control(value=>({type:"fullscreen",enabled:!value.fullscreen}));break;
      case "player.exit-fullscreen":if(value.fullscreen)control({type:"fullscreen",enabled:false});break;
      case "player.mute":control(value=>({type:"mute",muted:!value.muted}));break;
      case "player.next":void adjacent(1);break;case "player.previous":void adjacent(-1);break;
    }
  };
  const pointer=useCallback((event:VideoPointer,native=false)=>{
    const state=interaction.current;
    const intent=classifyVideoPointer({...event,blocked:state.blocked||hasLearningOverlay(),composing:state.composing},click.current!=null);
    if(intent==="none")return;
    if(native){(document.activeElement as HTMLElement)?.blur?.();root.current?.focus();setFocus("learning");}
    if(intent==="deferClick"){
      click.current=setTimeout(()=>{const pending=click.current!=null;click.current=null;if(classifyVideoPointer({kind:"commitClick",inside:true,blocked:interaction.current.blocked||hasLearningOverlay(),composing:interaction.current.composing},pending)==="togglePause")actionRef.current("player.toggle");},Math.max(350,Math.min(5000,event.doubleClickMs||300)+50));return;
    }
    if(click.current){clearTimeout(click.current);click.current=null;}
    if(intent==="toggleFullscreen")actionRef.current("player.fullscreen");
    else if(intent==="volumeUp"||intent==="volumeDown")control(value=>({type:"volume",volume:Math.max(0,Math.min(100,value.volume+(intent==="volumeUp"?5:-5)))}));
    else if(intent==="contextMenu"){const rect=root.current?.getBoundingClientRect();setMenu({x:(rect?.left||0)+40,y:(rect?.top||0)+100});}
  },[control]);
  const pointerRef=useRef(pointer);pointerRef.current=pointer;
  useEffect(()=>{
    const ticket=gate.begin();const disposes:(()=>void)[]=[];let active=true;stopped.current=false;setError("");setSnapshot(null);current.current=null;setMenu(null);
    const scoped=(value:ScopedPlaybackEvent)=>active&&gate.isCurrent(ticket)&&value.libraryId===libraryId&&value.courseId===courseId&&value.sessionId===current.current?.sessionId;
    const listeners=Promise.all([
      listen<PlaybackSnapshot>("learning-playback-state",event=>{if(scoped(event.payload))accept(event.payload);}),
      listen<ScopedPlaybackEvent&{id:string}>("learning-playback-shortcut",event=>{if(scoped(event.payload)&&interaction.current.focus==="learning"&&!interaction.current.blocked&&!interaction.current.composing&&!hasLearningOverlay())actionRef.current(event.payload.id);}),
      listen<NativePointerEvent>("learning-playback-pointer",event=>{const value=event.payload;if(!scoped(value))return;pointerRef.current({kind:value.kind==="wheelUp"||value.kind==="wheelDown"?"wheel":value.kind,inside:true,doubleClickMs:value.doubleClickMs,deltaY:value.kind==="wheelUp"?-1:1},true);}),
    ].map(promise=>promise.then(dispose=>{if(active)disposes.push(dispose);else dispose();})));
    const key=`${libraryId}:${courseId}:${retry}`;
    if(start.current?.key!==key)start.current={key,promise:listeners.then(()=>sourceFingerprint&&courseId===initialCourseId?learningApi.startPlayback(libraryId,courseId,sourceFingerprint):learningApi.startPlayback(libraryId,courseId))};
    void start.current.promise.then(async value=>{
      if(!active||!gate.isCurrent(ticket)||value.libraryId!==libraryId||value.courseId!==courseId)return;
      accept(value);
      // Boot may emit a one-shot failure before the start reply reaches the webview.
      const confirmed=await learningApi.getPlayback(libraryId,value.sessionId);
      if(scoped(confirmed))accept(confirmed);
    }).catch(reason=>{if(active&&gate.isCurrent(ticket))setError(String(reason));});
    void learningApi.getCourse(libraryId,courseId).then(course=>{if(active&&gate.isCurrent(ticket)){setTitle(course.title);setCompleted(course.completed);}}).catch(reason=>{if(active)setError(String(reason));});
    return()=>{active=false;gate.invalidate();adjacentGate.invalidate();disposes.forEach(dispose=>dispose());if(click.current){clearTimeout(click.current);click.current=null;}};
  },[libraryId,courseId,retry,gate,adjacentGate,accept,sourceFingerprint,initialCourseId]);
  useEffect(()=>{
    const focusChanged=(event:FocusEvent)=>setFocus((event.target as HTMLElement)?.closest("input,textarea,select,[contenteditable=true]")?"editor":"learning");
    const compositionStart=()=>setComposing(true),compositionEnd=()=>setComposing(false);
    const key=(event:KeyboardEvent)=>{
      if(event.defaultPrevented||event.isComposing||interaction.current.composing||interaction.current.blocked||hasLearningOverlay()||(event.target as HTMLElement)?.closest("input,textarea,select,[contenteditable=true]"))return;
      const binding=defaultLearningBindings.find(binding=>shortcutMatches(event,binding.shortcut));if(!binding)return;
      event.preventDefault();event.stopPropagation();if(!event.repeat||!binding.id.includes("fullscreen"))actionRef.current(binding.id);
    };
    window.addEventListener("focusin",focusChanged);window.addEventListener("compositionstart",compositionStart);window.addEventListener("compositionend",compositionEnd);window.addEventListener("keydown",key,true);
    return()=>{window.removeEventListener("focusin",focusChanged);window.removeEventListener("compositionstart",compositionStart);window.removeEventListener("compositionend",compositionEnd);window.removeEventListener("keydown",key,true);};
  },[]);
  useEffect(()=>{if(snapshot?.ready)control({type:"shortcuts",bindings:nativeBindings(defaultLearningBindings,blocked||composing?"dialog":focus)});},[snapshot?.sessionId,snapshot?.ready,focus,blocked,composing,control]);
  useEffect(()=>{
    if(!help)return;
    const close=()=>{setHelp(false);helpButton.current?.focus();};
    const key=(event:KeyboardEvent)=>{if(event.key==="Escape"){event.preventDefault();event.stopPropagation();close();}else if(event.key==="Tab"){event.preventDefault();helpDialog.current?.querySelector<HTMLButtonElement>("button")?.focus();}};
    const outside=(event:PointerEvent)=>{if(!helpDialog.current?.contains(event.target as Node)&&!helpButton.current?.contains(event.target as Node))close();};
    window.addEventListener("keydown",key,true);window.addEventListener("pointerdown",outside,true);
    return()=>{window.removeEventListener("keydown",key,true);window.removeEventListener("pointerdown",outside,true);};
  },[help]);
  useEffect(()=>{if(snapshot?.ready&&!hasLearningOverlay()&&document.activeElement===document.body)root.current?.focus();},[snapshot?.ready]);
  const failure=error||snapshot?.error||snapshot?.saveError;
  return <section ref={root} tabIndex={-1} className="learning-workspace" role="region" aria-label="学习播放器">
    <header className="learning-header"><button className="secondary-button" disabled={leaving} onClick={()=>void back()}><ArrowLeft size={16}/>{backLabel||(projectId?"返回项目":"返回视频详情")}</button><div><span className="eyebrow">LEARNING WORKSPACE</span><h1>{title}</h1></div><button ref={helpButton} className="secondary-button" onClick={()=>setHelp(value=>!value)} aria-expanded={help}><Keyboard size={16}/>快捷键</button></header>
    {failure&&<div className="learning-error" role="alert"><span>{failure}</span><button className="secondary-button" disabled={leaving} onClick={()=>{setError("");setRetry(value=>value+1);}}>重试播放</button></div>}
    {!snapshot&&!failure&&<LoadFeedback label="正在启动原生播放器…" skeleton/>}
    {panelError&&<div className="learning-error" role="alert"><span>{panelError}</span><button className="secondary-button" onClick={()=>setPanelRead(value=>value+1)}>重新读取布局</button></div>}
    <div className="learning-workspace-actions"><button className="secondary-button" aria-pressed={!!panels.focusSnapshot} onClick={()=>changePanels(togglePanelFocus)}>{panels.focusSnapshot?"恢复面板布局":"专注学习"}</button>
    <button className="secondary-button" disabled={!snapshot?.ready||leaving} aria-pressed={completed} onClick={()=>control({type:"completed",completed:!completed})}>{completed?"已完成 · 恢复学习":"标记课程完成"}</button></div>
    <div className="learning-stage-row">
      <WorkspacePanel label="课程导航" side="left" state={panels.left} limits={seriesLimits} onChange={next=>changePanels(value=>setLayoutPanel(value,"left",next))} tabs={[{id:"series",label:"课程系列",icon:<ListVideo size={18}/>,content:<LearningSeries libraryId={libraryId} courseId={courseId} onOrderChange={ids=>{seriesOrder.current=ids;}} onOpen={async id=>{if(busy.current)return;const ticket=adjacentGate.begin();await flushWorkspaceEdits();if(adjacentGate.isCurrent(ticket))setCourseId(id);}}/>}]}/>
      <div className="learning-video-stage">{snapshot&&<NativeLearningSurface snapshot={snapshot} hidden={!!menu||help} onBlockedChange={setBlocked} onPointer={pointer} onError={setError}/>} {snapshot&&!snapshot.ready&&!failure&&<LoadFeedback label="正在校验视频并等待首帧…"/>}</div>
      <WorkspacePanel label="学习资料" side="right" state={panels.right} limits={detailLimits} onChange={next=>changePanels(value=>setLayoutPanel(value,"right",next))} tabs={[
        {id:"transcript",label:"课程字幕",icon:<Captions size={18}/>,content:<TranscriptPanel libraryId={libraryId} courseId={courseId} sessionId={snapshot?.ready?snapshot.sessionId:undefined} onSeek={positionMs=>control({type:"seek",positionMs})}/>},
        {id:"analysis",label:"课程分析",icon:<Sparkles size={18}/>,content:<CourseAnalysisPanel libraryId={libraryId} courseId={courseId} canSeek={!!snapshot?.ready&&!snapshot.error&&!leaving} onSeek={positionMs=>control({type:"seek",positionMs})}/>},
      ]}/>
    </div>
    {snapshot&&<LearningControls key={snapshot.sessionId} snapshot={snapshot} disabled={!snapshot.ready||!!snapshot.error||leaving} onAction={control}/>}
    {help&&<div ref={helpDialog} className="learning-shortcut-help" data-native-overlay role="dialog" aria-modal="true" aria-label="播放器快捷键"><header><h2>播放器快捷键</h2><button className="secondary-button" autoFocus onClick={()=>{setHelp(false);helpButton.current?.focus();}}>关闭</button></header><p>编辑输入、中文输入法和弹窗优先。单击播放/暂停，双击全屏，视频区域滚轮调整音量。</p><dl>{defaultLearningBindings.map(binding=><div key={binding.id}><dt>{learningBindingLabels[binding.id]}</dt><dd><kbd>{binding.shortcut}</kbd></dd></div>)}</dl></div>}
    {menu&&<ContextMenu {...menu} title="播放控制" onClose={()=>setMenu(null)} items={[{label:snapshot?.paused?"播放":"暂停",run:()=>actionRef.current("player.toggle")},{label:"后退 5 秒",run:()=>actionRef.current("player.seek-back")},{label:"前进 5 秒",run:()=>actionRef.current("player.seek-forward")},{label:"切换全屏",run:()=>actionRef.current("player.fullscreen")},{label:"快捷键说明",run:()=>setHelp(true)}]}/>}
  </section>;
}
