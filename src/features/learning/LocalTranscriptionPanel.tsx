import {useCallback,useEffect,useRef,useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {LoadFeedback} from "../../components/LoadFeedback";
import {api} from "../../lib/api";
import {useRequestGate} from "../../lib/workspace";
import {learningApi} from "./api";
import {loadWhisperPrefs} from "./whisperPrefs";
import type {LibraryTask} from "../../types";
import type {WhisperComponentStatus,CourseTranscriptionRequest} from "./types";
export interface LocalTranscriptionProps {libraryId:string;courseId:string;onComplete:(id:string)=>void}
const active=(task:LibraryTask|null)=>!!task&&["queued","running"].includes(task.status);
export function LocalTranscriptionPanel(props:LocalTranscriptionProps){return <LocalContent key={`${props.libraryId}:${props.courseId}`} {...props}/>;}
function LocalContent({libraryId,courseId,onComplete}:LocalTranscriptionProps){
  const initialPrefs=loadWhisperPrefs();
  const [status,setStatus]=useState<WhisperComponentStatus|null>(null),[model,setModel]=useState<CourseTranscriptionRequest["modelId"]>(initialPrefs.defaultModel),[consent,setConsent]=useState(initialPrefs.skipRepeatConsent);
  const [task,setTask]=useState<LibraryTask|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState(""),[pollRevision,setPollRevision]=useState(0);
  const gate=useRequestGate(),busyRef=useRef(false),watched=useRef(new Set<string>()),delivered=useRef(new Set<string>()),complete=useRef(onComplete);complete.current=onComplete;
  const scoped=useCallback((value:LibraryTask)=>value.libraryId===libraryId&&value.payload.kind==="courseTranscription"&&value.payload.request.courseId===courseId,[libraryId,courseId]);
  const action=useCallback(async(job:(ticket:number)=>Promise<void>)=>{
    if(busyRef.current)return;busyRef.current=true;setBusy(true);setError("");const ticket=gate.begin();
    try{await job(ticket);}catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}
    finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}
  },[gate]);
  const refresh=useCallback(async()=>{
    const ticket=gate.begin();setBusy(true);setError("");
    try{
      const [components,tasks]=await Promise.all([learningApi.transcriptionComponents(libraryId),api.listLibraryTasks()]);
      if(!gate.isCurrent(ticket))return;
      setStatus(components);const candidates=tasks.filter(scoped),next=candidates.find(active)||candidates[0]||null;
      if(next&&active(next))watched.current.add(next.id);setTask(next);setPollRevision(value=>value+1);
    }catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}
    finally{if(gate.isCurrent(ticket))setBusy(false);}
  },[libraryId,gate,scoped]);
  useEffect(()=>{void refresh();},[refresh]);
  useEffect(()=>{
    if(!task||!active(task))return;let keep=true;let timer:number;
    const poll=async()=>{
      try{const next=await api.getLibraryTask(task.id);if(!keep)return;if(!scoped(next))throw new Error("任务不属于当前课程");setTask(next);if(active(next))timer=window.setTimeout(()=>void poll(),2000);}
      catch(reason){if(keep)setError(`任务状态读取失败：${String(reason)}。后台仍继续，请重新检查组件与任务。`);}
    };
    timer=window.setTimeout(()=>void poll(),2000);return()=>{keep=false;window.clearTimeout(timer);};
  },[task?.id,task?.status,pollRevision,scoped]);
  const result=task?.result&&typeof task.result==="object"&&"courseId" in task.result&&task.result.courseId===courseId&&"subtitleId" in task.result&&typeof task.result.subtitleId==="string"?task.result:null;
  const resultId=result&&typeof result.subtitleId==="string"?result.subtitleId:null;
  useEffect(()=>{
    if(task?.status==="completed"&&result&&resultId&&watched.current.has(task.id)&&!delivered.current.has(task.id)){
      delivered.current.add(task.id);complete.current(resultId);
      if("warning" in result&&typeof result.warning==="string")setNotice(result.warning);else setNotice("字幕已保存；未自动发起在线分析。");
    }
  },[task,result,resultId]);
  const install=(kind:"tools"|"model")=>action(async ticket=>{
    const path=await open(kind==="tools"?{directory:true,multiple:false,title:"选择含 tools 和 licenses 的 Whisper 资源目录"}:{multiple:false,title:"选择与固定清单匹配的模型文件",filters:[{name:"Whisper GGML",extensions:["bin"]}]});
    if(typeof path!=="string"||!gate.isCurrent(ticket))return;
    const next=kind==="tools"?await learningApi.installTranscriptionTools(libraryId,path):await learningApi.importTranscriptionModel(libraryId,model,path);
    if(gate.isCurrent(ticket)){setStatus(next);setConsent(false);setNotice("已核验所选组件。组件与模型为全局本机数据，不进入素材库备份。");}
  });
  const download=()=>action(async ticket=>{
    if(!learningApi.downloadTranscriptionModel)return;
    setNotice(`正在下载并校验 ${model} 模型…`);
    const next=await learningApi.downloadTranscriptionModel(libraryId,model);
    if(gate.isCurrent(ticket)){setStatus(next);setNotice("模型已下载并完成 SHA-256 校验。");}
  });
  const start=()=>action(async ticket=>{
    if(!consent||!status?.toolReady||!status.models.some(m=>m.id===model&&m.installed)||active(task))return;
    const next=await api.startLibraryTask({kind:"courseTranscription",request:{courseId,modelId:model}},libraryId);
    if(gate.isCurrent(ticket)){if(!scoped(next))throw new Error("返回任务不属于当前课程");watched.current.add(next.id);setTask(next);setConsent(false);setNotice("已加入后台任务。关闭此面板不会停止转写。");}
  });
  const cancel=()=>action(async ticket=>{if(!task)return;await api.cancelLibraryTask(task.id);if(gate.isCurrent(ticket)){setNotice("已请求取消；原件与已有字幕会保留。");setPollRevision(value=>value+1);}});
  const selected=status?.models.find(m=>m.id===model),locked=busy||active(task);
  return <section className="learning-local-transcription" aria-label="本地语音转写" aria-busy={busy}>
    <header><h3>本地语音转写</h3><button className="secondary-button" disabled={busy} onClick={()=>void refresh()}>重新检查组件</button></header>
    <p>优先导入现有 SRT / VTT。此处仅在本机识别，不上传整段视频，不自动调用付费分析。</p>
    {busy&&<LoadFeedback label="正在校验组件或准备任务…"/>}
    {error&&<p role="alert">{error}</p>}
    {status&&<>
      <p>Whisper {status.version} · {status.toolReady?"工具已核验":"工具未就绪"}</p><p>{status.toolMessage}</p>
      <label>语音模型<select value={model} disabled={locked} onChange={event=>{const value=event.target.value;if(value==="base"||value==="small"){setModel(value);setConsent(false);}}}>{status.models.map(m=><option key={m.id} value={m.id}>{m.label}</option>)}</select></label>
      {selected&&<p>{(selected.bytes/1048576).toFixed(1)} MiB · {selected.installed?"已安装并校验":"尚未安装"} · {selected.message}</p>}
    </>}
    <div className="learning-panel-actions">
      <button className="secondary-button" disabled={locked||!status} onClick={()=>void install("tools")}>安装本地组件包</button>
      {selected&&!selected.installed&&typeof learningApi.downloadTranscriptionModel==="function"&&<button className="primary-button" disabled={locked} onClick={()=>void download()}>一键下载并校验模型</button>}
      <button className="secondary-button" disabled={locked||!selected} onClick={()=>void install("model")}>导入所选模型</button>
      <button className="secondary-button" disabled={!selected||busy} onClick={()=>void action(async ticket=>{await navigator.clipboard.writeText(selected!.url);if(gate.isCurrent(ticket))setNotice("已复制固定版本模型下载链接；请自行选择下载后导入。");})}>复制模型下载链接</button>
    </div>
    <p className="muted">组件包应包含 tools/ 与 licenses/whisper-MIT.txt。仅复制固定清单中的文件；已有损坏组件需手动移走后重新导入，不会覆盖。临时完整音轨需要约每小时 110 MiB，识别速度取决于 CPU；取消后清理临时文件。</p>
    <label className="learning-local-consent"><input type="checkbox" checked={consent} disabled={locked} onChange={event=>setConsent(event.target.checked)}/>确认使用本机CPU及临时磁盘空间完成转写</label>
    <div className="learning-panel-actions"><button className="primary-button" disabled={locked||!consent||!status?.toolReady||!selected?.installed} onClick={()=>void start()}>开始本地转写</button>{active(task)&&<button className="secondary-button" disabled={busy} onClick={()=>void cancel()}>取消转写</button>}</div>
    {task&&<div role="status"><strong>{task.label}</strong><p>{task.phase} · {task.current}/{task.total||"—"}</p>{active(task)&&<progress aria-label="本地转写阶段" value={task.current} max={task.total||4}/>} {!!task.message&&<p>{task.message}</p>}{task.status==="completed"&&resultId&&<button className="secondary-button" onClick={()=>complete.current(resultId)}>载入生成字幕</button>}</div>}
    {notice&&<p role="status">{notice}</p>}
  </section>;
}
