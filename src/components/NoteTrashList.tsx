import { useCallback, useEffect, useMemo, useState } from "react";
import { BookOpen } from "lucide-react";
import { api } from "../lib/api";
import { knowledgeApi, type NoteSummary } from "../lib/knowledge";

export function NoteTrashList({notify,onOpen}:{notify:(text:string,error?:boolean)=>void;onOpen?:(id:string)=>void}) {
  const [libraryId,setLibraryId]=useState("");
  const [items,setItems]=useState<NoteSummary[]>([]),[total,setTotal]=useState(0),[page,setPage]=useState(0);
  const [loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[refresh,setRefresh]=useState(0);
  useEffect(()=>{let active=true;void api.getWorkspaceContext("knowledge").then(value=>{if(active)setLibraryId(value.libraryId)}).catch(error=>{if(active){setLoading(false);notify(String(error),true)}});return()=>{active=false}},[notify]);
  const client=useMemo(()=>knowledgeApi(libraryId),[libraryId]);
  useEffect(()=>{if(!libraryId)return;let active=true;setLoading(true);void client.search({query:"",notebookId:null,trash:true,favorite:false,offset:page*40,limit:40}).then(value=>{if(active){setItems(value.items);setTotal(value.total);if(page&&value.items.length===0)setPage(Math.max(0,Math.ceil(value.total/40)-1))}}).catch(error=>{if(active)notify(String(error),true)}).finally(()=>{if(active)setLoading(false)});return()=>{active=false}},[client,libraryId,page,refresh,notify]);
  const change=useCallback(async(item:NoteSummary,purge:boolean)=>{
    if(purge&&!window.confirm(`永久删除笔记“${item.title}”、全部历史版本和附件？此操作无法撤销。`))return;
    setBusy(true);
    try{const note=await client.get(item.id);if(purge)await client.purge(note);else await client.setDeleted(note,false);setRefresh(value=>value+1);notify(purge?"笔记已永久删除":"笔记已恢复")}
    catch(error){notify(String(error),true)}finally{setBusy(false)}
  },[client,notify]);
  return <section className="note-trash-section"><header><h2>已删除笔记</h2><span>{total} 篇 · 历史版本和附件保留至永久删除</span></header>{loading?<p role="status">正在读取笔记回收站…</p>:!items.length?<p>没有已删除的笔记</p>:<div className="trash-list">{items.map(item=><article key={item.id}><BookOpen size={22}/><div><strong>{item.title}</strong><span>{item.plainText.slice(0,120)}</span><time>{new Date(item.updatedAt).toLocaleString("zh-CN")}</time></div>{onOpen&&<button disabled={busy} className="secondary-button" onClick={()=>onOpen(item.id)}>查看笔记</button>}<button disabled={busy} className="secondary-button" onClick={()=>void change(item,false)}>恢复笔记</button><button disabled={busy} className="secondary-button danger" onClick={()=>void change(item,true)}>永久删除笔记</button></article>)}</div>}<footer className="project-tools"><button disabled={loading||busy||page===0} onClick={()=>setPage(page-1)}>上一页笔记</button><span>{page+1} / {Math.max(1,Math.ceil(total/40))}</span><button disabled={loading||busy||(page+1)*40>=total} onClick={()=>setPage(page+1)}>下一页笔记</button></footer></section>;
}
