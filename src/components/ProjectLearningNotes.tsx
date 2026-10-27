import {useCallback,useEffect,useRef,useState} from "react";
import {knowledgeApi,type NoteSummary} from "../lib/knowledge";
import {useRequestGate} from "../lib/workspace";
import type {Page} from "../types";
import {learningApi} from "../features/learning/api";
import type {ProjectLearningNote} from "../features/learning/types";
import {LoadFeedback} from "./LoadFeedback";
export interface ProjectLearningNotesProps {libraryId:string;projectId:string;archived:boolean;onOpenNote?:(id:string)=>void;notify:(message:string,error?:boolean)=>void}
export function ProjectLearningNotes(props:ProjectLearningNotesProps){return <ProjectNotes key={`${props.libraryId}:${props.projectId}`} {...props}/>;}
function ProjectNotes({libraryId,projectId,archived,onOpenNote,notify}:ProjectLearningNotesProps){
  const [page,setPage]=useState<Page<ProjectLearningNote>>({items:[],total:0,offset:0,limit:50}),[offset,setOffset]=useState(0);
  const [loading,setLoading]=useState(false),[error,setError]=useState(""),[busy,setBusy]=useState(false),[picking,setPicking]=useState(false);
  const [query,setQuery]=useState(""),[results,setResults]=useState<NoteSummary[]>([]),[resultTotal,setResultTotal]=useState(0),[resultOffset,setResultOffset]=useState(0),[searching,setSearching]=useState(false),[searchError,setSearchError]=useState(""),[searchRetry,setSearchRetry]=useState(0);
  const readGate=useRequestGate(),searchGate=useRequestGate(),changeGate=useRequestGate(),busyRef=useRef(false);
  const load=useCallback(async()=>{
    const ticket=readGate.begin();setLoading(true);setError("");
    try{const result=await learningApi.listProjectNotes(libraryId,projectId,offset,50);if(readGate.isCurrent(ticket)){if(offset>0&&!result.items.length&&offset>=result.total){setOffset(result.total?Math.floor((result.total-1)/50)*50:0);}else setPage(result);}}
    catch(reason){if(readGate.isCurrent(ticket))setError(`读取学习资料失败：${String(reason)}`);}
    finally{if(readGate.isCurrent(ticket))setLoading(false);}
  },[libraryId,projectId,offset,readGate]);
  useEffect(()=>{void load();return()=>readGate.invalidate();},[load,readGate]);
  useEffect(()=>{
    if(!picking||archived)return;const ticket=searchGate.begin();setSearching(true);setSearchError("");setResults([]);
    const timer=window.setTimeout(()=>{void knowledgeApi(libraryId).search({query,notebookId:null,trash:false,favorite:false,offset:resultOffset,limit:50}).then(value=>{if(searchGate.isCurrent(ticket)){setResults(value.items);setResultTotal(value.total);}}).catch(reason=>{if(searchGate.isCurrent(ticket))setSearchError(`搜索笔记失败：${String(reason)}`);}).finally(()=>{if(searchGate.isCurrent(ticket))setSearching(false);});},200);
    return()=>{window.clearTimeout(timer);searchGate.invalidate();};
  },[libraryId,picking,archived,query,resultOffset,searchGate,searchRetry]);
  const change=async(id:string,link:boolean)=>{
    if(busyRef.current)return;busyRef.current=true;setBusy(true);setError("");const ticket=changeGate.begin();
    try{if(link)await learningApi.linkProjectNote(libraryId,projectId,id);else await learningApi.unlinkProjectNote(libraryId,projectId,id);
      if(changeGate.isCurrent(ticket)){await load();if(changeGate.isCurrent(ticket))notify(link?"已关联笔记":"已解除关联，笔记与媒体保留");}
    }catch(reason){if(changeGate.isCurrent(ticket))setError(`操作失败：${String(reason)}`);}
    finally{busyRef.current=false;if(changeGate.isCurrent(ticket))setBusy(false);}
  };
  return <section className="project-panel project-learning-notes" role="region" aria-label="项目学习资料">
    <header><div><h2>学习资料</h2><p>集中查看关联笔记；解除关联不会删除笔记或媒体。</p></div><div className="learning-panel-actions"><button className="secondary-button" disabled={busy||loading} onClick={()=>void load()}>刷新学习资料</button><button className="secondary-button" disabled={busy||archived} aria-expanded={picking} onClick={()=>setPicking(v=>!v)}>{picking?"收起笔记选择":"关联现有笔记"}</button></div></header>
    {!onOpenNote&&<p>请在功能模块中启用知识库后打开笔记。</p>}
    {error&&<p role="alert" className="learning-analysis-alert">{error}</p>}{loading&&<LoadFeedback label="正在读取学习资料…"/>}
    {!loading&&!error&&!page.total&&<p>还没有关联笔记，可从学习摘录保存到项目，或关联已有笔记。</p>}
    <ul className="project-learning-list">{page.items.map(note=><li key={note.id}><div><button className="secondary-button" disabled={!onOpenNote||busy} aria-label={`打开笔记：${note.title}`} onClick={()=>onOpenNote?.(note.id)}><strong>{note.title}</strong></button><small>{note.courseCount?`关联 ${note.courseCount} 个课程`:"普通笔记"}</small></div><button className="secondary-button" disabled={busy} aria-label={`解除关联：${note.title}`} onClick={()=>void change(note.id,false)}>解除关联</button></li>)}</ul>
    {page.total>50&&<nav aria-label="学习资料分页"><button className="secondary-button" disabled={busy||loading||!offset} onClick={()=>setOffset(v=>Math.max(0,v-50))}>上一页</button><span>{Math.floor(offset/50)+1} / {Math.ceil(page.total/50)}</span><button className="secondary-button" disabled={busy||loading||offset+page.items.length>=page.total} onClick={()=>setOffset(v=>v+50)}>下一页</button></nav>}
    {picking&&!archived&&<section aria-label="选择现有笔记"><label>搜索笔记<input type="search" value={query} onChange={event=>{setQuery(event.target.value);setResultOffset(0);}}/></label>{searching&&<LoadFeedback label="正在搜索笔记…"/>}{searchError&&<p role="alert">{searchError}<button className="secondary-button" onClick={()=>setSearchRetry(v=>v+1)}>重试搜索</button></p>}
      <ul className="project-learning-list">{results.map(note=><li key={note.id}><span>{note.title}</span><button className="secondary-button" disabled={busy} aria-label={`关联笔记：${note.title}`} onClick={()=>void change(note.id,true)}>关联</button></li>)}</ul>
      {resultTotal>50&&<nav aria-label="搜索笔记分页"><button className="secondary-button" disabled={busy||searching||!resultOffset} onClick={()=>setResultOffset(v=>Math.max(0,v-50))}>上一页</button><button className="secondary-button" disabled={busy||searching||resultOffset+results.length>=resultTotal} onClick={()=>setResultOffset(v=>v+50)}>下一页</button></nav>}
    </section>}
  </section>;
}
