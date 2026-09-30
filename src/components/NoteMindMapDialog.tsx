import {useEffect,useRef,useState} from "react";
import MindElixir from "mind-elixir";
import "mind-elixir/style.css";
import {normalizeNoteMap,type NoteMapData} from "../lib/noteMindMap";
export default function NoteMindMapDialog({data,readOnly,onChange,onClose,registerDraft,onExport,onSave}:{data:NoteMapData;readOnly:boolean;onChange:(data:NoteMapData)=>void;onClose:()=>void;registerDraft:(commit:(()=>void)|null)=>void;onExport:(format:"svg"|"png",data:NoteMapData)=>Promise<void>;onSave:()=>Promise<void>}){
  const host=useRef<HTMLDivElement>(null);const instance=useRef<MindElixir|null>(null);
  const initial=useRef(data);const update=useRef(onChange);update.current=onChange;
  const [error,setError]=useState("");const [exporting,setExporting]=useState(false);
  const exportMap=(format:"svg"|"png")=>{if(!instance.current||exporting)return;(document.activeElement as HTMLElement|null)?.blur();if(!commit())return;setExporting(true);void onExport(format,normalizeNoteMap(instance.current.getData())).catch(e=>setError(String(e))).finally(()=>setExporting(false));};
  const commit=()=>{try{if(instance.current&&!readOnly){const next=normalizeNoteMap(instance.current.getData());initial.current=next;update.current(next);}return true;}catch(e){setError(String(e));return false;}};
  useEffect(()=>{registerDraft(()=>{(document.activeElement as HTMLElement|null)?.blur();if(!commit())throw new Error("思维导图未能保存，请检查节点内容");});return()=>registerDraft(null);});
  useEffect(()=>{
    if(!host.current)return;
    const mind=new MindElixir({el:host.current,editable:!readOnly,contextMenu:readOnly?false:{link:false,focus:false},toolBar:false,allowUndo:true,theme:MindElixir.DARK_THEME,locale:"zh_CN"});
    instance.current=mind;
    try{const result=mind.init(normalizeNoteMap(initial.current));if(result)throw result;mind.scaleFit();}catch(e){setError(String(e));}
    mind.bus.addListener("operation",event=>{if(event.name!=="beginEdit")commit();});
    mind.bus.addListener("expandNode",()=>commit());
    return()=>{instance.current=null;mind.destroy();};
  },[readOnly]);
  const close=()=>{(document.activeElement as HTMLElement|null)?.blur();if(commit())onClose();};
  return <div className="modal-backdrop note-map-backdrop" onPointerDown={event=>{if(event.target===event.currentTarget&&!exporting)close();}}><section className="note-map-dialog" role="dialog" aria-modal="true" aria-label="思维导图" onKeyDown={event=>{event.stopPropagation();if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="s"){event.preventDefault();if(commit())void onSave().catch(e=>setError(String(e)));}if(event.key==="Escape"&&!exporting)close();}}>
    <header><div><strong>思维导图</strong><span>{readOnly?"阅读模式":"双击编辑 · Tab 新建子节点 · Enter 新建同级节点"}</span></div><div>{!readOnly&&<><button onClick={()=>{instance.current?.undo();commit();}}>撤销</button><button onClick={()=>{instance.current?.redo();commit();}}>重做</button></>}<button onClick={()=>instance.current?.scaleFit()}>适应窗口</button><button disabled={exporting} onClick={()=>exportMap("svg")}>导出 SVG</button><button disabled={exporting} onClick={()=>exportMap("png")}>导出 PNG</button><button disabled={exporting} onClick={close}>关闭</button></div></header>
    {error&&<p role="alert">{error}</p>}<div ref={host} className="note-map-canvas"/>
  </section></div>;
}
