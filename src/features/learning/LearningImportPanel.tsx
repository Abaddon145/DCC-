import {useEffect,useRef,useState} from "react";
import {learningApi,type ImportBatchView,type ImportItemView,type ImportNotePreview,type ImportSourceBinding} from "./api";
import {api} from "../../lib/api";
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

export default function LearningImportPanel(props:Props){
  return <ImportPanel key={props.libraryId} {...props}/>;
}

function ImportPanel({libraryId,onClose}:Props){
  const dialog=useRef<HTMLDialogElement>(null);
  const [binding,setBinding]=useState<ImportSourceBinding|null>(null);
  const [relativePath,setRelativePath]=useState("notes/hello.md");
  const [preview,setPreview]=useState<ImportNotePreview|null>(null);
  const [batches,setBatches]=useState<ImportBatchView[]>([]);
  const [items,setItems]=useState<ImportItemView[]>([]);
  const [selectedBatch,setSelectedBatch]=useState<string>("");
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const [status,setStatus]=useState("尚未绑定来源：选择本地来源目录后才会读取任何 Markdown。");

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
  },[libraryId,status]);

  useEffect(()=>{
    if(!selectedBatch){setItems([]);return;}
    let active=true;
    void learningApi.listImportItems(libraryId,selectedBatch).then(value=>{
      if(active)setItems(value);
    }).catch(reason=>{if(active)setError(String(reason));});
    return()=>{active=false;};
  },[libraryId,selectedBatch,status]);

  async function pickSource(){
    setBusy(true);setError("");setPreview(null);
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

  async function queuePrepare(){
    if(!binding||!preview?.canPrepare||preview.willOverwrite)return;
    setBusy(true);setError("");
    try{
      const task=await learningApi.queueImportPrepare(
        libraryId,
        binding.profileId,
        binding.epoch,
        binding.vault,
        binding.root,
        relativePath.trim(),
      );
      setStatus(`已排队准备任务 ${task.id}（${task.label}）。不会静默覆盖已有正文。`);
      setSelectedBatch("");
    }catch(reason){
      setError(String(reason));
    }finally{setBusy(false);}
  }

  async function runBatchAction(action:"commit"|"cancel",batchId:string){
    setBusy(true);setError("");
    try{
      const created=await api.startLibraryTask(
        {kind:"learningImport",request:{action,batchId}},
        libraryId,
      );
      for(;;){
        const task=await api.getLibraryTask(created.id);
        if(task.status==="completed"){
          setStatus(action==="commit"?`批次 ${batchId} 已提交（冲突仅留回执，不覆盖正文）。`:`批次 ${batchId} 已取消未完成项；已提交条目保留。`);
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
      <div className="learning-import-actions">
        <button type="button" className="primary-button" disabled={busy} onClick={()=>void pickSource()}>选择来源目录…</button>
        <button type="button" disabled={busy||!binding} onClick={()=>void runPreview()}>预检相对路径</button>
        <button type="button" disabled={busy||!preview?.canPrepare||!!preview?.willOverwrite} onClick={()=>void queuePrepare()}>排队准备（不覆盖）</button>
      </div>
      {binding&&<dl className="learning-import-meta" aria-label="已绑定来源">
        <div><dt>素材库</dt><dd>{binding.libraryId}</dd></div>
        <div><dt>来源根</dt><dd><code>{binding.root}</code></dd></div>
        <div><dt>profile / epoch</dt><dd><code>{binding.profileId}</code> / <code>{binding.epoch}</code></dd></div>
        <div><dt>边界说明</dt><dd>{binding.boundary}</dd></div>
      </dl>}
      <label className="learning-import-path">相对路径
        <input aria-label="迁入相对路径" value={relativePath} disabled={busy||!binding} onChange={e=>setRelativePath(e.target.value)} placeholder="notes/example.md"/>
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
      <p className="learning-import-status" role="status">{status}</p>
      {error&&<p className="learning-import-error" role="alert">{error}<button type="button" onClick={()=>{setError("");setStatus(s=>s+"（已清除错误，可重试）");}}>清除并重试</button></p>}
      <div className="knowledge-archive-body learning-import-batches">
        <nav aria-label="迁入批次">
          <h3>批次</h3>
          <button type="button" className="secondary-button" disabled={busy} onClick={()=>setStatus(s=>s.endsWith("（已刷新批次）")?s:`${s}（已刷新批次）`)}>刷新批次</button>
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
              </div>
              {items.length===0?<p>该批次暂无条目。</p>:
                <ul>{items.map(item=><li key={item.id}>
                  <strong>{item.status}</strong>
                  {item.conflictCode?` · 冲突 ${item.conflictCode}`:""}
                  {item.targetId?` · 目标 ${item.targetId.slice(0,8)}…`:""}
                  {item.stageRelative?` · 暂存 ${item.stageRelative}`:""}
                </li>)}</ul>}
              {batches.find(b=>b.id===selectedBatch)?.receipt&&
                <pre className="learning-import-receipt" aria-label="批次回执">{JSON.stringify(batches.find(b=>b.id===selectedBatch)?.receipt,null,2)}</pre>}
            </>}
        </section>
      </div>
    </dialog>
  </div>;
}
