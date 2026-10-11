import { useEffect,useState } from "react";
import { knowledgeApi,type RecoveryDraft } from "../lib/knowledge";
export function NoteRecovery({libraryId,id,onOpen,notify,onPending}:{libraryId:string;id?:string;onOpen:(id:string)=>void;notify:(message:string,error?:boolean)=>void;onPending?:(pending:boolean)=>void}){
  const [failed,setFailed]=useState(false),[revision,setRevision]=useState(0);
  const [ids,setIds]=useState<string[]>([]);const [draft,setDraft]=useState<RecoveryDraft|null>(null);const [busy,setBusy]=useState(false);
  const client=knowledgeApi(libraryId);
  useEffect(()=>{let alive=true;setFailed(false);if(id){onPending?.(true);void client.recovery(id).then(v=>{if(alive){setDraft(v);onPending?.(!!v)}}).catch(e=>{if(alive){setFailed(true);notify(String(e),true)}});}else{void client.recoveryNotes().then(v=>{if(alive)setIds(Array.isArray(v)?v:[])}).catch(e=>notify(String(e),true));}return()=>{alive=false}},[libraryId,id,revision]);
  if(!id)return ids.length?<section className="read-failure" role="status"><span>{ids.length} 篇笔记有恢复副本</span><button onClick={()=>onOpen(ids[0])}>检查恢复副本</button></section>:null;
  if(failed)return <section className="read-failure"><span>无法检查恢复副本，编辑已暂停；原文件保留。</span><button onClick={()=>setRevision(v=>v+1)}>重试恢复检查</button></section>;
  if(!draft)return null;
  const recover=async()=>{if(busy)return;setBusy(true);try{const value=await client.restoreRecoveryCopy(id);await client.clearRecovery(id);setDraft(null);onPending?.(false);onOpen(value.id);notify("已恢复为新笔记，原笔记未被覆盖")}catch(e){notify(String(e),true)}finally{setBusy(false)}};
  return <section className="read-failure" role="status"><span>发现 {new Date(draft.savedAt).toLocaleString("zh-CN")} 的恢复副本：{draft.input.title}</span><button disabled={busy} onClick={()=>void recover()}>恢复为新笔记</button><button disabled={busy} onClick={()=>{if(window.confirm("丢弃这份恢复副本？原笔记和历史不会删除。")){setBusy(true);void client.clearRecovery(id).then(()=>{setDraft(null);onPending?.(false)}).catch(e=>notify(String(e),true)).finally(()=>setBusy(false));}}}>丢弃恢复副本</button></section>;
}
