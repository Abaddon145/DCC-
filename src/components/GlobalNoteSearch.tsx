import {useEffect,useState} from "react";
import {api} from "../lib/api";
import {knowledgeApi,type NoteSummary} from "../lib/knowledge";

export function GlobalNoteSearch({query,onOpen}:{query:string;onOpen:(id:string)=>void}){
  const [items,setItems]=useState<NoteSummary[]>([]);const [total,setTotal]=useState(0);const [expanded,setExpanded]=useState(false);
  useEffect(()=>{let active=true;setExpanded(false);setItems([]);setTotal(0);if(!query.trim())return;const timer=setTimeout(()=>{void api.getWorkspaceContext("knowledge").then(v=>knowledgeApi(v.libraryId).search({query,notebookId:null,trash:false,favorite:false,offset:0,limit:5})).then(v=>{if(active){setItems(v.items);setTotal(v.total);}}).catch(()=>{});},250);return()=>{active=false;clearTimeout(timer);};},[query]);
  if(!total)return null;
  return <div style={{position:"relative",flexShrink:0}} onMouseLeave={()=>setExpanded(false)}><button title="查看知识库搜索结果" onClick={()=>setExpanded(v=>!v)}>笔记 · {total}</button>{expanded&&<div role="region" aria-label="笔记搜索结果" style={{position:"absolute",right:0,top:"100%",zIndex:30,width:280,background:"var(--panel)",border:"1px solid var(--border)",borderRadius:8,padding:10,boxShadow:"0 12px 30px #0007"}}><strong>知识库 · {total} 篇</strong>{items.map(v=><button key={v.id} style={{display:"block",width:"100%",textAlign:"left",padding:10}} onClick={()=>onOpen(v.id)}>{v.title}</button>)}</div>}</div>;
}
