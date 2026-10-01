import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { knowledgeApi, type KnowledgeNote, type NoteSummary } from "../lib/knowledge";

interface Source {id:string;revision:number;title:string;text:string}
interface Answer {text:string;citations:string[];insufficientEvidence:boolean}
export function NoteAiDialog({libraryId,note,selection,onClose,onApply,onNavigate}:{libraryId:string;note:KnowledgeNote;selection:string;onClose:()=>void;onApply:(text:string)=>void;onNavigate:(id:string)=>void}) {
  const client=useMemo(()=>knowledgeApi(libraryId),[libraryId]);
  const [action,setAction]=useState("summarize");const [prompt,setPrompt]=useState("");const [query,setQuery]=useState("");const [candidates,setCandidates]=useState<NoteSummary[]>([]);
  const [sources,setSources]=useState<Source[]>([{id:note.id,revision:note.revision,title:note.title,text:selection||note.plainText}]);
  const [answer,setAnswer]=useState<Answer|null>(null);const [error,setError]=useState("");const [busy,setBusy]=useState(false);const [loading,setLoading]=useState(false);
  const requestId=useRef<string|null>(null);const mounted=useRef(true);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;if(requestId.current)void invoke("cancel_ai_session",{id:requestId.current}).catch(()=>{});};},[]);
  useEffect(()=>{if(action!=="question")return;let active=true;const timer=setTimeout(()=>{void client.search({query,notebookId:null,trash:false,favorite:false,offset:0,limit:15}).then(v=>{if(active)setCandidates(v.items);}).catch(e=>{if(active)setError(String(e));});},220);return()=>{active=false;clearTimeout(timer);};},[client,query,action]);
  const count=sources.reduce((sum,v)=>sum+Array.from(v.text).length,0);
  const generate=async()=>{
    const id=crypto.randomUUID();requestId.current=id;setBusy(true);setError("");setAnswer(null);
    try{const value=await invoke<Answer>("generate_note_ai",{request:{requestId:id,libraryId,action,prompt,sources}});if(mounted.current)setAnswer(value);}catch(e){if(mounted.current)setError(String(e));}finally{requestId.current=null;if(mounted.current)setBusy(false);}
  };
  return <div className="modal-backdrop"><section className="modal-card knowledge-ai-dialog" role="dialog" aria-modal="true" aria-label="笔记 AI 辅助"><header><div><span className="eyebrow">DEEPSEEK · 按需发送</span><h2>先确认资料，再请求 AI</h2></div><button onClick={onClose}>关闭</button></header>
    <p>仅把下方可见的标题和文字发送给 DeepSeek，不上传附件、不扫描整个素材库；使用现有凭据及共享月度额度。</p>
    <select aria-label="AI 操作" disabled={busy} value={action} onChange={e=>{setAction(e.target.value);setAnswer(null);}}><option value="summarize">提炼知识要点</option><option value="organize">整理笔记结构</option><option value="review">生成复习问题</option><option value="question">基于笔记问答</option></select>
    <textarea aria-label="AI 要求" maxLength={2000} disabled={busy} value={prompt} onChange={e=>{setPrompt(e.target.value);setAnswer(null);}} placeholder={action==="question"?"输入你想查询的问题":"可选：补充关注点，例如 Unreal Engine 材质制作"}/>
    {action==="question"&&!busy&&<div><input aria-label="本地检索知识" value={query} onChange={e=>setQuery(e.target.value)} placeholder="先在本地搜索相关笔记，再选择发送范围"/><div className="knowledge-ai-candidates">{candidates.filter(v=>!sources.some(s=>s.id===v.id)).map(v=><button key={v.id} disabled={loading||sources.length>=8} onClick={()=>{setLoading(true);void client.get(v.id).then(n=>{if(mounted.current){setSources(items=>[...items,{id:n.id,revision:n.revision,title:n.title,text:n.plainText}]);setAnswer(null);}}).catch(e=>setError(String(e))).finally(()=>setLoading(false));}}>＋ {v.title}</button>)}</div></div>}
    <div className="knowledge-ai-sources">{sources.map(source=><details key={source.id} open><summary>{source.title}<button disabled={busy} onClick={()=>{setSources(items=>items.filter(v=>v.id!==source.id));setAnswer(null);}}>移除</button></summary><pre>{source.text}</pre></details>)}</div>
    <p>{sources.length} 篇笔记 · {count.toLocaleString()} 字 / 24,000 字上限</p>{error&&<p role="alert" className="knowledge-save-error">{error}</p>}
    {answer&&<div className="knowledge-ai-answer"><h3>{answer.insufficientEvidence?"笔记依据不足，请补充资料":"结果预览 · 尚未写入"}</h3><pre>{answer.text}</pre><div>{answer.citations.map(id=><button key={id} onClick={()=>onNavigate(id)}>来源：{sources.find(v=>v.id===id)?.title}</button>)}</div></div>}
    <footer>{busy?<button onClick={()=>{if(requestId.current)void invoke("cancel_ai_session",{id:requestId.current}).catch(()=>{});}}>取消请求</button>:<button className="primary-button" disabled={loading||sources.length===0||sources.some(v=>!v.text.trim())||count>24000||(action==="question"&&!prompt.trim())} onClick={()=>void generate()}>确认发送并生成</button>}{answer&&<button className="primary-button" onClick={()=>onApply(answer.text)}>追加到当前笔记</button>}<button onClick={onClose}>关闭，不写入</button></footer>
  </section></div>;
}
