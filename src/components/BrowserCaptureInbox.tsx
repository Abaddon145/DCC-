import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Inbox, Link, Check, RefreshCw, NotebookPen, FolderInput } from "lucide-react";
import { api, mediaLibraryFileUrl } from "../lib/api";
import type { BrowserCapture, BrowserCaptureState, MediaEntryDetail, MediaFolder } from "../types";
import { LoadFeedback, ReadFailure } from "./LoadFeedback";
import { MediaDetail } from "./MediaLibraryView";
import { MediaAssociationPicker } from "./MediaAssociationPicker";
import "./BrowserCaptureInbox.css";

const statuses = {saved:"文件已保存",duplicate:"重复已跳过",link:"仅来源链接",failed:"处理失败"};
export function BrowserCaptureInbox({ notify, onOpenNote, canAddProject=true, canAddBoard=true }: { notify:(message:string,error?:boolean)=>void; onOpenNote?:(id:string)=>void; canAddProject?:boolean; canAddBoard?:boolean }) {
  const [libraryId,setLibraryId]=useState(""); const [libraryName,setLibraryName]=useState("");
  const [items,setItems]=useState<BrowserCapture[]>([]); const [total,setTotal]=useState(0); const [page,setPage]=useState(0);
  const [loading,setLoading]=useState(true); const [failed,setFailed]=useState(false); const [extensionId,setExtensionId]=useState("");
  const [busy,setBusy]=useState(false); const [connection,setConnection]=useState<BrowserCaptureState|null>(null);
  const [detail,setDetail]=useState<MediaEntryDetail|null>(null); const [association,setAssociation]=useState<{id:string;target:"project"|"board"}|null>(null);
  const [moving,setMoving]=useState<BrowserCapture|null>(null); const [folders,setFolders]=useState<MediaFolder[]>([]); const [folderId,setFolderId]=useState("");
  const [tags,setTags]=useState(""); const mounted=useRef(true); const version=useRef(0); const detailVersion=useRef(0); const moveVersion=useRef(0); const mutation=useRef(false);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;version.current++;detailVersion.current++;moveVersion.current++;};},[]);
  useEffect(()=>{const outside=(event:PointerEvent)=>{if(!(event.target as HTMLElement)?.closest(".media-detail-panel,.lightbox,.modal-backdrop")){detailVersion.current++;setDetail(null);}};document.addEventListener("pointerdown",outside);return()=>document.removeEventListener("pointerdown",outside);},[]);
  const refresh=useCallback(async()=>{
    const request=++version.current;setLoading(true);
    try {
      const context=await api.getWorkspaceContext("browserCapture");
      const result=await api.listBrowserCaptures(context.libraryId,page*100);
      if(mounted.current&&request===version.current){setLibraryId(context.libraryId);setItems(result.items);setTotal(result.total);setFailed(false);}
    } catch(error){if(mounted.current&&request===version.current){setFailed(true);notify(String(error),true);}}
    finally{if(mounted.current&&request===version.current)setLoading(false);}
  },[page,notify]);
  useEffect(()=>{void refresh();},[refresh]);
  useEffect(()=>{
    let active=true; let dispose:(()=>void)|undefined;
    void listen<BrowserCaptureState>("browser-capture-state",event=>{if(active){setConnection(event.payload);setLibraryName(event.payload.name);}}).then(fn=>{if(active)dispose=fn;else fn();});
    let refreshAfter=0;
    const timer=window.setInterval(()=>{if(Date.now()>refreshAfter){refreshAfter=Date.now()+10000;void refresh();}},10000);
    return()=>{active=false;dispose?.();window.clearInterval(timer);};
  },[refresh]);
  const run=async(action:()=>Promise<void>)=>{
    if(mutation.current)return;mutation.current=true;setBusy(true);
    try{await action();}catch(error){if(mounted.current)notify(String(error),true);}finally{mutation.current=false;if(mounted.current)setBusy(false);}
  };
  const open=async(item:BrowserCapture)=>{
    if(!item.entryId||item.unavailable)return;
    const request=++detailVersion.current;
    try{const value=await api.getMediaEntry(item.entryId);if(mounted.current&&request===detailVersion.current)setDetail(value);}catch(error){notify(String(error),true);}
  };
  const move=async(item:BrowserCapture)=>{
    const request=++moveVersion.current;
    const values=await api.listMediaFolders(item.kind==="video"?"video":"image");if(!mounted.current||request!==moveVersion.current)return;
    setFolders(values);setFolderId("");setTags("");setMoving(item);
  };
  const saveMove=()=>run(async()=>{
    if(!moving?.entryId)return;
    await api.batchUpdateMediaEntries({ids:[moving.entryId],folderId:folderId||null,clearFolder:false,addTags:tags.split(/[;；]/).map(t=>t.trim()).filter(Boolean),removeTags:[],favorite:null},libraryId);
    await api.organizeBrowserCapture(libraryId,moving.id);if(mounted.current)setMoving(null);await refresh();notify("已整理；媒体原件仍保存在当前素材库");
  });
  return <section className={`capture-workspace ${detail?"has-preview":""}`}>
    <header className="capture-heading"><div><span className="eyebrow">WEB CAPTURE INBOX</span><h1><Inbox size={23}/>采集收件箱</h1><p>{total} 项 · {libraryName||"当前素材库"} · 文件与来源链接分开标记</p></div><button className="secondary-button" disabled={loading} onClick={()=>void refresh()}><RefreshCw size={15}/>刷新</button></header>
    <details className="capture-connect"><summary>连接 Edge / Chrome 浏览器扩展</summary><p>将配套扩展文件夹通过浏览器扩展页的“加载解压缩的扩展”安装。复制扩展 ID，在这里连接；升级程序后可再次连接。仅注册当前用户，不读取账号或 Cookie。</p><label>扩展 ID<input value={extensionId} onChange={event=>setExtensionId(event.target.value.trim())} placeholder="32 位扩展 ID" maxLength={32}/></label><button className="primary-button" disabled={busy||!/^[a-p]{32}$/.test(extensionId)} onClick={()=>void run(async()=>{notify(await api.registerBrowserCaptureHost(extensionId));})}>{busy?"正在连接…":"连接浏览器"}</button><p>软件未打开时，扩展会启动后台接收；请在扩展确认目标素材库。点击扩展“打开栈藏”即可显示软件。</p></details>
    {!!connection?.otherLibraries&&<p role="status">还有 {connection.otherLibraries} 项属于其他素材库的待处理采集，请切换到原库处理；不会导入到当前库。</p>}
    <p className="capture-hint">从网页右键保存，或用扩展批量选图与区域截图。文件自动进入媒体库；收件箱保留来源和整理状态。受限视频只保存链接，不代表已下载。</p>
    {failed&&<ReadFailure onRetry={()=>void refresh()}/>}{loading&&!items.length&&<LoadFeedback label="正在读取采集内容…" skeleton/>}
    {!loading&&!failed&&!items.length&&<div className="capture-empty"><Inbox size={35}/><h2>把灵感留在这里</h2><p>浏览网页时使用“保存至栈藏”，然后将参考资料整理到项目、笔记或参考板。</p></div>}
    <div className="capture-items">{items.map(item=><article className="capture-item" key={item.id}>
      <button className="capture-thumbnail" disabled={!item.entryId||item.unavailable} onClick={()=>void open(item)} aria-label={`预览 ${item.title}`}>{item.thumbnailId&&!item.unavailable?<img loading="lazy" src={mediaLibraryFileUrl(item.thumbnailId)} alt=""/>:<Link size={24}/>}</button>
      <div className="capture-item-info"><span className={`capture-status status-${item.status}`}>{statuses[item.status]}{item.organized?" · 已整理":" · 待整理"}{item.unavailable?" · 媒体在回收站或已永久删除":""}</span><h2>{item.title}</h2><button className="capture-source" onClick={()=>void api.openExternal(item.pageUrl).catch(error=>notify(String(error),true))}>{item.pageUrl}</button><p>{item.message}</p>
        <div className="capture-actions">{item.entryId&&!item.unavailable&&<><button className="secondary-button" onClick={()=>void open(item)}>{item.status==="duplicate"?"打开已有条目":"打开预览"}</button><button className="secondary-button" disabled={busy} onClick={()=>void move(item).catch(error=>notify(String(error),true))}><FolderInput size={14}/>文件夹与标签</button>{canAddProject&&<button className="secondary-button" onClick={()=>setAssociation({id:item.entryId!,target:"project"})}>加入项目</button>}{canAddBoard&&item.kind==="image"&&<button className="secondary-button" onClick={()=>setAssociation({id:item.entryId!,target:"board"})}>加入参考板</button>}</>}
          {onOpenNote&&<button className="secondary-button" disabled={busy} onClick={()=>void run(async()=>{const id=await api.createBrowserCaptureNote(libraryId,item.id);if(mounted.current)onOpenNote(id);})}><NotebookPen size={14}/>建立学习笔记</button>}
          {!item.organized&&<button className="secondary-button" disabled={busy} onClick={()=>void run(async()=>{await api.organizeBrowserCapture(libraryId,item.id);await refresh();})}><Check size={14}/>标记已整理</button>}
        </div></div>
    </article>)}</div>
    {total>100&&<nav className="capture-pagination" aria-label="采集分页"><button disabled={!page||loading} onClick={()=>setPage(p=>p-1)}>上一页</button><span>{page+1} / {Math.ceil(total/100)}</span><button disabled={(page+1)*100>=total||loading} onClick={()=>setPage(p=>p+1)}>下一页</button></nav>}
    {detail&&<MediaDetail expectedLibraryId={libraryId} canAddProject={canAddProject} canAddBoard={canAddBoard} detail={detail} onClose={()=>{detailVersion.current++;setDetail(null);}} onChanged={async()=>{if(detail){const request=++detailVersion.current;const updated=await api.getMediaEntry(detail.id);if(mounted.current&&request===detailVersion.current)setDetail(updated);await refresh();}}} onNotify={notify}/>}
    {association&&<MediaAssociationPicker expectedLibraryId={libraryId} canAddProject={canAddProject} canAddBoard={canAddBoard} ids={[association.id]} target={association.target} onClose={()=>setAssociation(null)} onAdded={refresh} notify={notify}/>}
    {moving&&<div className="modal-backdrop" onPointerDown={event=>{if(event.target===event.currentTarget&&!busy)setMoving(null);}}><section className="modal-card" role="dialog" aria-modal="true" aria-label="整理采集"><h2>整理到媒体库</h2><p>{moving.title}。更改的是已保存条目的分类，不复制媒体文件。</p><label>文件夹<select value={folderId} onChange={event=>setFolderId(event.target.value)}><option value="">保留当前文件夹</option>{folders.map(folder=><option value={folder.id} key={folder.id}>{folder.name}</option>)}</select></label><label>添加标签<input value={tags} onChange={event=>setTags(event.target.value)} placeholder="分号分隔"/></label><footer><button className="secondary-button" disabled={busy} onClick={()=>setMoving(null)}>取消</button><button className="primary-button" disabled={busy} onClick={()=>void saveMove()}>{busy?"正在保存…":"保存整理"}</button></footer></section></div>}
  </section>;
}
