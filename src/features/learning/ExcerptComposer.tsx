import {useCallback,useEffect,useRef,useState} from "react";
import {X} from "lucide-react";
import {api} from "../../lib/api";
import {knowledgeApi,type KnowledgeNote,type KnowledgeNotebook,type NoteSummary} from "../../lib/knowledge";
import {useRequestGate} from "../../lib/workspace";
import {LoadFeedback} from "../../components/LoadFeedback";
import type {LibraryTask,Page} from "../../types";
import {learningApi} from "./api";
import {learningTime} from "./LearningControls";
import type {CommitExcerptRequest,ExcerptDestination,ExcerptOperationSummary,ExcerptReceipt,ExcerptSelection,ExcerptTargets,StoredExcerpt} from "./types";
export interface ExcerptComposerProps {libraryId:string;courseId:string;selection:ExcerptSelection;durationMs?:number|null;projectId?:string;onClose:()=>void;onOpen:(receipt:ExcerptReceipt)=>void|Promise<void>}
export function ExcerptComposer(props:ExcerptComposerProps){return <Composer key={`${props.libraryId}:${props.courseId}`} {...props}/>;}
const active=(task:LibraryTask|null)=>!!task&&["queued","running"].includes(task.status);
const states:Record<string,string>={pending:"待确认恢复",committed:"已保存",cancelled:"已取消",unreadable:"记录需检查",queued:"排队中",running:"处理中",completed:"已完成",failed:"失败",stopped:"已停止",interrupted:"已中断",paused:"已暂停"};
const exactTime=(ms:number)=>`${learningTime(ms)}.${String(ms%1000).padStart(3,"0")}`;
function destinationLabel(destination:ExcerptDestination,targets:ExcerptTargets){
  if(destination.kind==="new")return `新笔记：${destination.title}`;
  if(destination.kind==="project")return `项目：${targets.projects.find(p=>p.id===destination.projectId)?.name||destination.projectId} · ${destination.title}`;
  if(destination.kind==="append")return `追加到笔记 ${destination.noteId} · 修订 ${destination.expectedRevision} · 第 ${destination.insertIndex} 个块之前`;
  return `参考板：${targets.boards.find(b=>b.id===destination.boardId)?.name||destination.boardId}`;
}
function Composer({libraryId,courseId,selection,durationMs,projectId,onClose,onOpen}:ExcerptComposerProps){
  const [preview,setPreview]=useState<StoredExcerpt|null>(null),[frozen,setFrozen]=useState<CommitExcerptRequest|null>(null),[receipt,setReceipt]=useState<ExcerptReceipt|null>(null);
  const [task,setTask]=useState<LibraryTask|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState("");
  const [kind,setKind]=useState<ExcerptDestination["kind"]>("new"),[title,setTitle]=useState(""),[notebook,setNotebook]=useState(""),[project,setProject]=useState(projectId||""),[board,setBoard]=useState("");
  const [note,setNote]=useState<KnowledgeNote|null>(null),[index,setIndex]=useState(0),[query,setQuery]=useState(""),[notes,setNotes]=useState<NoteSummary[]>([]),[noteOffset,setNoteOffset]=useState(0),[noteTotal,setNoteTotal]=useState(0);
  const [comment,setComment]=useState(selection.comment),[capture,setCapture]=useState(selection.frameTimeMs!==null),[includeFrame,setIncludeFrame]=useState(true);
  const [startTime,setStartTime]=useState(selection.ranges[0]?(selection.ranges[0].startMs/1000).toFixed(3):""),[endTime,setEndTime]=useState(selection.ranges[0]?(selection.ranges[0].endMs/1000).toFixed(3):"");
  const [targets,setTargets]=useState<ExcerptTargets>({projects:[],boards:[]}),[notebooks,setNotebooks]=useState<KnowledgeNotebook[]>([]);
  const [history,setHistory]=useState<Page<ExcerptOperationSummary>>({items:[],total:0,offset:0,limit:20}),[offset,setOffset]=useState(0),[preparedTasks,setPreparedTasks]=useState<LibraryTask[]>([]),[revision,setRevision]=useState(0);
  const gate=useRequestGate(),busyRef=useRef(false),composing=useRef(false),dialog=useRef<HTMLElement>(null);
  const locked=busy||active(task)||!!frozen||!!receipt;
  useEffect(()=>{const previous=document.activeElement as HTMLElement|null;dialog.current?.focus();return()=>{if(previous?.isConnected)previous.focus();};},[]);
  const run=async(work:()=>Promise<void>)=>{
    if(busyRef.current)return;busyRef.current=true;setBusy(true);setError("");const ticket=gate.begin();
    try{await work();}catch(reason){if(gate.isCurrent(ticket))setError(String(reason));}finally{busyRef.current=false;if(gate.isCurrent(ticket))setBusy(false);}
  };
  useEffect(()=>{let live=true;
    void Promise.allSettled([learningApi.excerptTargets(libraryId),knowledgeApi(libraryId).notebooks(),learningApi.listExcerptOperations(libraryId,courseId,offset,20),api.listLibraryTasks()]).then(results=>{
      if(!live)return;const [t,n,h,tasks]=results;
      if(t.status==="fulfilled")setTargets(t.value);
      if(n.status==="fulfilled")setNotebooks(n.value);
      if(h.status==="fulfilled")setHistory(h.value);
      if(tasks.status==="fulfilled")setPreparedTasks(tasks.value.filter(t=>t.libraryId===libraryId&&t.payload.kind==="courseExcerpt"&&t.payload.request.action==="prepare"&&t.payload.request.courseId===courseId).slice(0,20));
      const failures=results.filter(r=>r.status==="rejected");if(failures.length)setError(`部分目标或记录读取失败：${failures.map(r=>String(r.reason)).join("；")}；已有结果保留，可刷新重试。`);
    });return()=>{live=false;};
  },[libraryId,courseId,offset,revision]);
  useEffect(()=>{
    if(kind!=="append"||frozen)return;let live=true;setNotes([]);
    const timer=window.setTimeout(()=>{void knowledgeApi(libraryId).search({query,notebookId:null,trash:false,favorite:false,offset:noteOffset,limit:50}).then(page=>{if(live){setNotes(page.items);setNoteTotal(page.total);}}).catch(reason=>{if(live)setError(`笔记读取失败：${String(reason)}`);});},200);
    return()=>{live=false;window.clearTimeout(timer);};
  },[kind,query,noteOffset,libraryId,frozen]);
  const readPreview=useCallback(async(id:string)=>{
    const value=await learningApi.getExcerpt(libraryId,id);
    if(value.prepared.libraryId!==libraryId||value.prepared.text.courseId!==courseId)throw new Error("预览不属于当前课程或素材库");
    if(value.status!=="ready")throw new Error("预览已取消或提交，请读取对应操作记录");
    setPreview(value);setTitle(value.prepared.text.title+" · 学习摘录");setIncludeFrame(!!value.prepared.frame);setReceipt(null);
  },[libraryId,courseId]);
  const readReceipt=useCallback(async(request:CommitExcerptRequest)=>{
    const value=await learningApi.getExcerptOperation(libraryId,request.operationId);
    if(value.operationId!==request.operationId||value.excerptId!==request.excerptId)throw new Error("操作回执身份不一致，请重新核对记录");
    if(value.warning)setNotice(value.warning);
    if(value.receipt){setReceipt(value.receipt);setRevision(n=>n+1);}else throw new Error(value.status==="cancelled"?"操作已取消，不会继续保存":"尚无保存回执，请保留原确认重试或检查任务记录");
  },[libraryId]);
  const consumeTask=useCallback(async(next:LibraryTask,request:CommitExcerptRequest|null)=>{
    if(next.libraryId!==libraryId||next.payload.kind!=="courseExcerpt")throw new Error("任务不属于当前素材库");
    if(next.payload.request.action==="prepare"&&next.payload.request.courseId!==courseId)throw new Error("任务不属于当前课程");
    if(next.payload.request.action==="commit"&&(!request||next.payload.request.request.operationId!==request.operationId||next.payload.request.request.excerptId!==request.excerptId))throw new Error("任务确认身份不一致");
    setTask(next);
    if(next.status!=="completed")return;
    if(next.payload.request.action==="commit"){await readReceipt(next.payload.request.request);return;}
    const result=next.result;if(!result||typeof result!=="object"||!("previewId" in result)||typeof result.previewId!=="string")throw new Error("任务缺少可读取的预览ID，请核对任务记录");
    await readPreview(result.previewId);
  },[libraryId,courseId,readPreview,readReceipt]);
  useEffect(()=>{if(!active(task))return;let live=true;
    const timer=window.setTimeout(()=>{void api.getLibraryTask(task!.id).then(async next=>{if(live)await consumeTask(next,frozen);}).catch(reason=>{if(live)setError(`任务读取失败：${String(reason)}；关闭窗口不会停止后台任务，请刷新任务。`);});},1500);
    return()=>{live=false;window.clearTimeout(timer);};
  },[task,frozen,consumeTask,revision]);
  // Closing dismisses this view only. The shared task and durable operation own recovery.
  const prepare=()=>run(async()=>{
    if(frozen||active(task))return;setNotice("");
    let ranges=selection.ranges;
    if(!selection.cueIds.length){
      const parse=(value:string)=>{if(!/^\d+(?:\.\d{1,3})?$/.test(value.trim()))throw new Error("请输入非负秒数，最多三位小数");return Math.round(Number(value)*1000);};
      const start=parse(startTime),end=parse(endTime);
      if(!Number.isSafeInteger(start)||!Number.isSafeInteger(end)||start>=end||(durationMs!=null&&end>durationMs))throw new Error("时间范围无效或超过视频时长");
      if(capture&&selection.frameTimeMs!==null&&(selection.frameTimeMs<start||selection.frameTimeMs>=end))throw new Error("截图时间不在所选范围内");
      ranges=[{startMs:start,endMs:end}];
    }
    setPreview(null);await consumeTask(await learningApi.startExcerptTask(libraryId,{action:"prepare",courseId,selection:{...selection,ranges,comment,frameTimeMs:capture?selection.frameTimeMs:null}}),null);
  });
  const confirm=()=>run(async()=>{
    if(!preview||active(task))return;
    let request=frozen;
    if(!request){
      let destination:ExcerptDestination;
      if(kind==="new")destination={kind,title,notebookId:notebook||null};
      else if(kind==="project"){if(!project)throw new Error("请选择项目");destination={kind,projectId:project,title,notebookId:notebook||null};}
      else if(kind==="board"){if(!board||!preview.prepared.frame||!includeFrame)throw new Error("参考板需要真实截图和已选择的目标板");destination={kind,boardId:board,x:0,y:0};}
      else {if(!note||!Number.isSafeInteger(index)||index<0||index>(note.document.content?.length||0))throw new Error("请选择笔记并确认插入位置");destination={kind,noteId:note.id,expectedRevision:note.revision,insertIndex:index};}
      request={operationId:crypto.randomUUID(),excerptId:preview.id,destination,includeFrame:!!preview.prepared.frame&&includeFrame};setFrozen(request);
    }
    await consumeTask(await learningApi.startExcerptTask(libraryId,{action:"commit",request}),request);
  });
  const recover=(id:string)=>run(async()=>{
    if(frozen||active(task))throw new Error("请先核对或取消当前确认操作");
    const value=await learningApi.getExcerptOperation(libraryId,id);if(value.operationId!==id)throw new Error("操作身份不一致");
    setNotice(value.warning||"");
    if(value.receipt){setPreview(null);setFrozen(value.request);setReceipt(value.receipt);return;}
    if(value.status!=="pending")throw new Error("此操作已取消，不会重新写入");
    const next=await learningApi.getExcerpt(libraryId,value.excerptId);
    if(next.prepared.libraryId!==libraryId||next.prepared.text.courseId!==courseId||next.status!=="ready")throw new Error("未提交预览不可读取，记录保留，请核对或取消操作");
    setFrozen(value.request);setPreview(next);setIncludeFrame(value.request.includeFrame);setReceipt(null);setTask(null);
  });
  const cancel=()=>run(async()=>{if(!frozen||active(task))return;await learningApi.cancelExcerptOperation(libraryId,frozen.operationId);setFrozen(null);setPreview(null);setTask(null);setNotice("已取消本次确认，已保存内容不受影响");setRevision(n=>n+1);});
  const closeKey=(event:React.KeyboardEvent<HTMLElement>)=>{
    event.stopPropagation();if(event.nativeEvent.isComposing||composing.current)return;
    if(event.key==="Escape"){event.preventDefault();onClose();}
    else if(event.key==="Tab"){
      const nodes=[...event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled),input:not(:disabled),textarea:not(:disabled),select:not(:disabled)")].filter(n=>!n.closest("[hidden]"));
      const first=nodes[0],last=nodes.at(-1);if(event.shiftKey&&(document.activeElement===first||document.activeElement===event.currentTarget)){event.preventDefault();last?.focus();}else if(!event.shiftKey&&(document.activeElement===last||document.activeElement===event.currentTarget)){event.preventDefault();first?.focus();}
    }
  };
  return <div className="modal-backdrop" onPointerDown={event=>{if(event.target===event.currentTarget&&!composing.current)onClose();}}>
    <section ref={dialog} tabIndex={-1} className="modal-card learning-excerpt" role="dialog" aria-modal="true" aria-label="学习摘录" data-native-overlay onKeyDown={closeKey} onCompositionStart={()=>{composing.current=true;}} onCompositionEnd={()=>{composing.current=false;}}>
      <header><div><span className="eyebrow">LEARNING EXCERPT</span><h2>学习摘录</h2></div><button className="icon-button" aria-label="关闭摘录窗口" onClick={onClose}><X size={18}/></button></header>
      <div className="learning-excerpt-body">
        <p>保留完整选中字幕和准确时间段；预览后确认保存，不复制整段视频，也不调用付费 AI。</p>
        {error&&<p role="alert" className="learning-analysis-alert">{error}</p>}{notice&&<p role="status">{notice}</p>}
        {busy&&<LoadFeedback label="正在核对摘录…"/>}
        {!preview&&!frozen&&<><p>已选 {selection.cueIds.length} 段字幕{selection.ranges.length?` · ${selection.ranges.length} 个时间段`:""}</p>
          {!selection.cueIds.length&&<><label>开始时间（秒）<input inputMode="decimal" value={startTime} disabled={locked} onChange={event=>setStartTime(event.target.value)}/></label><label>结束时间（秒）<input inputMode="decimal" value={endTime} disabled={locked} onChange={event=>setEndTime(event.target.value)}/></label><p>精确到毫秒；结束时间必须晚于开始时间。</p></>}
          <label>摘录备注<textarea value={comment} disabled={locked} maxLength={20000} onChange={event=>setComment(event.target.value)}/></label>
          {selection.frameTimeMs!==null&&<label className="learning-excerpt-check"><input type="checkbox" checked={capture} disabled={locked} onChange={event=>setCapture(event.target.checked)}/>截图时间 {exactTime(selection.frameTimeMs)}</label>}
          <button className="secondary-button" disabled={locked} onClick={()=>void prepare()}>准备摘录预览</button></>}
        {task&&<section aria-label="摘录后台任务"><p>{states[task.status]||task.status} · {task.phase}</p>{task.status==="failed"&&task.message&&<p role="alert" className="learning-analysis-alert">{task.message}</p>}{active(task)&&<><progress aria-label="摘录任务进度"/><button className="secondary-button" disabled={busy||!task.cancellable} onClick={()=>void run(async()=>{await api.cancelLibraryTask(task.id);setNotice("已请求停止，最终结果以保存回执为准");setRevision(n=>n+1);})}>停止后台任务</button></>}
          <button className="secondary-button" disabled={busy} onClick={()=>void run(async()=>{await consumeTask(await api.getLibraryTask(task.id),frozen);setRevision(n=>n+1);})}>刷新任务</button></section>}
        {preview&&<section role="region" aria-label="摘录预览" className="learning-excerpt-preview"><h3>确认来源</h3><p>{preview.prepared.text.title}</p>
          <ul>{preview.prepared.text.ranges.map((r,i)=><li key={i}>{exactTime(r.startMs)}—{exactTime(r.endMs)}</li>)}</ul>
          {preview.prepared.text.cues.map(c=><pre key={c.id}>{c.text}</pre>)}{preview.prepared.text.comment&&<p className="learning-excerpt-comment">{preview.prepared.text.comment}</p>}
          {preview.prepared.frame&&<figure><img src={preview.prepared.frame.dataUrl} alt={`课程截图 ${exactTime(preview.prepared.frame.timeMs)}`}/><figcaption>{exactTime(preview.prepared.frame.timeMs)}</figcaption></figure>}
          {preview.prepared.warnings.map((w,i)=><p key={i}>{w}</p>)}
          {!frozen&&<><label className="learning-excerpt-check"><input type="checkbox" disabled={locked||!preview.prepared.frame||kind==="board"} checked={includeFrame&&!!preview.prepared.frame} onChange={event=>setIncludeFrame(event.target.checked)}/>保存截图</label>
          <label>保存到<select disabled={locked} value={kind} onChange={event=>{const v=event.target.value as ExcerptDestination["kind"];setKind(v);if(v==="board")setIncludeFrame(true);}}><option value="new">新建笔记</option><option value="append">已有笔记</option><option value="project">项目学习资料</option><option value="board" disabled={!preview.prepared.frame}>参考板</option></select></label>
          {(kind==="new"||kind==="project")&&<><label>摘录标题<input value={title} disabled={locked} onChange={event=>setTitle(event.target.value)}/></label><label>笔记本<select value={notebook} disabled={locked} onChange={event=>setNotebook(event.target.value)}><option value="">未分类</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select></label></>}
          {kind==="project"&&<label>所属项目<select value={project} disabled={locked} onChange={event=>setProject(event.target.value)}><option value="">请选择项目</option>{targets.projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label>}
          {kind==="board"&&<label>目标参考板<select value={board} disabled={locked} onChange={event=>setBoard(event.target.value)}><option value="">请选择参考板</option>{targets.boards.map(b=><option key={b.id} value={b.id}>{b.name}</option>)}</select></label>}
          {kind==="append"&&<><label>搜索目标笔记<input value={query} disabled={locked} onChange={event=>{setQuery(event.target.value);setNoteOffset(0);}}/></label><label>目标笔记<select value={note?.id||""} disabled={locked} onChange={event=>{const id=event.target.value;setNote(null);if(id)void run(async()=>{const n=await knowledgeApi(libraryId).get(id);if(n.id!==id||n.deletedAt)throw new Error("目标笔记不可追加");setNote(n);setIndex(n.document.content?.length||0);});}}><option value="">请选择笔记</option>{note&&!notes.some(n=>n.id===note.id)&&<option value={note.id}>{note.title}</option>}{notes.map(n=><option key={n.id} value={n.id}>{n.title}</option>)}</select></label>
            {noteTotal>50&&<nav aria-label="目标笔记分页"><button className="secondary-button" disabled={locked||!noteOffset} onClick={()=>setNoteOffset(n=>Math.max(0,n-50))}>上一页笔记</button><button className="secondary-button" disabled={locked||noteOffset+notes.length>=noteTotal} onClick={()=>setNoteOffset(n=>n+50)}>下一页笔记</button></nav>}
            {note&&<label>插入到第几个顶层块之前（0 为开头，{note.document.content?.length||0} 为末尾）<input type="number" min={0} max={note.document.content?.length||0} step={1} value={index} disabled={locked} onChange={event=>setIndex(Number(event.target.value))}/><span>已读取修订 {note.revision}；保存时若有新修订则拒绝覆盖。</span></label>}</>}</>}
        </section>}
        {frozen&&<section aria-label="冻结的保存确认"><h3>已锁定本次确认</h3><p>{destinationLabel(frozen.destination,targets)}</p><p>{frozen.includeFrame?"包含来源截图":"仅保存文字和来源"}</p><details><summary>操作编号</summary>{frozen.operationId}</details>{!receipt&&<div className="learning-panel-actions"><button className="secondary-button" disabled={busy||active(task)} onClick={()=>void run(()=>readReceipt(frozen))}>读取保存回执</button><button className="secondary-button" disabled={busy||active(task)} onClick={()=>void cancel()}>取消本次确认</button></div>}</section>}
        {receipt&&<section aria-label="摘录保存回执"><h3>摘录已保存</h3>{receipt.warnings.map((w,i)=><p key={i}>{w}</p>)}<button className="primary-button" disabled={busy} onClick={()=>void run(async()=>{await onOpen(receipt);})}>打开保存位置</button></section>}
        <details><summary>摘录记录与恢复</summary><button className="secondary-button" disabled={busy} onClick={()=>setRevision(n=>n+1)}>刷新记录与目标</button>
          {history.items.map(h=><div key={h.operationId}><button className="secondary-button" disabled={busy||active(task)||!!frozen||h.status==="unreadable"} onClick={()=>void recover(h.operationId)} aria-label={`读取记录：${h.title}`}>{h.title} · {states[h.status]}</button>{h.error&&<p>{h.error}</p>}{h.status==="pending"&&<button className="secondary-button" disabled={locked} onClick={()=>void run(async()=>{await learningApi.cancelExcerptOperation(libraryId,h.operationId);setRevision(n=>n+1);})}>取消未提交记录</button>}</div>)}
          {!history.total&&<p>暂无保存操作记录</p>}{history.total>20&&<nav aria-label="摘录记录分页"><button className="secondary-button" disabled={busy||!offset} onClick={()=>setOffset(n=>Math.max(0,n-20))}>上一页记录</button><button className="secondary-button" disabled={busy||offset+history.items.length>=history.total} onClick={()=>setOffset(n=>n+20)}>下一页记录</button></nav>}
          {preparedTasks.map(t=><button key={t.id} className="secondary-button" disabled={locked} onClick={()=>void run(()=>consumeTask(t,null))}>读取准备任务 · {states[t.status]} · {t.createdAt}</button>)}
        </details>
      </div>
      <footer><button className="secondary-button" onClick={onClose}>收起</button>{preview&&!receipt&&<button className="primary-button" disabled={busy||active(task)||(!frozen&&(kind==="new"||kind==="project")&&!title.trim())} onClick={()=>void confirm()}>{frozen?"重试原确认":"确认保存摘录"}</button>}</footer>
    </section>
  </div>;
}
