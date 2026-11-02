import {useEffect,useRef,useState} from "react";
import {learningApi,type CourseBatchCursor,type CourseImportBatchSummary,type CourseImportEntryPage,type CourseRevertReport,type CourseSourceBinding,type ImportBatchView,type ImportItemView,type ImportNotePreview,type ImportSourceBinding} from "./api";
import {api} from "../../lib/api";
import CourseSqliteImport from "./CourseSqliteImport";
import type {LibraryTask} from "../../types";
import "./noteArchives.css";
import "./learningImport.css";

type Props={libraryId:string;onClose:()=>void};

const CONFLICT_LABEL:Record<string,string>={
  unmapped:"新建",
  unchanged:"目标未变",
  source_changed:"来源已变",
  target_changed:"目标已改",
  both_changed:"双方已变",
  target_unavailable:"目标不可用",
};
const REVERT_LABEL:Record<string,string>={
  kept_modified:"已修改或被使用",kept_reused:"已被其他批次复用",kept_conflict:"存在冲突",kept_missing:"目标不可用",
};
const COURSE_CONFLICT_LABEL:Record<string,string>={unresolved:"未绑定目标媒体",progress_absent:"来源进度缺失",divergent_progress:"来源和目标进度不一致",source_changed:"来源已变",target_changed:"目标已改",both_changed:"来源和目标均已变化",target_unavailable:"目标不可用"};
type RevertReport={batchId:string;reverted:string[];kept:{itemId:string;outcome:string;noteId:string|null}[];archivesKept:true};
function readRevertReport(value:unknown,batchId:string):RevertReport|null{
  if(!value||typeof value!=="object"||!("batchId" in value)||value.batchId!==batchId||!("reverted" in value)||!Array.isArray(value.reverted)||!value.reverted.every(v=>typeof v==="string")||!("kept" in value)||!Array.isArray(value.kept)||!("archivesKept" in value)||value.archivesKept!==true)return null;
  if(!value.kept.every(v=>v&&typeof v==="object"&&typeof v.itemId==="string"&&typeof v.outcome==="string"&&(typeof v.noteId==="string"||v.noteId===null)))return null;
  return value as RevertReport;
}

function CourseRevertDetails({report}:{report:CourseRevertReport}){
  const [revertedPage,setRevertedPage]=useState(0);
  const [keptPage,setKeptPage]=useState(0);
  return <section aria-label="课程撤回报告"><h4>撤回报告</h4>
    <p>撤回记录仅包含目标 ID，保留记录可通过条目 ID 对照；原媒体保留。</p>
    <p>报告中的已撤回目标（{report.reverted.length}）</p>
    <div className="preview-table-wrap"><ul>{report.reverted.slice(revertedPage*100,revertedPage*100+100).map((id,index)=><li key={`${id}-${index}`}>{id}</li>)}</ul></div>
    {report.reverted.length>100&&<div><button type="button" aria-label="上一页已撤回目标" disabled={revertedPage===0} onClick={()=>setRevertedPage(value=>value-1)}>上一页</button><span>第 {revertedPage+1} / {Math.ceil(report.reverted.length/100)} 页</span><button type="button" aria-label="下一页已撤回目标" disabled={(revertedPage+1)*100>=report.reverted.length} onClick={()=>setRevertedPage(value=>value+1)}>下一页</button></div>}
    <p>保留记录（{report.kept.length}）</p>
    <div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>条目 ID</th><th>类型</th><th>保留原因</th><th>目标 ID</th></tr></thead><tbody>{report.kept.slice(keptPage*100,keptPage*100+100).map((entry,index)=><tr key={`${entry.entryId}-${index}`}><td>{entry.entryId}</td><td>{entry.targetKind==="course"?"课程":"系列"}</td><td>{REVERT_LABEL[entry.outcome]||entry.outcome}</td><td>{entry.targetId||"—"}</td></tr>)}</tbody></table></div>
    {report.kept.length>100&&<div><button type="button" aria-label="上一页保留记录" disabled={keptPage===0} onClick={()=>setKeptPage(value=>value-1)}>上一页</button><span>第 {keptPage+1} / {Math.ceil(report.kept.length/100)} 页</span><button type="button" aria-label="下一页保留记录" disabled={(keptPage+1)*100>=report.kept.length} onClick={()=>setKeptPage(value=>value+1)}>下一页</button></div>}
  </section>;
}

export default function LearningImportPanel(props:Props){
  return <ImportPanel key={props.libraryId} {...props}/>;
}

function ImportPanel({libraryId,onClose}:Props){
  const dialog=useRef<HTMLDialogElement>(null);
  const [binding,setBinding]=useState<ImportSourceBinding|null>(null);
  const [relativePath,setRelativePath]=useState("");
  const [preview,setPreview]=useState<ImportNotePreview|null>(null);
  const [drafts,setDrafts]=useState<ImportNotePreview[]>([]);
  const [batches,setBatches]=useState<ImportBatchView[]>([]);
  const [courseBatches,setCourseBatches]=useState<CourseImportBatchSummary[]|null>(null);
  const [courseCursors,setCourseCursors]=useState<(CourseBatchCursor|null)[]>([null]);
  const courseCursor=courseCursors[courseCursors.length-1];
  const [courseError,setCourseError]=useState("");
  const [selectedCourseBatch,setSelectedCourseBatch]=useState("");
  const [courseOffset,setCourseOffset]=useState(0);
  const [courseEntries,setCourseEntries]=useState<{batchId:string;offset:number;page:CourseImportEntryPage}|null>(null);
  const [courseEntriesError,setCourseEntriesError]=useState("");
  const [courseEntriesRefresh,setCourseEntriesRefresh]=useState(0);
  const [courseSource,setCourseSource]=useState<CourseSourceBinding|null>(null);
  const [courseSourceError,setCourseSourceError]=useState("");
  const [courseSourceStatus,setCourseSourceStatus]=useState("");
  const courseSourceRequest=useRef(0);
  const [items,setItems]=useState<ImportItemView[]>([]);
  const [selectedBatch,setSelectedBatch]=useState<string>("");
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const [status,setStatus]=useState("尚未绑定来源：选择本地来源目录后才会读取任何 Markdown。");
  const [refresh,setRefresh]=useState(0);
  const [courseRefresh,setCourseRefresh]=useState(0);

  useEffect(()=>{
    const element=dialog.current!;
    element.showModal();
    return()=>{element.close();};
  },[]);

  useEffect(()=>{
    let active=true;
    setError("");
    void learningApi.listImportBatches(libraryId).then(value=>{
      if(active)setBatches(value);
    }).catch(reason=>{if(active)setError(String(reason));});
    return()=>{active=false;};
  },[libraryId,status,refresh]);

  useEffect(()=>{
    let active=true;
    setCourseBatches(null);setCourseError("");
    void (courseCursor?learningApi.listCourseImportBatches(libraryId,courseCursor):learningApi.listCourseImportBatches(libraryId)).then(value=>{if(active)setCourseBatches(value);}).catch(reason=>{if(active)setCourseError(String(reason));});
    return()=>{active=false;};
  },[libraryId,refresh,courseRefresh,courseCursor]);

  useEffect(()=>{
    if(!selectedCourseBatch){setCourseEntries(null);return;}
    let active=true;
    setCourseEntries(null);setCourseEntriesError("");
    void learningApi.listCourseBatchEntries(libraryId,selectedCourseBatch,courseOffset).then(page=>{
      if(active)setCourseEntries({batchId:selectedCourseBatch,offset:courseOffset,page});
    }).catch(reason=>{if(active)setCourseEntriesError(String(reason));});
    return()=>{active=false;};
  },[libraryId,selectedCourseBatch,courseOffset,refresh,courseRefresh,courseEntriesRefresh]);

  useEffect(()=>{
    const completed=(event:Event)=>{
      const task=(event as CustomEvent<LibraryTask|undefined>).detail;
      if(task?.libraryId!==libraryId||["queued","running"].includes(task.status)||task.payload?.kind!=="learningImport")return;
      if(["commitCourse","cancelCourse","revertCourse"].includes(task.payload.request.action))setCourseRefresh(value=>value+1);
    };
    window.addEventListener("library-task-completed",completed);
    return()=>window.removeEventListener("library-task-completed",completed);
  },[libraryId]);

  useEffect(()=>{
    let active=true;const request=++courseSourceRequest.current;
    void learningApi.currentCourseSource(libraryId).then(value=>{if(active&&request===courseSourceRequest.current)setCourseSource(value);}).catch(reason=>{if(active&&request===courseSourceRequest.current)setCourseSourceError(String(reason));});
    return()=>{active=false;};
  },[libraryId]);

  useEffect(()=>{
    if(!selectedBatch){setItems([]);return;}
    let active=true;
    void learningApi.listImportItems(libraryId,selectedBatch).then(value=>{
      if(active)setItems(value);
    }).catch(reason=>{if(active)setError(String(reason));});
    return()=>{active=false;};
  },[libraryId,selectedBatch,status,refresh]);

  async function pickSource(){
    setBusy(true);setError("");setPreview(null);setRelativePath("");setDrafts([]);
    try{
      const next=await learningApi.bindImportSource(libraryId,"vault-ui");
      if(!next){setBinding(null);setStatus("已取消目录选择；未绑定来源。");return;}
      setBinding(next);
      setStatus(`已绑定来源。范围：${next.boundary}`);
    }catch(reason){
      setError(String(reason));
      setBinding(null);
    }finally{setBusy(false);}
  }

  async function pickNote(){
    if(!binding)return;
    setBusy(true);setError("");setPreview(null);
    try{
      const next=await learningApi.pickImportNote(libraryId,binding.profileId,binding.epoch,binding.vault,binding.root);
      if(!next){setRelativePath("");setStatus("已取消笔记选择；来源目录仍保留。");return;}
      setRelativePath(next.relativePath);setPreview(next);
      setStatus(next.canPrepare?`预检完成：${next.relativePath} 可排队准备。`:`预检完成：${next.relativePath} 不可排队，请按预检结果处理后重试。`);
    }catch(reason){setError(String(reason));}finally{setBusy(false);}
  }

  async function runPreview(){
    if(!binding)return;
    setBusy(true);setError("");
    try{
      const next=await learningApi.previewImportNote(
        libraryId,
        binding.profileId,
        binding.epoch,
        binding.vault,
        binding.root,
        relativePath.trim(),
      );
      setPreview(next);
      setStatus(next.canPrepare
        ?`预检完成：${next.relativePath} 可排队准备。`
        :`预检完成：${next.relativePath} 不可排队，请按预检结果处理后重试。`);
    }catch(reason){
      setPreview(null);
      setError(String(reason));
    }finally{setBusy(false);}
  }

  function rememberDraft(next:ImportNotePreview){
    setDrafts(current=>{
      const rest=current.filter(item=>item.relativePath!==next.relativePath);
      return [...rest,next].sort((left,right)=>left.relativePath.localeCompare(right.relativePath));
    });
  }

  async function pickNotes(){
    if(!binding)return;
    setBusy(true);setError("");
    try{
      const picked=await learningApi.pickImportNotes(libraryId,binding.profileId,binding.epoch,binding.vault,binding.root);
      if(!picked||picked.length===0){setStatus("已取消笔记选择；来源目录仍保留。");return;}
      setDrafts([...picked].sort((left,right)=>left.relativePath.localeCompare(right.relativePath)));
      setPreview(null);setRelativePath("");
      setStatus(`已加入 ${picked.length} 篇笔记。请确认每篇预检后排队准备；不会静默覆盖已有正文。`);
    }catch(reason){setError(String(reason));}finally{setBusy(false);}
  }

  function addDraft(){
    if(!preview?.canPrepare||preview.willOverwrite)return;
    rememberDraft(preview);
    setStatus(`已加入本批：${preview.relativePath}。可继续添加其他笔记，或排队准备。`);
  }

  async function queuePrepare(){
    if(!binding)return;
    const queued=drafts.length>0?drafts:preview?[preview]:[];
    if(queued.length===0||queued.some(item=>!item.canPrepare||item.willOverwrite))return;
    if(queued.length>32){setError("迁入批次笔记数量无效");return;}
    setBusy(true);setError("");
    try{
      const task=drafts.length===0&&preview
        ?await learningApi.queueImportPrepare(
          libraryId,
          binding.profileId,
          binding.epoch,
          binding.vault,
          binding.root,
          preview.relativePath,
          preview.sourceSha256,
          preview.selectionId,
          preview.targetToken,
        )
        :await learningApi.queueImportPrepareBatch(
          libraryId,
          binding.profileId,
          binding.epoch,
          binding.vault,
          binding.root,
          queued.map(item=>({relativePath:item.relativePath,sourceSha256:item.sourceSha256,targetToken:item.targetToken})),
          queued[0].selectionId,
        );
      setStatus(queued.length===1
        ?`已排队准备任务 ${task.id}（${task.label}）。不会静默覆盖已有正文。`
        :`已排队准备任务 ${task.id}（${task.label}），本批 ${queued.length} 篇。不会静默覆盖已有正文。`);
      if(drafts.length>0)setDrafts([]);
      setSelectedBatch("");
    }catch(reason){
      setError(String(reason));
    }finally{setBusy(false);}
  }

  async function runBatchAction(action:"commit"|"cancel"|"revert",batchId:string,expectedEpoch?:string){
    setBusy(true);setError("");
    try{
      if(action==="revert"&&!expectedEpoch)throw new Error("来源状态不可用，请刷新批次并重新确认。");
      const request=action==="revert"?{action,batchId,expectedEpoch:expectedEpoch!}:{action,batchId};
      const created=await api.startLibraryTask({kind:"learningImport",request},libraryId);
      for(;;){
        const task=await api.getLibraryTask(created.id);
        if(task.status==="completed"){
          const result=task.result;
          if(!result||typeof result!=="object"||!("batchId" in result)||result.batchId!==batchId||!("status" in result))
            throw new Error("无法确认批次结果，请刷新批次并查看任务中心。");
          if(action==="revert"){
            if(result.status!=="reverted"||!("report" in result)||!readRevertReport(result.report,batchId))throw new Error("无法确认撤回报告，请刷新批次并查看任务中心。");
            setStatus("撤回已完成；已修改、被使用或有冲突的笔记保持不变。");
          }else if(result.status==="committed"){
            setStatus(action==="commit"?`批次 ${batchId} 已提交（冲突仅留回执，不覆盖正文）。`:`批次已提交，无法取消；已提交内容保留（${batchId}）。`);
          }else if(result.status==="cancelled"){
            setStatus(action==="commit"?`批次已取消，未提交的条目未写入；此前已提交内容保留（${batchId}）。`:`批次 ${batchId} 已取消未完成项；已提交条目保留。`);
          }else throw new Error("无法确认批次结果，请刷新批次并查看任务中心。");
          setSelectedBatch(batchId);
          break;
        }
        if(task.status==="failed")throw new Error(task.message||"迁入任务失败");
        if(["stopped","interrupted","paused","cancelled"].includes(task.status)){
          throw new Error("任务已停止，请在任务中心检查后手动继续");
        }
        await new Promise(resolve=>window.setTimeout(resolve,400));
      }
    }catch(reason){
      setError(String(reason));
    }finally{setBusy(false);}
  }

  async function queueCourseAction(action:"cancelCourse"|"revertCourse",batch:CourseImportBatchSummary){
    if(!batch.currentSourceEpoch)return;
    if(action==="revertCourse"&&!window.confirm("撤回本批课程迁入？仅删除本批创建且未被修改或复用的课程和系列；原媒体保留。"))return;
    setBusy(true);setCourseError("");
    try{
      const request=action==="revertCourse"?{action,batchId:batch.id,expectedEpoch:batch.currentSourceEpoch}:{action,batchId:batch.id};
      const task=await api.startLibraryTask({kind:"learningImport",request},libraryId);
      setStatus(`课程批次任务 ${task.id} 已排队，请在任务中心查看结果。`);
    }catch(reason){setCourseError(String(reason));}finally{setBusy(false);}
  }

  async function pickCourseSource(){
    ++courseSourceRequest.current;setBusy(true);setCourseSource(null);setCourseSourceError("");setCourseSourceStatus("");
    try{
      const selected=await learningApi.selectCourseSource(libraryId);
      setCourseSource(selected);
      if(!selected)setCourseSourceStatus("已取消课程数据库选择。");
    }catch(reason){setCourseSourceError(String(reason));}finally{setBusy(false);}
  }

  async function releaseCourseSource(){
    if(!courseSource)return;
    ++courseSourceRequest.current;setBusy(true);setCourseSourceError("");setCourseSourceStatus("");
    try{
      await learningApi.releaseCourseSource(libraryId,courseSource.selectionId);
      setCourseSource(null);setCourseSourceStatus("课程来源选择已撤销或失效。");
    }catch(reason){setCourseSourceError(String(reason));}finally{setBusy(false);}
  }

  const queued=drafts.length>0?drafts:preview?[preview]:[];
  const canQueue=!!binding&&queued.length>0&&queued.every(item=>item.canPrepare&&!item.willOverwrite);
  const selected=batches.find(batch=>batch.id===selectedBatch);
  const revertReport=selected?.revertReport?readRevertReport(selected.revertReport,selected.id):null;
  const selectedItems=items.filter(item=>item.batchId===selectedBatch);
  const canRevert=!!selected&&!selected.revertReport&&["committed","cancelled"].includes(selected.status)&&selectedItems.some(item=>["committed","conflict"].includes(item.status));

  return <div className="modal-backdrop knowledge-archive-backdrop">
    <dialog ref={dialog} className="knowledge-archive-dialog learning-import-dialog" aria-label="学习资料迁入" onCancel={event=>{event.preventDefault();onClose();}} onPointerDown={event=>{
      const rect=event.currentTarget.getBoundingClientRect();
      if(event.clientX<rect.left||event.clientX>rect.right||event.clientY<rect.top||event.clientY>rect.bottom)onClose();
    }}>
      <header>
        <div>
          <span className="eyebrow">LEARNING IMPORT</span>
          <h2>学习资料迁入</h2>
        </div>
        <button type="button" className="secondary-button" autoFocus onClick={onClose}>关闭</button>
      </header>
      <p className="learning-import-boundary" role="note">
        授权边界：只读取你通过系统目录对话框明确选择的本地文件夹；仅在预检或准备时读取所选目录中的指定 Markdown；不会自动读取映流数据库，也不会根据清单提示路径自动授权（清单提示路径不构成授权）。冲突可暂存并提交回执，但 <strong>willOverwrite 恒为 false</strong>，已有笔记正文绝不被改写。
      </p>
      <p className="learning-import-boundary" role="note">
        费用边界：迁入只在本机进行 Markdown 读取、转换、暂存与提交，不调用付费 AI 或云端服务，不产生费用。准备、提交、取消都作为任务进入任务中心，可在那里查看进度与失败原因。
      </p>
      <p className="learning-import-boundary" role="note">
        同一已授权目录内可以加入多篇 Markdown，作为同一个批次准备。提交、取消和撤回仍作用于该批次。未加入本批的单篇预检仍可直接排队。
      </p>
      <div className="learning-import-actions">
        <button type="button" className="primary-button" disabled={busy} onClick={()=>void pickSource()}>选择来源目录…</button>
        <button type="button" disabled={busy||!binding} onClick={()=>void pickNote()}>选择 Markdown…</button>
        <button type="button" disabled={busy||!binding} onClick={()=>void pickNotes()}>选择多篇 Markdown…</button>
        <button type="button" disabled={busy||!binding||!relativePath.trim()} onClick={()=>void runPreview()}>预检相对路径</button>
        <button type="button" disabled={busy||!preview?.canPrepare||!!preview?.willOverwrite} onClick={addDraft}>加入本批</button>
        <button type="button" disabled={busy||!canQueue} onClick={()=>void queuePrepare()}>排队准备（不覆盖）</button>
      </div>
      {binding&&<dl className="learning-import-meta" aria-label="已绑定来源">
        <div><dt>素材库</dt><dd>{binding.libraryId}</dd></div>
        <div><dt>来源根</dt><dd><code>{binding.root}</code></dd></div>
        <div><dt>profile / epoch</dt><dd><code>{binding.profileId}</code> / <code>{binding.epoch}</code></dd></div>
        <div><dt>边界说明</dt><dd>{binding.boundary}</dd></div>
      </dl>}
      <label className="learning-import-path">相对路径
        <input aria-label="迁入相对路径" value={relativePath} disabled={busy||!binding} onChange={e=>{setRelativePath(e.target.value);setPreview(null);}} placeholder="notes/example.md"/>
      </label>
      {preview&&<section className="learning-import-preview" aria-label="预检结果">
        <h3>预检</h3>
        <p>{preview.message}</p>
        <ul>
          <li>冲突：{CONFLICT_LABEL[preview.conflict]||preview.conflict}</li>
          <li>标题：{preview.title}</li>
          <li>可准备：{preview.canPrepare?"是":"否"}</li>
          <li>会覆盖正文：{preview.willOverwrite?"是（禁止）":"否"}</li>
          <li>转换挂起：{preview.pendingConversion?"是":"否"}</li>
          {preview.targetId&&<li>目标笔记：{preview.targetId}</li>}
        </ul>
        {preview.warnings.length>0&&<p role="status">警告：{preview.warnings.join("；")}</p>}
      </section>}
      {drafts.length>0&&<section className="learning-import-drafts" aria-label="本批预检">
        <h3>本批预检（{drafts.length}）</h3>
        <ul>{drafts.map(item=><li key={item.relativePath}>
          <span>
            <strong>{item.relativePath}</strong>
            {` · ${CONFLICT_LABEL[item.conflict]||item.conflict}`}
            {` · ${item.canPrepare?"可准备":"不可准备"}`}
            {` · 覆盖${item.willOverwrite?"：是（禁止）":"：否"}`}
            {item.pendingConversion?" · 转换挂起":""}
            <small>{item.message}</small>
          </span>
          <button type="button" aria-label={`移出 ${item.relativePath}`} onClick={()=>setDrafts(current=>current.filter(draft=>draft.relativePath!==item.relativePath))}>移出</button>
        </li>)}</ul>
      </section>}
      <p className="learning-import-status" role="status">{status}</p>
      {error&&<p className="learning-import-error" role="alert">{error}<button type="button" onClick={()=>{setError("");setStatus(s=>s+"（已清除错误，可重试）");}}>清除并重试</button></p>}
      <section aria-label="课程来源数据库">
        <h3>课程来源数据库</h3>
        <p>通过系统文件对话框选择数据库副本，页面不能提交路径。非 Windows 只接受带合成标记的只读样本，未标记数据库会在打开 SQLite 之前被拒绝。只有点击课程预检才读取所选副本。</p>
        <button type="button" disabled={busy} onClick={()=>void pickCourseSource()}>选择课程数据库…</button>
        {courseSource&&<><p>已选择：{courseSource.fileName}</p><button type="button" disabled={busy} onClick={()=>void releaseCourseSource()}>撤销课程来源选择</button></>}
        {courseSource&&<CourseSqliteImport key={courseSource.selectionId} libraryId={libraryId} source={courseSource}/>}
        {courseSourceStatus&&<p role="status">{courseSourceStatus}</p>}{courseSourceError&&<p role="alert">课程来源：{courseSourceError}</p>}
      </section>
      <section aria-label="课程迁入批次">
        <h3>课程迁入批次</h3>
        <p>按时间查看课程批次；课程提交需先完成来源授权与预检。</p>
        {courseError?<p role="alert">课程批次读取失败：{courseError}<button type="button" onClick={()=>setCourseRefresh(value=>value+1)}>重新读取课程批次</button></p>:courseBatches===null?<p role="status">正在读取课程批次…</p>:courseBatches.length===0?<p>{courseCursors.length===1?"暂无课程迁入批次。":"此页暂无课程迁入批次。"}</p>:<ul>{courseBatches.slice(0,100).map(batch=><li key={batch.id}>
          <strong>{batch.id.slice(0,8)}…</strong>{` · ${batch.status} · ${batch.createdAt}`}
          {batch.staged?" · 已暂存":""}{` · 已提交 ${batch.committed} · 冲突 ${batch.conflicts}`}{batch.reverted&&<> · <span>已撤回</span></>}
          {batch.revertDamaged&&<> · <span role="alert">撤回记录损坏，请检查批次</span></>}
          {!batch.currentSourceEpoch&&<> · <span role="alert">来源状态损坏，暂不可在此操作</span></>}
          <button type="button" aria-expanded={selectedCourseBatch===batch.id} aria-label={`查看课程批次明细 ${batch.id.slice(0,8)}`} onClick={()=>{setCourseOffset(0);setSelectedCourseBatch(current=>current===batch.id?"":batch.id);}}>明细</button>
          {batch.currentSourceEpoch&&batch.status==="pending"&&<button type="button" disabled={busy} aria-label={`取消课程批次 ${batch.id.slice(0,8)}`} onClick={()=>void queueCourseAction("cancelCourse",batch)}>取消</button>}
          {batch.currentSourceEpoch&&batch.status==="committed"&&!batch.reverted&&!batch.revertDamaged&&<button type="button" disabled={busy} aria-label={`撤回课程批次 ${batch.id.slice(0,8)}`} onClick={()=>void queueCourseAction("revertCourse",batch)}>撤回</button>}
          {selectedCourseBatch===batch.id&&(courseEntriesError?<p role="alert">课程条目读取失败：{courseEntriesError}<button type="button" onClick={()=>setCourseEntriesRefresh(value=>value+1)}>重新读取课程条目</button></p>:courseEntries?.batchId!==batch.id||courseEntries.offset!==courseOffset?<p role="status">正在读取课程条目…</p>:<div>
            <p>以下为提交时记录。</p>
            {courseEntries.page.items.length===0?<p>暂无已提交或冲突条目。</p>:<div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>条目 ID</th><th>类型</th><th>来源条目</th><th>状态</th><th>冲突原因</th><th>目标 ID</th></tr></thead><tbody>{courseEntries.page.items.map(item=><tr key={item.id}><td>{item.id}</td><td>{item.entryKind==="course"?"课程":"系列"}</td><td>{item.sourceId}</td><td>{item.status==="committed"?"已提交":"冲突"}</td><td>{item.conflictCode?(COURSE_CONFLICT_LABEL[item.conflictCode]||item.conflictCode):"—"}</td><td>{item.targetId||"—"}</td></tr>)}</tbody></table></div>}
            <button type="button" aria-label="上一页课程条目" disabled={courseOffset===0} onClick={()=>setCourseOffset(value=>Math.max(0,value-100))}>上一页</button><span>第 {courseOffset/100+1} 页</span><button type="button" aria-label="下一页课程条目" disabled={!courseEntries.page.hasMore} onClick={()=>setCourseOffset(value=>value+100)}>下一页</button>
            {courseEntries.page.revertDamaged&&<p role="alert">撤回报告损坏，无法显示逐项结果。</p>}
            {courseEntries.page.revertReport&&<CourseRevertDetails key={batch.id} report={courseEntries.page.revertReport}/>}
          </div>)}
        </li>)}</ul>}
        <button type="button" aria-label="上一页课程批次" disabled={courseCursors.length===1||(courseBatches===null&&!courseError)} onClick={()=>{setCourseBatches(null);setCourseError("");setSelectedCourseBatch("");setCourseOffset(0);setCourseCursors(current=>current.slice(0,-1));}}>上一页</button>
        <span>第 {courseCursors.length} 页</span>
        <button type="button" aria-label="下一页课程批次" disabled={!courseBatches||courseBatches.length<=100} onClick={()=>{const last=courseBatches?.[99];if(!last)return;setCourseBatches(null);setCourseError("");setSelectedCourseBatch("");setCourseOffset(0);setCourseCursors(current=>[...current,{createdAt:last.createdAt,id:last.id}]);}}>下一页</button>
      </section>
      <div className="knowledge-archive-body learning-import-batches">
        <nav aria-label="迁入批次">
          <h3>批次</h3>
          <button type="button" className="secondary-button" disabled={busy} onClick={()=>setRefresh(value=>value+1)}>刷新批次</button>
          {batches.length===0?<p>尚无迁入批次。预检通过后可排队准备。</p>:batches.map(batch=>
            <button key={batch.id} type="button" aria-pressed={selectedBatch===batch.id} onClick={()=>setSelectedBatch(batch.id)}>
              <strong>{batch.status}</strong>
              <span>{batch.id.slice(0,8)}…</span>
              <small>{batch.finishedAt||batch.createdAt}</small>
            </button>
          )}
        </nav>
        <section aria-label="批次条目与恢复">
          <h3>条目 / 失败恢复</h3>
          <p className="learning-import-recovery">中断或失败的批次保持 pending：可再次「提交批次」续跑（已提交条目不会重复写入），或「取消未完成项」清理暂存；已提交笔记保留，冲突条目只留回执。</p>
          {!selectedBatch?<p>选择批次以查看条目状态、冲突码与暂存路径。</p>:
            <>
              <div className="learning-import-batch-actions">
                <button type="button" disabled={busy} onClick={()=>void runBatchAction("commit",selectedBatch)}>提交批次</button>
                <button type="button" disabled={busy} onClick={()=>void runBatchAction("cancel",selectedBatch)}>取消未完成项</button>
                <button type="button" disabled={busy||!canRevert} onClick={()=>{
                  if(!selected||!window.confirm("撤回本批迁入？仅将本批新建且之后未修改、未被使用的笔记移入回收站；其他笔记和原文归档保留。"))return;
                  void runBatchAction("revert",selected.id,selected.currentSourceEpoch);
                }}>撤回本批迁入</button>
              </div>
              {selectedItems.length===0?<p>该批次暂无条目。</p>:
                <ul>{selectedItems.map(item=><li key={item.id}>
                  <strong>{item.status}</strong>
                  {item.relativePath?` · ${item.relativePath}`:""}
                  {item.conflictCode?` · 冲突 ${item.conflictCode}`:""}
                  {item.targetId?` · 目标 ${item.targetId.slice(0,8)}…`:""}
                  {item.stageRelative?` · 暂存 ${item.stageRelative}`:""}
                </li>)}</ul>}
              {batches.find(b=>b.id===selectedBatch)?.receipt&&
                <pre className="learning-import-receipt" aria-label="批次回执">{JSON.stringify(batches.find(b=>b.id===selectedBatch)?.receipt,null,2)}</pre>}
              {selected?.revertReport&&!revertReport&&<p role="alert">撤回报告损坏，请检查批次，未重复执行撤回。</p>}
              {revertReport&&<section aria-label="撤回报告">
                <h4>撤回报告</h4>
                <p>已移入回收站：{revertReport.reverted.length}；保留：{revertReport.kept.length}。原文归档保留。</p>
                <ul>{revertReport.reverted.map(id=><li key={id}>已移入回收站：{id}</li>)}
                  {revertReport.kept.map(item=><li key={item.itemId}>{REVERT_LABEL[item.outcome]||item.outcome}：{item.noteId||item.itemId}</li>)}</ul>
              </section>}
            </>}
        </section>
      </div>
    </dialog>
  </div>;
}
