import {useCallback,useEffect,useId,useRef,useState} from "react";
import {api} from "../../lib/api";
import {LoadFeedback} from "../../components/LoadFeedback";
import {useRequestGate} from "../../lib/workspace";
import type {LibraryTask,Page} from "../../types";
import {learningApi} from "./api";
import {learningTime} from "./LearningControls";
import type {CourseAnalysisDetail,CourseAnalysisHistoryItem,CourseAnalysisPreview,CourseAnalysisRequest,CourseAnalysisResult,CourseAnalysisTemplate,LearningSubtitle,WhisperComponentStatus} from "./types";
import "./LearningWorkspace.css";
import {analysisTemplateLabels as templates} from "./analysisTemplates";

export interface CourseAnalysisPanelProps {libraryId:string;courseId:string;durationMs?:number|null;canSeek?:boolean;autoPrepare?:boolean;onSeek:(ms:number)=>void}
type Phase=CourseAnalysisRequest["stage"];
const labels:Record<string,string>={queued:"排队中",running:"进行中",partial:"部分完成",completed:"已完成",failed:"失败",cancelled:"已取消",pending:"待处理",stopped:"已停止",interrupted:"已中断",paused:"已暂停"};
const active=(task:LibraryTask|null)=>!!task&&["queued","running"].includes(task.status);
function preferredSubtitle(rows:LearningSubtitle[],previous:string){
  if(rows.some(row=>row.id===previous))return previous;
  return [...rows].reverse().find(row=>row.cueCount>0)?.id||rows.at(-1)?.id||"";
}
function LocalWhisperOffer({libraryId,courseId,disabled,onSubtitle}:{libraryId:string;courseId:string;disabled:boolean;onSubtitle:(id:string)=>void}){
  const [status,setStatus]=useState<WhisperComponentStatus|null>(null),[model,setModel]=useState<"base"|"small">("base"),[consent,setConsent]=useState(false);
  const [task,setTask]=useState<LibraryTask|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const gate=useRequestGate(),busyRef=useRef(false),delivered=useRef(new Set<string>()),ready=useRef(onSubtitle);ready.current=onSubtitle;
  const refresh=useCallback(async()=>{
    const ticket=gate.begin();setError("");
    try{const components=await learningApi.transcriptionComponents(libraryId);if(!gate.isCurrent(ticket))return;setStatus(components);const installed=components.models.filter(item=>item.installed);setModel(current=>installed.some(item=>item.id===current)?current:installed.find(item=>item.id==="base")?.id||installed[0]?.id||"base");}
    catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}
  },[libraryId,gate]);
  useEffect(()=>{void refresh();},[refresh]);
  useEffect(()=>{
    if(!task||!active(task))return;let keep=true;let timer=0;
    const poll=async()=>{
      try{const next=await api.getLibraryTask(task.id);if(!keep)return;if(next.libraryId!==libraryId||next.payload.kind!=="courseTranscription"||next.payload.request.courseId!==courseId)throw new Error("任务不属于当前课程");setTask(next);if(active(next))timer=window.setTimeout(()=>void poll(),2000);}
      catch(reason){if(keep)setError(`转写状态读取失败：${String(reason)}。后台仍继续，请重新检查组件。`);}
    };
    timer=window.setTimeout(()=>void poll(),0);return()=>{keep=false;window.clearTimeout(timer);};
  },[task?.id,task?.status,libraryId,courseId]);
  const resultId=task?.status==="completed"&&task.result&&typeof task.result==="object"&&"courseId" in task.result&&task.result.courseId===courseId&&"subtitleId" in task.result&&typeof task.result.subtitleId==="string"?task.result.subtitleId:null;
  useEffect(()=>{if(task?.status==="completed"&&resultId&&!delivered.current.has(task.id)){delivered.current.add(task.id);ready.current(resultId);}},[task,resultId]);
  const selected=status?.models.find(item=>item.id===model),installed=!!selected?.installed,toolReady=!!status?.toolReady&&installed,locked=disabled||busy||active(task);
  const run=async(job:()=>Promise<void>)=>{if(busyRef.current)return;busyRef.current=true;setBusy(true);setError("");const ticket=gate.begin();try{await job();if(!gate.isCurrent(ticket))return;}catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}};
  return <section className="learning-analysis-whisper" aria-label="本地转写材料">
    <h3>没有可用字幕</h3>
    <p>优先使用已导入的 SRT / VTT。没有可用段落时，可在本机已核验的 Whisper 上转写；不会自动下载工具或模型，也不会因此调用付费分析。</p>
    {!status&&!error&&<LoadFeedback label="正在检查本机转写组件…"/>}
    {error&&<p role="alert">{error}</p>}
    {status&&!toolReady&&<p role="status">Whisper {status.version} 未就绪：{status.toolReady?selected?.message||"所选模型尚未安装并校验":status.toolMessage}。请到「课程字幕」导入已核验组件，或只填写采样时间。不会自动下载。</p>}
    {status&&toolReady&&<>
      <p role="status">Whisper {status.version} 已核验。转写只在本机读取课程音频，不上传视频；完成后请再预览，不会自动开始付费分析。</p>
      <label>语音模型<select aria-label="语音模型" value={model} disabled={locked} onChange={event=>{const value=event.target.value;if(value==="base"||value==="small"){setModel(value);setConsent(false);}}}>{status.models.map(item=><option key={item.id} value={item.id} disabled={!item.installed}>{item.label}{item.installed?"":" · 未安装"}</option>)}</select></label>
      {selected&&<p>{(selected.bytes/1048576).toFixed(1)} MiB · {selected.message}</p>}
      <label className="learning-analysis-consent"><input type="checkbox" checked={consent} disabled={locked} onChange={event=>setConsent(event.target.checked)}/>确认使用本机 CPU 和临时磁盘完成本地转写</label>
      <div className="learning-panel-actions"><button className="secondary-button" type="button" disabled={locked||!consent} onClick={()=>void run(async()=>{const next=await api.startLibraryTask({kind:"courseTranscription",request:{courseId,modelId:model}},libraryId);if(next.libraryId!==libraryId||next.payload.kind!=="courseTranscription"||next.payload.request.courseId!==courseId)throw new Error("返回任务不属于当前课程");setTask(next);setConsent(false);})}>开始本地转写</button>{active(task)&&<button className="secondary-button" type="button" disabled={busy||!task?.cancellable} onClick={()=>void run(async()=>{if(!task)return;await api.cancelLibraryTask(task.id);setTask(current=>current?{...current,status:"cancelled"}:current);})}>取消本地转写</button>}</div>
      {task&&<p role="status">{task.phase||labels[task.status]||task.status}{task.total>0?` · ${task.current} / ${task.total}`:""}</p>}
    </>}
  </section>;
}
export function CourseAnalysisPanel(props:CourseAnalysisPanelProps){return <AnalysisContent key={`${props.libraryId}:${props.courseId}`} {...props}/>;}

function AnalysisContent({libraryId,courseId,durationMs,canSeek=true,autoPrepare=false,onSeek}:CourseAnalysisPanelProps){
  const [subtitles,setSubtitles]=useState<LearningSubtitle[]>([]),[subtitleId,setSubtitleId]=useState(""),[frameText,setFrameText]=useState(""),[subtitleState,setSubtitleState]=useState<"loading"|"ready"|"error">("loading");
  const [templateId,setTemplateId]=useState("general");
  const [templateNotice,setTemplateNotice]=useState("");
  const [customTemplates,setCustomTemplates]=useState<CourseAnalysisTemplate[]>([]),[templateReady,setTemplateReady]=useState(false),[templateBusy,setTemplateBusy]=useState(false),[templateRead,setTemplateRead]=useState(0);
  const [templateDraft,setTemplateDraft]=useState<{id:string|null;version:number|null;name:string;focus:string}>({id:null,version:null,name:"",focus:""});
  const [subtitleRead,setSubtitleRead]=useState(0);
  const pendingSubtitle=useRef("");
  const [history,setHistory]=useState<Page<CourseAnalysisHistoryItem>>({items:[],total:0,offset:0,limit:50}),[offset,setOffset]=useState(0),[analysisId,setAnalysisId]=useState("");
  const [stageKey,setStageKey]=useState<string|null>(null),[stageOffset,setStageOffset]=useState(0),[detail,setDetail]=useState<CourseAnalysisDetail|null>(null);
  const [prepared,setPrepared]=useState<{value:CourseAnalysisPreview;phase:Phase}|null>(null),[consent,setConsent]=useState(false),[now,setNow]=useState(Date.now());
  const [previewOffset,setPreviewOffset]=useState(0);
  const [task,setTask]=useState<LibraryTask|null>(null),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true),[reading,setReading]=useState(false),[revision,setRevision]=useState(0);
  const [error,setError]=useState(""),[notice,setNotice]=useState("");
  const gate=useRequestGate(),historyGate=useRequestGate(),resultGate=useRequestGate(),busyRef=useRef(false),watchTask=useRef("");
  const hintId=useId();
  const autoStarted=useRef(false);
  const templateOperation=useRef(false);
  useEffect(()=>{let keep=true;void Promise.all([learningApi.listCourseAnalysisTemplates(libraryId),learningApi.getCourseAnalysisDefault(libraryId)]).then(([rows,preferred])=>{if(keep){setCustomTemplates(rows);if(templateRead===0&&!templates[preferred]&&!rows.some(row=>row.id===preferred))setTemplateNotice("默认模板已不可用，已改用综合分析；可在设置中重新选择。");setTemplateId(value=>{const desired=templateRead===0?preferred:value;return templates[desired]||rows.some(row=>row.id===desired)?desired:"general";});setTemplateReady(true);}}).catch(reason=>{if(keep)setError(`读取分析模板失败：${String(reason)}`)});return()=>{keep=false};},[libraryId,templateRead]);
  useEffect(()=>{let keep=true;void learningApi.listSubtitles(libraryId,courseId).then(rows=>{if(!keep)return;const wanted=pendingSubtitle.current;setSubtitles(rows);setSubtitleId(previous=>wanted&&rows.some(row=>row.id===wanted)?(pendingSubtitle.current="",wanted):preferredSubtitle(rows,previous));setSubtitleState("ready");}).catch(reason=>{if(keep){setError(String(reason));setSubtitleState("error");}});return()=>{keep=false;};},[libraryId,courseId,subtitleRead]);
  const refresh=useCallback(async()=>{
    const ticket=historyGate.begin();
    try{
      const [page,tasks]=await Promise.all([learningApi.listCourseAnalyses(libraryId,courseId,offset,50),api.listLibraryTasks()]);
      if(!historyGate.isCurrent(ticket))return;
      const scoped=tasks.filter(t=>t.libraryId===libraryId&&t.payload.kind==="courseAnalysis"&&t.payload.request.courseId===courseId);
      setHistory(page);setTask(scoped.find(t=>active(t))||null);setAnalysisId(value=>value||page.items[0]?.id||"");setRevision(value=>value+1);setLoading(false);
      if(watchTask.current&&scoped.some(t=>t.id===watchTask.current)){
        const selected=await api.getLibraryTask(watchTask.current);
        if(!historyGate.isCurrent(ticket)||selected.libraryId!==libraryId||selected.payload.kind!=="courseAnalysis"||selected.payload.request.courseId!==courseId)return;
        const report=selected.result;
        if(report&&typeof report==="object"&&"analysisId" in report&&typeof report.analysisId==="string"&&"courseId" in report&&report.courseId===courseId){setAnalysisId(report.analysisId);setStageKey(null);setStageOffset(0);if(!active(selected))watchTask.current="";}
      }
    }catch(reason){if(historyGate.isCurrent(ticket)){setError(`读取分析记录失败：${String(reason)}；请刷新记录，后台任务仍继续。`);setLoading(false);}}
  },[libraryId,courseId,offset,historyGate]);
  useEffect(()=>{void refresh();},[refresh]);
  useEffect(()=>{if(!active(task))return;const timer=window.setTimeout(()=>void refresh(),2000);return()=>window.clearTimeout(timer);},[task,refresh]);
  useEffect(()=>{
    const ticket=resultGate.begin();if(!analysisId){setDetail(null);return;}
    setReading(true);setDetail(value=>value?.id===analysisId?{...value,result:null}:null);
    void learningApi.getCourseAnalysis(libraryId,courseId,analysisId,stageKey,stageOffset,50).then(value=>{
      if(resultGate.isCurrent(ticket)&&value.courseId===courseId&&value.id===analysisId&&value.selectedStageKey===stageKey){setDetail(value);setReading(false);}
    }).catch(reason=>{if(resultGate.isCurrent(ticket)){setError(String(reason));setReading(false);}});
  },[libraryId,courseId,analysisId,stageKey,stageOffset,revision,resultGate]);
  useEffect(()=>{if(!prepared)return;setNow(Date.now());const timer=window.setInterval(()=>setNow(Date.now()),1000);return()=>window.clearInterval(timer);},[prepared]);
  const invalidatePreview=()=>{gate.invalidate();setPrepared(null);setConsent(false);};
  const saveTemplate=async()=>{
    if(templateOperation.current||busyRef.current||!templateReady)return;templateOperation.current=true;setTemplateBusy(true);setError("");
    try{const saved=await learningApi.saveCourseAnalysisTemplate(libraryId,templateDraft.id,templateDraft.version,templateDraft.name,templateDraft.focus);setCustomTemplates(rows=>[...rows.filter(row=>row.id!==saved.id),saved]);setTemplateId(saved.id);setTemplateNotice("");setTemplateDraft({id:saved.id,version:saved.version,name:saved.name,focus:saved.focus});invalidatePreview();setNotice("模板已保存；请重新预览后确认分析。");}
    catch(reason){setError(`保存模板失败：${String(reason)}。可刷新模板后重试。`);}finally{templateOperation.current=false;setTemplateBusy(false);}
  };
  const deleteTemplate=async()=>{
    if(templateOperation.current||busyRef.current||!templateDraft.id||templateDraft.version===null||!window.confirm("删除这个自定义模板？已保存的分析历史会保留。"))return;
    templateOperation.current=true;setTemplateBusy(true);setError("");try{await learningApi.deleteCourseAnalysisTemplate(libraryId,templateDraft.id,templateDraft.version);setCustomTemplates(rows=>rows.filter(row=>row.id!==templateDraft.id));if(templateId===templateDraft.id)setTemplateId("general");setTemplateDraft({id:null,version:null,name:"",focus:""});invalidatePreview();setNotice("模板已删除，分析历史仍然保留。");}catch(reason){setError(`删除模板失败：${String(reason)}`);}finally{templateOperation.current=false;setTemplateBusy(false);}
  };
  const prepare=async(phase:Phase,parent?:string,automaticFrames?:number[])=>{
    if(busyRef.current||templateOperation.current||active(task))return;
    busyRef.current=true;setBusy(true);setError("");setNotice("");setPrepared(null);setConsent(false);setPreviewOffset(0);const ticket=gate.begin();
    try{
      let value:CourseAnalysisPreview;
      if(phase==="summary"){
        if(!parent)throw new Error("该历史缺少可核验的汇总父结果，请重新读取分析记录。");
        value=await learningApi.prepareCourseSummary(libraryId,courseId,parent);
      }else{
        const frameTimesMs=automaticFrames??(frameText.trim()?frameText.split(/[,，\s]+/).filter(Boolean).map(raw=>{const ms=Math.round(Number(raw)*1000);if(!/^(?:\d+(?:\.\d{1,3})?|\.\d{1,3})$/.test(raw)||!Number.isSafeInteger(ms))throw new Error("采样时间必须是非负秒数，最多三位小数。");return ms;}):[]);
        const custom=customTemplates.find(item=>item.id===templateId);
        value=await learningApi.prepareCourseAnalysis(libraryId,courseId,{subtitleId:subtitleId||null,frameTimesMs,...(templateId==="general"?{}:{templateId}),...(custom?{templateVersion:custom.version}:{})});
      }
      if(gate.isCurrent(ticket)&&value.courseId===courseId){setPrepared({value,phase});setNow(Date.now());}
    }catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}
  };
  useEffect(()=>{if(!autoPrepare||autoStarted.current||loading||!templateReady||templateBusy||subtitleState!=="ready"||active(task))return;const usableSubtitle=subtitles.some(row=>row.id===subtitleId&&row.cueCount>0);const frames=typeof durationMs==="number"&&Number.isSafeInteger(durationMs)&&durationMs>=4?[...new Set([1,2,3].map(part=>Math.floor(durationMs*part/4)))]:[];if(usableSubtitle||frames.length){autoStarted.current=true;setFrameText(frames.map(ms=>String(ms/1000)).join(", "));void prepare("segments",undefined,frames);}},[autoPrepare,loading,templateReady,templateBusy,subtitleState,subtitleId,subtitles,task,durationMs]);
  const start=async()=>{
    if(!prepared||!consent||busyRef.current||templateOperation.current||active(task))return;
    if(prepared.value.expiresAt*1000<=Date.now()){setError("预览已过期，请重新预览并确认费用。");setConsent(false);return;}
    busyRef.current=true;setBusy(true);setError("");const ticket=gate.begin();
    try{
      const next=await api.startLibraryTask({kind:"courseAnalysis",request:{courseId,previewId:prepared.value.previewId,stage:prepared.phase}},libraryId);
      if(gate.isCurrent(ticket)&&next.libraryId===libraryId){setTask(next);watchTask.current=next.id;setPrepared(null);setConsent(false);setNotice("已加入后台任务；关闭面板不会停止任务。不会自动写入笔记。");}
    }catch(reason){if(gate.isCurrent(ticket)){setError(`${String(reason)}；请重新预览，不要重放旧批准。`);setPrepared(null);setConsent(false);}}finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}
  };
  const cancel=async()=>{
    if(!task||busyRef.current)return;busyRef.current=true;setBusy(true);const ticket=gate.begin();
    try{await api.cancelLibraryTask(task.id);if(gate.isCurrent(ticket)){setNotice("已请求取消；已发送请求可能产生用量，成功阶段仍保留。");await refresh();}}
    catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}
  };
  const selectedPhase=detail?.parentAnalysisId?"summary":history.items.find(row=>row.id===analysisId)?.stage||"segments";
  const expired=!!prepared&&prepared.value.expiresAt*1000<=now;
  const selectedSubtitle=subtitles.find(row=>row.id===subtitleId);
  const usable=!!selectedSubtitle&&selectedSubtitle.cueCount>0;
  const acceptTranscript=(id:string)=>{pendingSubtitle.current=id;setSubtitleRead(value=>value+1);setNotice("本地字幕已保存。请预览材料并确认费用后再开始分析，不会自动发送。");invalidatePreview();};
  return <section className="learning-analysis" aria-label="课程分析">
    <header><h2>课程分析</h2><button className="secondary-button" disabled={busy} onClick={()=>{setError("");invalidatePreview();setSubtitleRead(value=>value+1);void refresh();}}>刷新记录</button></header>
    <p id={hintId}>先准备材料：优先使用已导入且有内容的字幕，可选附加少量 JPEG 采样帧。确认预览后做分段分析，再按需汇总。不上传整段视频，也不把整段画面当作视觉分析。失败后必须重新预览，不会自动重试付费请求，也不会自动写入笔记。</p>
    {error&&<div className="learning-analysis-alert" role="alert">{error}</div>}
    {notice&&<p role="status">{notice}</p>}
    {loading&&<LoadFeedback label="正在读取分析记录…"/>}
    <form className="learning-analysis-form" onSubmit={event=>{event.preventDefault();void prepare("segments");}}>
      <fieldset>
        <legend>分析材料</legend>
        <label>分析模板<select aria-label="分析模板" value={templateId} disabled={!templateReady||templateBusy||busy||active(task)} onChange={event=>{setTemplateId(event.target.value);setTemplateNotice("");invalidatePreview();}}>{Object.entries(templates).map(([id,name])=><option value={id} key={id}>{name}</option>)}{customTemplates.map(item=><option value={item.id} key={item.id}>{item.name} · 自定义</option>)}</select></label>{templateNotice&&<p role="status">{templateNotice}</p>}
        <details><summary>管理自定义模板</summary><p>填写希望重点分析的内容；保存或修改后，需要重新预览材料和费用。</p>
          <button type="button" className="secondary-button" disabled={templateBusy||busy||active(task)} onClick={()=>{setTemplateReady(false);setTemplateDraft({id:null,version:null,name:"",focus:""});setError("");invalidatePreview();setTemplateRead(value=>value+1);}}>刷新模板</button>
          {customTemplates.length>0&&<label>编辑已有模板<select aria-label="编辑已有模板" value={templateDraft.id||""} disabled={templateBusy||busy||active(task)} onChange={event=>{const row=customTemplates.find(item=>item.id===event.target.value);setTemplateDraft(row?{...row}:{id:null,version:null,name:"",focus:""});}}><option value="">新建模板</option>{customTemplates.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}</select></label>}
          <label>模板名称<input aria-label="模板名称" value={templateDraft.name} maxLength={80} disabled={templateBusy||busy||active(task)} onChange={event=>setTemplateDraft(old=>({...old,name:event.target.value}))}/></label>
          <label>分析要求<textarea aria-label="分析要求" value={templateDraft.focus} maxLength={4000} disabled={templateBusy||busy||active(task)} onChange={event=>setTemplateDraft(old=>({...old,focus:event.target.value}))}/></label>
          <button type="button" className="secondary-button" disabled={!templateReady||templateBusy||busy||active(task)||!templateDraft.name.trim()||!templateDraft.focus.trim()} onClick={()=>void saveTemplate()}>保存模板</button>{templateDraft.id&&<button type="button" className="secondary-button" disabled={templateBusy||busy||active(task)} onClick={()=>void deleteTemplate()}>删除模板</button>}
        </details>
        <label>分析字幕<select value={subtitleId} disabled={busy||active(task)} onChange={event=>{pendingSubtitle.current="";setSubtitleId(event.target.value);invalidatePreview();}} aria-describedby={hintId}>{!subtitles.length&&<option value="">无已导入字幕</option>}{subtitles.map(s=><option key={s.id} value={s.id}>{s.name} · {s.cueCount} 段{s.cueCount?"":" · 无可用段落"}</option>)}</select></label>
        {subtitleState==="ready"&&usable&&<p role="status">将发送「{selectedSubtitle?.name}」的完整段落{frameText.trim()?"，并附加列出的 JPEG 采样帧":"，不附加画面"}。不会上传视频文件。</p>}
        {subtitleState==="ready"&&selectedSubtitle&&!usable&&<p role="status">所选字幕没有可用段落，不能作为文字来源。</p>}
        {subtitleState==="ready"&&!usable&&<LocalWhisperOffer libraryId={libraryId} courseId={courseId} disabled={busy||active(task)} onSubtitle={acceptTranscript}/>}
        <label>采样时间（秒）<input value={frameText} disabled={busy||active(task)} placeholder="例如 12.5, 30，可留空" aria-describedby={hintId} onChange={event=>{setFrameText(event.target.value);invalidatePreview();}}/></label>
        <p>留空表示不采样。填写的每个时间只抽取一张 JPEG，不是整段视频画面分析。</p>
        <button className="secondary-button" disabled={!templateReady||templateBusy||busy||active(task)||loading} type="submit">预览分段分析</button>
      </fieldset>
    </form>
    {busy&&<LoadFeedback label="正在校验资料并处理请求…"/>}
    {prepared&&<section className="learning-analysis-preview" aria-label="分析费用预览">
      <h3>{prepared.phase==="summary"?"汇总":"分段"}确认</h3><p>本次上限 {prepared.value.tokensMax.toLocaleString("zh-CN")} Token · {prepared.value.model}</p>
      <p>分析模板：{templates[prepared.value.templateId]||customTemplates.find(item=>item.id===prepared.value.templateId)?.name||prepared.value.templateId} · 版本 {prepared.value.templateVersion}</p>
      <p>{prepared.value.stages.filter(s=>s.cached).length} 个阶段可复用缓存 · {prepared.value.frameIds.length} 个来源帧</p>
      <p>字幕：{subtitles.find(s=>s.id===prepared.value.subtitleId)?.name||(prepared.value.subtitleId?"快照中的字幕":"未使用字幕")}</p>
      <details><summary>查看 {prepared.value.stages.length} 个阶段范围</summary><ol start={previewOffset+1}>{prepared.value.stages.slice(previewOffset,previewOffset+50).map(s=><li key={s.key}>{learningTime(s.startMs)}—{learningTime(s.endMs)} · {s.cached?"缓存":"需请求"} · {s.tokensMax.toLocaleString("zh-CN")} Token</li>)}</ol>{prepared.value.stages.length>50&&<nav aria-label="预览阶段分页"><button className="secondary-button" disabled={!previewOffset} onClick={()=>setPreviewOffset(value=>Math.max(0,value-50))}>上一页预览</button><span>{Math.floor(previewOffset/50)+1} / {Math.ceil(prepared.value.stages.length/50)}</span><button className="secondary-button" disabled={previewOffset+50>=prepared.value.stages.length} onClick={()=>setPreviewOffset(value=>value+50)}>下一页预览</button></nav>}</details>
      {prepared.value.warnings.map((w,i)=><p key={i}>{w}</p>)}
      {expired&&<p role="status">预览已过期，请重新预览。</p>}
      <label className="learning-analysis-consent"><input type="checkbox" checked={consent} disabled={expired||busy} onChange={event=>setConsent(event.target.checked)}/>确认发送上述材料给 DeepSeek，并使用显示的额度</label>
      <button className="primary-button" disabled={!consent||expired||templateBusy||busy||active(task)} onClick={()=>void start()}>确认并开始分析</button>
    </section>}
    {active(task)&&<section aria-label="当前分析任务"><p role="status">{task!.phase||labels[task!.status]}{task!.total>0?` · ${task!.current} / ${task!.total}`:" · 等待实际阶段数量"}</p>{task!.total>0?<progress aria-label="分析任务进度" max={task!.total} value={task!.current}/>:<progress aria-label="分析任务进度"/>}<button className="secondary-button" disabled={busy||!task!.cancellable} onClick={()=>void cancel()}>取消分析任务</button></section>}
    <section aria-label="分析历史"><h3>分析历史</h3>{!loading&&!history.total&&<p>暂无分析记录。先预览资料和费用，再确认开始。</p>}
      {!!history.items.length&&<label>历史记录<select value={analysisId} onChange={event=>{watchTask.current="";setAnalysisId(event.target.value);setStageKey(null);setStageOffset(0);}}>{history.items.map(row=><option value={row.id} key={row.id}>{row.stage==="summary"?"汇总":"分段"} · {labels[row.status]||row.status} · {row.completedStages}/{row.totalStages} · {new Date(row.createdAt*1000).toLocaleString("zh-CN")}</option>)}</select></label>}
      {history.total>50&&<nav aria-label="分析历史分页"><button className="secondary-button" disabled={!offset} onClick={()=>{setAnalysisId("");setOffset(value=>Math.max(0,value-50));}}>上一页记录</button><span>{Math.floor(offset/50)+1} / {Math.ceil(history.total/50)}</span><button className="secondary-button" disabled={offset+history.items.length>=history.total} onClick={()=>{setAnalysisId("");setOffset(value=>value+50);}}>下一页记录</button></nav>}
      {reading&&<LoadFeedback label="正在核验历史来源…"/>}
      {detail&&<><p>{labels[detail.status]||detail.status}</p>{detail.error&&<p className="learning-analysis-alert">{detail.error}</p>}{detail.staleReasons.map((reason,i)=><p key={i} role="status">{reason}</p>)}
        <div className="learning-panel-actions"><button className="secondary-button" disabled={busy||active(task)||!!detail.staleReasons.length||!(detail.status==="partial"||detail.status==="failed"||detail.status==="cancelled")} onClick={()=>void prepare(selectedPhase,detail.parentAnalysisId||undefined)}>重新预览缺失阶段</button>
        <button className="secondary-button" disabled={busy||active(task)||!!detail.staleReasons.length||detail.status!=="completed"||(selectedPhase==="summary"&&!!detail.result)} onClick={()=>void prepare("summary",detail.id)}>预览下一层汇总</button>
        <button className="secondary-button" disabled={!stageKey||reading} onClick={()=>setStageKey(null)}>查看整体结果</button></div>
        <ol className="learning-analysis-stages" start={detail.stages.offset+1}>{detail.stages.items.map((stage,n)=><li key={stage.key}><button className="secondary-button" aria-label={`查看第 ${detail.stages.offset+n+1} 阶段`} aria-pressed={stageKey===stage.key} onClick={()=>setStageKey(stage.key)}>{learningTime(stage.startMs)}—{learningTime(stage.endMs)} · {labels[stage.status]}{stage.tokensUsed!==null?` · 已用 ${stage.tokensUsed} Token`:""}</button>{stage.error&&<p>{stage.error}</p>}</li>)}</ol>
        {detail.stages.total>50&&<nav aria-label="分析阶段分页"><button className="secondary-button" disabled={!stageOffset||reading} onClick={()=>setStageOffset(value=>Math.max(0,value-50))}>上一页阶段</button><button className="secondary-button" disabled={stageOffset+detail.stages.items.length>=detail.stages.total||reading} onClick={()=>setStageOffset(value=>value+50)}>下一页阶段</button></nav>}
        {detail.result?<AnalysisResult result={detail.result} canSeek={canSeek&&!detail.staleReasons.length} onSeek={onSeek}/>:!reading&&<p>请选择已完成阶段查看结果；多组汇总需逐层确认后才有整体摘要。</p>}
      </>}
    </section>
  </section>;
}

function AnalysisResult({result,canSeek,onSeek}:{result:CourseAnalysisResult;canSeek:boolean;onSeek:(ms:number)=>void}){
  const jump=(ms:number,title:string)=><button className="secondary-button" disabled={!canSeek} onClick={()=>onSeek(ms)}>{learningTime(ms)} · {title}</button>;
  return <section className="learning-analysis-result" aria-label="分析结果"><h3>{result.oneSentenceSummary}</h3><p>{result.overview}</p>
    {!!result.chapters.length&&<><h4>章节</h4><ol>{result.chapters.map((c,i)=><li key={i}>{jump(c.startMs,c.title)}<p>{c.summary}</p></li>)}</ol></>}
    {!!result.keyPoints.length&&<><h4>重点</h4><ol>{result.keyPoints.map((p,i)=><li key={i}>{jump(p.timeMs,p.title)}<p>{p.content}</p></li>)}</ol></>}
    {!!result.tutorialSteps.length&&<><h4>操作步骤</h4><ol>{result.tutorialSteps.map(s=><li key={s.order}>{jump(s.timeMs,s.title)}<p>{s.description}</p>{s.notes.map((text,i)=><p key={i}>{text}</p>)}</li>)}</ol></>}
    {!!result.terms.length&&<><h4>术语</h4><dl>{result.terms.map((t,i)=><div key={i}><dt>{jump(t.timeMs,t.name)}</dt><dd>{t.definition}</dd></div>)}</dl></>}
    {!!result.actionItems.length&&<><h4>建议练习</h4><ul>{result.actionItems.map((text,i)=><li key={i}>{text}</li>)}</ul></>}
    {result.sections.map(section=><section key={section.id}><h4>{section.title}</h4>{section.items.map((item,i)=><div key={i}>{item.timeMs===null?<strong>{item.title}</strong>:jump(item.timeMs,item.title)}<p>{item.content}</p></div>)}</section>)}
    {!!result.tags.length&&<p>标签：{result.tags.join("、")}</p>}{result.warnings.map((warning,i)=><p key={i} role="status">{warning}</p>)}
    {!!result.sectionValidation.length&&<details open><summary>本机来源校验 · {result.sectionValidation.length} 项</summary><ul>{result.sectionValidation.map((issue,i)=><li key={i}><strong>{issue.itemTitle}</strong><p>{issue.reason}</p></li>)}</ul></details>}
  </section>;
}
