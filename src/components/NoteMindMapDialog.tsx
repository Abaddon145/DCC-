import {useEffect,useRef,useState} from "react";
import MindElixir from "mind-elixir";
import "mind-elixir/style.css";
import {normalizeNoteMap,type NoteMapData,type NoteMapNode,type NoteMapFormat} from "../lib/noteMindMap";
import {activeNoteShortcutAction} from "../lib/noteShortcutKeys";
import {invoke} from "@tauri-apps/api/core";
import {api} from "../lib/api";

function displayData(data:NoteMapData){
  const visit=(node:NoteMapNode):NoteMapNode&{style:Record<string,string>;hyperLink?:string}=>{
    const f=node.metadata?.format||{};
    return {...node,style:{fontSize:`${f.heading?[0,26,22,18][f.heading]:16}px`,fontWeight:f.bold||f.heading?"bold":"normal",fontFamily:f.code?"monospace":"inherit",textDecoration:[f.underline?"underline":"",f.strike?"line-through":""].filter(Boolean).join(" ")||"none"},...(f.link?{hyperLink:f.link}:{}),children:node.children?.map(visit)};
  };return {...data,nodeData:visit(data.nodeData)};
}
export default function NoteMindMapDialog({data,readOnly,onChange,onClose,registerDraft,onExport,onSave}:{data:NoteMapData;readOnly:boolean;onChange:(data:NoteMapData)=>void;onClose:()=>void;registerDraft:(commit:(()=>void)|null)=>void;onExport:(format:"svg"|"png",data:NoteMapData)=>Promise<void>;onSave:()=>Promise<void>}){
  const host=useRef<HTMLDivElement>(null);const instance=useRef<MindElixir|null>(null);
  const initial=useRef(normalizeNoteMap(data));const update=useRef(onChange);update.current=onChange;
  const history=useRef<NoteMapData[]>([]);const future=useRef<NoteMapData[]>([]);
  const [error,setError]=useState("");const [exporting,setExporting]=useState(false);
  const [find,setFind]=useState(false);const [query,setQuery]=useState("");const [match,setMatch]=useState(0);
  const decorate=()=>{const mind=instance.current;if(!mind)return;const visit=(node:NoteMapNode)=>{const el=mind.findEle(node.id);if(el)el.style.fontStyle=node.metadata?.format.italic?"italic":"normal";node.children?.forEach(visit);};visit(normalizeNoteMap(mind.getData()).nodeData);};
  const commit=()=>{try{if(instance.current&&!readOnly){const next=normalizeNoteMap(instance.current.getData());if(JSON.stringify(next)!==JSON.stringify(initial.current)){history.current.push(initial.current);history.current=history.current.slice(-50);future.current=[];initial.current=next;update.current(next);}decorate();}return true;}catch(e){setError(String(e));return false;}};
  const replace=(next:NoteMapData)=>{initial.current=normalizeNoteMap(next);instance.current?.refresh(displayData(initial.current));decorate();update.current(initial.current);};
  const undo=(redo=false)=>{if(readOnly)return;const from=redo?future.current:history.current;const to=redo?history.current:future.current;const previous=from.pop();if(previous){to.push(initial.current);replace(previous);}};
  const exportMap=(format:"svg"|"png")=>{if(!instance.current||exporting)return;(document.activeElement as HTMLElement|null)?.blur();if(!commit())return;setExporting(true);void onExport(format,normalizeNoteMap(instance.current.getData())).catch(e=>setError(String(e))).finally(()=>setExporting(false));};
  useEffect(()=>{registerDraft(()=>{(document.activeElement as HTMLElement|null)?.blur();if(!commit())throw new Error("思维导图未能保存，请检查节点内容");});return()=>registerDraft(null);});
  useEffect(()=>{
    if(!host.current)return;
    const mind=new MindElixir({el:host.current,editable:!readOnly,keypress:false,contextMenu:readOnly?false:{link:false,focus:false},toolBar:false,allowUndo:false,theme:MindElixir.DARK_THEME,locale:"zh_CN"});
    instance.current=mind;
    try{const result=mind.init(displayData(initial.current));if(result)throw result;mind.scaleFit();decorate();}catch(e){setError(String(e));}
    mind.bus.addListener("operation",event=>{if(event.name!=="beginEdit")commit();});
    mind.bus.addListener("expandNode",()=>commit());
    return()=>{instance.current=null;mind.destroy();};
  },[readOnly]);
  const close=()=>{(document.activeElement as HTMLElement|null)?.blur();if(commit())onClose();};
  const jump=(step:number)=>{
    const mind=instance.current;if(!mind)return;const matches:string[]=[];
    const visit=(node:NoteMapNode)=>{if(query&&node.topic.toLocaleLowerCase().includes(query.toLocaleLowerCase()))matches.push(node.id);node.children?.forEach(visit);};visit(normalizeNoteMap(mind.getData()).nodeData);
    if(!matches.length){setError("没有匹配的主题");return;}const index=(match+step+matches.length)%matches.length;setMatch(index);const el=mind.findEle(matches[index]);mind.selectNode(el);mind.scrollIntoView(el,true);setError("");
  };
  const key=async(event:React.KeyboardEvent)=>{
    if(event.nativeEvent.isComposing)return;
    const mind=instance.current;if(!mind)return;
    const id=activeNoteShortcutAction(event.nativeEvent);
    const typing=!!(event.target as Element).closest("input,textarea,[contenteditable=true]");
    if(typing&&!["noteUndo","noteRedo","noteFind","noteZoomIn","noteZoomOut"].includes(id||""))return;
    if(!id)return;event.preventDefault();event.stopPropagation();
    try{
      if(id==="noteFind"){setFind(true);return;}
      if(id==="noteZoomIn"||id==="noteZoomOut"){mind.scale(Math.max(.2,Math.min(3,mind.scaleVal+(id==="noteZoomIn"?.1:-.1))));return;}
      if(id==="noteGlobalFind"){if(commit()){await onSave();host.current?.dispatchEvent(new CustomEvent("note-global-search",{bubbles:true}));onClose();}return;}
      if(readOnly)return;
      if(id==="noteScreenshot"){await invoke("start_note_screenshot");setError("截取后关闭独立导图，返回笔记正文用 Ctrl+V 插入图片");return;}
      if(id==="noteUndo"||id==="noteRedo"){undo(id==="noteRedo");return;}
      const node=mind.currentNode;if(!node)throw Error("请先选择主题");
      if(id==="noteSibling"){if(!node.nodeObj.parent)await mind.addChild(node);else await mind.insertSibling("after",node);return;}
      if(id==="noteIndent"){const parent=node.nodeObj.parent;const index=parent?.children?.findIndex(n=>n.id===node.nodeObj.id)??-1;if(!parent||index<=0)throw Error("没有可缩进的前一个同级主题");await mind.moveNodeIn([node],mind.findEle(parent.children![index-1].id));return;}
      if(id==="notePromote"){const parent=node.nodeObj.parent;if(!parent||!parent.parent)throw Error("已经位于根级");await mind.moveNodeAfter([node],mind.findEle(parent.id));return;}
      const current=normalizeNoteMap(mind.getData());
      let selected:NoteMapNode|undefined;let siblings:NoteMapNode[]=[];
      const visit=(n:NoteMapNode,group:NoteMapNode[])=>{if(n.id===node.nodeObj.id){selected=n;siblings=group;}n.children?.forEach(c=>visit(c,n.children!));};visit(current.nodeData,[current.nodeData]);
      if(!selected)return;
      const heading=/^note(Siblings)?(Heading([123])|Paragraph)$/.exec(id);
      const targets=heading?.[1]?siblings:[selected];
      const mark:Record<string,keyof NoteMapFormat>={noteBold:"bold",noteItalic:"italic",noteUnderline:"underline",noteStrike:"strike",noteCode:"code"};
      const enabled=mark[id]?!selected.metadata?.format[mark[id]]:false;
      let link:string|undefined;
      if(id==="noteLink"){const value=window.prompt("链接地址（http/https），留空移除",selected.metadata?.format.link||"");if(value===null)return;if(value.trim()){const url=new URL(value);if(!["http:","https:"].includes(url.protocol))throw Error("链接仅支持 http/https");link=url.href;}}
      if(!heading&&!mark[id]&&id!=="noteLink")return;
      for(const item of targets){const format={...item.metadata?.format};if(heading)format.heading=Number(heading[3]||0);if(mark[id])(format as Record<string,unknown>)[mark[id]]=enabled;if(id==="noteLink"){if(link)format.link=link;else delete format.link;}item.metadata={format};}
      history.current.push(initial.current);history.current=history.current.slice(-50);future.current=[];replace(current);
    }catch(e){setError(String(e));}
  };
  return <div className="modal-backdrop note-map-backdrop" onPointerDown={event=>{if(event.target===event.currentTarget&&!exporting)close();}}><section className="note-map-dialog" role="dialog" aria-modal="true" aria-label="思维导图" onKeyDownCapture={event=>{void key(event);}} onKeyDown={event=>{event.stopPropagation();if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="s"){event.preventDefault();if(commit())void onSave().catch(e=>setError(String(e)));}if(event.key==="Escape"&&!exporting)close();}}>
    <header><div><strong>思维导图</strong><span>{readOnly?"阅读模式":"双击编辑 · Enter 同级 · Tab 缩进 · Shift+Tab 提升"}</span></div><div>{!readOnly&&<><button onClick={()=>undo()}>撤销</button><button onClick={()=>undo(true)}>重做</button></>}<button onClick={()=>instance.current?.scaleFit()}>适应窗口</button><button disabled={exporting} onClick={()=>exportMap("svg")}>导出 SVG</button><button disabled={exporting} onClick={()=>exportMap("png")}>导出 PNG</button><button disabled={exporting} onClick={close}>关闭</button></div></header>
    {find&&<div className="note-find-bar"><input autoFocus aria-label="查找导图主题" value={query} onChange={e=>{setQuery(e.target.value);setMatch(-1);}} onKeyDown={e=>{if(e.key==="Enter"){e.preventDefault();jump(e.shiftKey?-1:1);}if(e.key==="Escape"){e.stopPropagation();setFind(false);}}}/><button onClick={()=>jump(-1)}>上一处</button><button onClick={()=>jump(1)}>下一处</button><button onClick={()=>setFind(false)}>关闭查找</button></div>}
    {error&&<p role="alert">{error}</p>}<div ref={host} className="note-map-canvas" onClickCapture={event=>{const link=(event.target as Element).closest("a");if(link){event.preventDefault();event.stopPropagation();const href=link.getAttribute("href");if(href&&/^https?:\/\//i.test(href))void api.openExternal(href).catch(e=>setError(String(e)));}}}/>
  </section></div>;
}
