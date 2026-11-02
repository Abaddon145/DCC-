import {useEffect,useRef,useState} from "react";
import {save} from "@tauri-apps/plugin-dialog";
import {LoadFeedback} from "../../components/LoadFeedback";
import {learningApi} from "./api";
import type {NoteArchivePage,NoteArchiveVersion,NoteArchiveView} from "./types";
import "./noteArchives.css";

interface Props {libraryId:string;onClose:()=>void}
const states={unconverted:"尚未转换",partial:"部分转换，仍有待处理内容",converted:"已有完整转换记录"};
export default function NoteArchiveBrowser(props:Props) {return <ArchiveBrowser key={props.libraryId} {...props}/>;}
function ArchiveBrowser({libraryId,onClose}:Props) {
  const dialog=useRef<HTMLDialogElement>(null);
  const [page,setPage]=useState<NoteArchivePage|null>(null),[offset,setOffset]=useState(0),[retry,setRetry]=useState(0);
  const [selected,setSelected]=useState<NoteArchiveVersion|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState("");
  useEffect(()=>{const element=dialog.current!;element.showModal();return()=>{element.close();};},[]);
  useEffect(()=>{
    let active=true;setLoading(true);setError("");setPage(null);setSelected(null);
    void learningApi.listNoteArchives(libraryId,offset,50).then(value=>{if(active){setPage(value);setLoading(false);}}).catch(reason=>{if(active){setError(String(reason));setLoading(false);}});
    return()=>{active=false;};
  },[libraryId,offset,retry]);
  return <div className="modal-backdrop knowledge-archive-backdrop"><dialog ref={dialog} className="knowledge-archive-dialog" aria-label="原文归档" onCancel={event=>{event.preventDefault();onClose();}} onPointerDown={event=>{
    if(event.target!==event.currentTarget)return;
    const box=event.currentTarget.getBoundingClientRect();
    if(event.clientX<box.left||event.clientX>box.right||event.clientY<box.top||event.clientY>box.bottom)onClose();
  }}>
    <header><div><span className="eyebrow">SOURCE ARCHIVE</span><h2>原文归档</h2></div><button type="button" className="secondary-button" autoFocus onClick={onClose}>关闭归档</button></header>
    <div className="knowledge-archive-summary"><p>只读核对迁入的原始版本；不会改写笔记或源文件。转换记录不代表对应笔记仍存在。</p>{page&&<p>{page.versions.total} 个原文版本 · {page.unconvertedCount} 个版本未完整转换（含 {page.partialCount} 个部分转换）</p>}</div>
    {loading&&<LoadFeedback label="正在读取原文版本…"/>}{error&&<div className="knowledge-archive-error" role="alert"><p>原件列表读取失败：{error}</p><button className="secondary-button" onClick={()=>setRetry(value=>value+1)}>重新读取</button></div>}
    {page&&<><div className="knowledge-archive-body"><nav aria-label="原文版本">{page.versions.items.length===0?<p>尚无原文归档。普通笔记历史仍在“历史”中。</p>:page.versions.items.map(version=><button key={version.id} type="button" aria-pressed={selected?.id===version.id} onClick={()=>setSelected(version)}><strong>{version.sourceKey}</strong><span>{version.versionKey}</span><small>{states[version.conversionState]}</small></button>)}</nav>
      {selected?<ArchiveReader key={selected.id} libraryId={libraryId} version={selected}/>:<section className="knowledge-archive-reader"><p>选择一个版本后读取原件。</p></section>}
    </div><footer><button className="secondary-button" disabled={offset===0} onClick={()=>setOffset(value=>Math.max(0,value-50))}>上一页</button><span>第 {Math.floor(offset/50)+1} 页</span><button className="secondary-button" disabled={offset+50>=page.versions.total} onClick={()=>setOffset(value=>value+50)}>下一页</button></footer></>}
  </dialog></div>;
}
function ArchiveReader({libraryId,version}:{libraryId:string;version:NoteArchiveVersion}) {
  const [view,setView]=useState<NoteArchiveView|null>(null),[loading,setLoading]=useState(true),[error,setError]=useState(""),[retry,setRetry]=useState(0);
  const [exporting,setExporting]=useState(false),[notice,setNotice]=useState("");
  const active=useRef(false),busy=useRef(false);
  useEffect(()=>{active.current=true;return()=>{active.current=false;};},[]);
  useEffect(()=>{
    let keep=true;setLoading(true);setError("");setView(null);
    void learningApi.readNoteArchive(libraryId,version.id).then(value=>{
      if(!keep)return;
      if(value.archiveId!==version.id){setError("原件身份不一致，请重新读取。");}else setView(value);
      setLoading(false);
    }).catch(reason=>{if(keep){setError(String(reason));setLoading(false);}});
    return()=>{keep=false;};
  },[libraryId,version.id,retry]);
  const exportOriginal=async()=>{
    if(!view||busy.current)return;busy.current=true;setExporting(true);setError("");setNotice("");
    try {
      const extension=view.text===null?"bin":"md";
      const path=await save({defaultPath:`archive-${version.id}.${extension}`,filters:[{name:"原始字节（不覆盖已有文件）",extensions:[extension]}]});
      if(!path||!active.current)return;
      await learningApi.exportNoteArchive(libraryId,version.id,path);
      if(active.current)setNotice("原字节已导出，归档保持不变。");
    }catch(reason){if(active.current)setError(`导出失败：${String(reason)}`);}finally{busy.current=false;if(active.current)setExporting(false);}
  };
  return <section className="knowledge-archive-reader" aria-label="原件内容"><h3>{version.sourceKey}</h3><p>{version.createdAt} · {version.size.toLocaleString()} 字节</p><details><summary>来源与完整摘要</summary><p>来源档案：{version.profileId}</p><p>版本：{version.versionKey}</p><p>SHA-256：{version.sha256}</p></details>
    {loading&&<LoadFeedback label="正在校验原始字节…"/>}{error&&<div role="alert"><p>{error}</p>{!view&&<button className="secondary-button" onClick={()=>setRetry(value=>value+1)}>重试读取原件</button>}</div>}
    {view&&(view.text===null?<p>原件不是有效 UTF-8；未替换编码。可导出完整原字节核对。</p>:<textarea aria-label="归档原文（只读）" readOnly value={view.text} spellCheck={false}/>)}
    <button type="button" className="secondary-button" disabled={!view||exporting} onClick={()=>void exportOriginal()}>{exporting?"正在导出原件…":"导出原字节"}</button>{notice&&<p role="status">{notice}</p>}
  </section>;
}
