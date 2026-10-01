import {useEffect,useMemo,useState} from "react";
import type {Editor} from "@tiptap/react";
import {Extension} from "@tiptap/react";
import {Plugin,PluginKey} from "@tiptap/pm/state";
import {Decoration,DecorationSet} from "@tiptap/pm/view";
import {knowledgeApi,type NoteSummary} from "../lib/knowledge";

const searchKey=new PluginKey<DecorationSet>("noteFind");
export const NoteSearchHighlights=Extension.create({name:"noteSearchHighlights",addProseMirrorPlugins(){return [new Plugin({key:searchKey,state:{init:()=>DecorationSet.empty,apply(tr,previous){const ranges=tr.getMeta(searchKey) as Array<{from:number;to:number}>|undefined;return ranges?DecorationSet.create(tr.doc,ranges.map(range=>Decoration.inline(range.from,range.to,{class:"note-search-hit"}))):previous.map(tr.mapping,tr.doc);}},props:{decorations:state=>searchKey.getState(state)}})];}});

export function noteTextMatches(editor:Editor,query:string){
  const matches:Array<{from:number;to:number;block:number}>=[];
  if(!query)return matches;
  let block=-1;
  editor.state.doc.descendants((node,pos,parent)=>{
    if(parent===editor.state.doc)block++;
    if(!node.isTextblock)return;
    const text=node.textBetween(0,node.content.size,"\n","\ufffc").toLocaleLowerCase();
    const needle=query.toLocaleLowerCase();let at=text.indexOf(needle);
    while(at>=0){matches.push({from:pos+1+at,to:pos+1+at+needle.length,block});at=text.indexOf(needle,at+Math.max(1,needle.length));}
    return false;
  });return matches;
}

export function NoteFindBar({editor,onClose}:{editor:Editor;onClose:()=>void}){
  const [query,setQuery]=useState("");const [revision,setRevision]=useState(0);const [index,setIndex]=useState(0);
  useEffect(()=>{const update=()=>setRevision(v=>v+1);editor.on("update",update);return()=>{editor.off("update",update);};},[editor]);
  const matches=useMemo(()=>noteTextMatches(editor,query),[editor,query,revision]);
  useEffect(()=>{editor.view.dispatch(editor.state.tr.setMeta(searchKey,matches).setMeta("addToHistory",false));return()=>{if(!editor.isDestroyed)editor.view.dispatch(editor.state.tr.setMeta(searchKey,[]).setMeta("addToHistory",false));};},[editor,matches]);
  const jump=(step:number)=>{if(!matches.length)return;const next=(index+step+matches.length)%matches.length;setIndex(next);const match=matches[next];editor.commands.setTextSelection(match);editor.view.dom.dispatchEvent(new CustomEvent("note-select-block",{bubbles:true,detail:match.block}));const dom=editor.view.domAtPos(match.from).node;const element=dom instanceof Element?dom:dom.parentElement;element?.scrollIntoView?.({block:"center"});};
  return <div className="note-find-bar" role="search" aria-label="文档内搜索" onKeyDown={event=>{if(event.key==="Escape"){event.preventDefault();event.stopPropagation();onClose();}if(event.key==="Enter"){event.preventDefault();jump(event.shiftKey?-1:1);}}}><input autoFocus aria-label="查找当前笔记" value={query} onChange={e=>{setQuery(e.target.value);setIndex(-1);}}/><span>{matches.length?`${Math.max(0,index)+1} / ${matches.length}`:"0 / 0"}</span><button disabled={!matches.length} onClick={()=>jump(-1)}>上一处</button><button disabled={!matches.length} onClick={()=>jump(1)}>下一处</button><button onClick={onClose} aria-label="关闭文档搜索">关闭</button></div>;
}

export function NoteGlobalSearch({libraryId,onClose,onOpen}:{libraryId:string;onClose:()=>void;onOpen:(id:string)=>void}){
  const client=useMemo(()=>knowledgeApi(libraryId),[libraryId]);
  const [query,setQuery]=useState("");const [page,setPage]=useState(0);const [items,setItems]=useState<NoteSummary[]>([]);const [total,setTotal]=useState(0);const [error,setError]=useState("");const [loading,setLoading]=useState(false);
  useEffect(()=>{let active=true;setLoading(true);const timer=setTimeout(()=>{void client.search({query,notebookId:null,trash:false,favorite:false,offset:page*40,limit:40}).then(value=>{if(active){setItems(value.items);setTotal(value.total);setError("");}}).catch(e=>{if(active)setError(String(e));}).finally(()=>{if(active)setLoading(false);});},180);return()=>{active=false;clearTimeout(timer);};},[client,query,page]);
  return <div className="modal-backdrop" onPointerDown={e=>{if(e.target===e.currentTarget)onClose();}}><section className="modal-card note-global-search" role="dialog" aria-modal="true" aria-label="搜索全部笔记" onKeyDown={e=>{if(e.key==="Escape"){e.preventDefault();e.stopPropagation();onClose();}}}><header><h2>搜索全部笔记</h2><button onClick={onClose}>关闭</button></header><input autoFocus aria-label="全局笔记搜索" placeholder="搜索标题、正文与标签" value={query} onChange={e=>{setQuery(e.target.value);setPage(0);}}/>{error&&<p role="alert">{error}</p>}<div aria-busy={loading}>{items.map(note=><button key={note.id} onClick={()=>onOpen(note.id)}><strong>{note.title}</strong><span>{note.plainText}</span></button>)}</div><footer><button disabled={loading||!page} onClick={()=>setPage(v=>v-1)}>上一页</button><span>{total} 篇 · {page+1}/{Math.max(1,Math.ceil(total/40))}</span><button disabled={loading||(page+1)*40>=total} onClick={()=>setPage(v=>v+1)}>下一页</button></footer></section></div>;
}
