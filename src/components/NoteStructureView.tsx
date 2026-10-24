import {hasLearningAtoms} from "../lib/learningNoteNodes";
import {useEffect,useMemo,useRef,useState} from "react";
import {EditorContent,useEditor,type Editor,type JSONContent} from "@tiptap/react";
import {Fragment} from "@tiptap/pm/model";
import {closeHistory} from "@tiptap/pm/history";
import MindElixir from "mind-elixir";
import "mind-elixir/style.css";
import {blockText,findStructure,structureAt,normalizeStructure,moveStructure,noteStructure,removeStructure,structureMap,insertStructureSibling,insertStructureChild,indentStructure,formatStructure,type StructureNode} from "../lib/noteStructure";
import {activeNoteShortcutAction,applyNoteFormat} from "../lib/noteShortcuts";
import {beginPointerReorder} from "../lib/drag";
import {noteTemplates,templateDocument,type NoteTemplate} from "../lib/knowledge";
import {ImageLightbox} from "./ImageLightbox";
import {NoteEditorTools} from "./NoteEditorTools";
import {copyNoteSelection,pasteNoteSelection} from "../lib/noteClipboard";

export default function NoteStructureView({editor,document:doc,title,layout,disabled,libraryId,noteId,onInsert,onAi,onExport,notify,trackImport}:{editor:Editor;document:JSONContent;title:string;layout:"outline"|"mindMap";disabled:boolean;libraryId:string;noteId:string;onInsert:(kind:"attachment"|"reference"|"screenshot")=>void;onAi:()=>void;onExport:(format:"svg"|"png")=>void;notify:(message:string,error?:boolean)=>void;trackImport:(work:Promise<unknown>)=>Promise<unknown>}){
  const root=useMemo(()=>noteStructure(doc,title),[doc,title]);const [selected,setSelected]=useState("root");const [folded,setFolded]=useState(new Set<string>());
  const [imagePreview,setImagePreview]=useState<{images:string[];index:number}|null>(null);
  const [direction,setDirection]=useState<1|2|3>([1,2,3].includes(doc.attrs?.mapDirection)?doc.attrs!.mapDirection:2);const [details,setDetails]=useState(true);const [mapTheme,setMapTheme]=useState(["graphite","ocean","forest"].includes(doc.attrs?.mapTheme)?doc.attrs!.mapTheme:"graphite");
  const selectedRef=useRef(selected);selectedRef.current=selected;
  const [template,setTemplate]=useState<NoteTemplate>("tutorial");const host=useRef<HTMLDivElement>(null);const mind=useRef<MindElixir|null>(null);const latest=useRef({root,doc,disabled});latest.current={root,doc,disabled};
  const cleanupDrag=useRef<(()=>void)|null>(null);const [drop,setDrop]=useState<{id:string;zone:"inside"|"before"|"after";invalid:boolean}|null>(null);
  const suppressClick=useRef(false);
  const section=useRef<HTMLElement>(null);const [outlineZoom,setOutlineZoom]=useState(1);
  const node=findStructure(root,selected)||root;
  useEffect(()=>{
    const dom=editor.view.dom;
    const zoom=(event:Event)=>{const delta=(event as CustomEvent<number>).detail;const instance=mind.current;if(instance)instance.scale(Math.max(.2,Math.min(3,instance.scaleVal+delta*.1)));else setOutlineZoom(value=>Math.max(.5,Math.min(2,value+delta*.1)));};
    const select=(event:Event)=>setSelected(structureAt(latest.current.root,(event as CustomEvent<number>).detail)?.id||"root");
    dom.addEventListener("note-zoom",zoom);dom.addEventListener("note-select-block",select);
    return()=>{dom.removeEventListener("note-zoom",zoom);dom.removeEventListener("note-select-block",select);};
  },[editor]);
  const apply=(next:JSONContent)=>{if(disabled)return;const value=editor.schema.nodeFromJSON(next);editor.view.dispatch(closeHistory(editor.state.tr).replaceWith(0,editor.state.doc.content.size,value.content));};
  const move=(from:string,to:string,zone:"inside"|"before"|"after")=>{const state=latest.current;if(state.disabled)return;const source=findStructure(state.root,from);const target=findStructure(state.root,to);if(!source||!target||source.index<0)throw new Error("请选择有效的内容分支");apply(moveStructure(state.doc,source.index,target.index,zone));};
  const moveRef=useRef(move);moveRef.current=move;
  useEffect(()=>{
    if(layout!=="mindMap"||!host.current)return;
    const instance=new MindElixir({el:host.current,direction,alignment:"nodes",editable:!disabled,contextMenu:false,keypress:false,toolBar:false,allowUndo:false,locale:"zh_CN",theme:MindElixir.DARK_THEME,before:{beginEdit:()=>false,removeNodes:()=>false,addChild:()=>false,insertParent:()=>false,insertSibling:()=>false,copyNode:()=>false,copyNodes:()=>false}});
    mind.current=instance;const error=instance.init(structureMap(latest.current.root));if(error)notify(String(error),true);else instance.scaleFit();
    instance.bus.addListener("selectNodes",nodes=>{if(nodes[0])setSelected(nodes[0].id);});
    instance.bus.addListener("expandNode",node=>setFolded(v=>{const next=new Set(v);if(node.expanded===false)next.add(node.id);else next.delete(node.id);return next;}));
    instance.bus.addListener("operation",event=>{
      if(!["moveNodeIn","moveNodeBefore","moveNodeAfter","moveUpNode","moveDownNode"].includes(event.name))return;
      try{if("objs" in event&&"toObj" in event){if(event.objs.length!==1)throw new Error("请逐个移动章节，以保留正文顺序");moveRef.current(event.objs[0].id,event.toObj.id,event.name==="moveNodeIn"?"inside":event.name==="moveNodeBefore"?"before":"after");}}
      catch(e){notify(String(e),true);}finally{instance.refresh(structureMap(latest.current.root));}
    });return()=>{mind.current=null;instance.destroy();};
  },[layout,disabled,notify]);
  useEffect(()=>{if(!mind.current)return;const data=structureMap(root);const fold=(n:typeof data.nodeData)=>{n.expanded=!folded.has(n.id);n.children?.forEach(fold);};fold(data.nodeData);mind.current.direction=direction;mind.current.refresh(data);const target=host.current?.querySelector<HTMLElement>(`me-tpc[data-nodeid="me${selectedRef.current}"]`);if(target)mind.current.selection.select(target);},[root,folded,direction]);
  useEffect(()=>{if(!mind.current)return;const palettes:Record<string,string[]>={graphite:["#D99A42","#7CACCB","#85B596","#B097C9"],ocean:["#78B4CA","#93B5DD","#82C1BC","#9EABCB"],forest:["#85B596","#ABC08E","#C4B48A","#96B9AD"]};mind.current.changeTheme({...MindElixir.DARK_THEME,name:mapTheme,palette:palettes[mapTheme]});},[mapTheme,layout,disabled]);
  const add=(kind:"heading"|"paragraph")=>{
    try{const result=insertStructureChild(doc,node.index);if(kind==="heading")result.document.content![result.index]={...result.document.content![result.index],type:"heading",attrs:{...result.document.content![result.index].attrs,level:1}};apply(result.document);setSelected(result.document.content![result.index].attrs!.topicId);}catch(error){notify(String(error),true);}
  };
  const remove=()=>{if(node.index<0)return;const blocks=(doc.content||[]).slice(node.index,node.end);const count=(value:JSONContent):number=>(["image","attachment","reference","mindMap","remoteImage"].includes(value.type||"")?1:0)+(value.content||[]).reduce((n,child)=>n+count(child),0);if(hasLearningAtoms(blocks)||window.confirm(`删除“${node.topic}”分支？将删除 ${blocks.length} 个内容块、${blocks.map(blockText).join("").length} 字符和 ${blocks.reduce((n,b)=>n+count(b),0)} 个图片/附件/引用。可使用撤销恢复。`))apply(removeStructure(doc,node.index));};
  const keyboard=(event:React.KeyboardEvent)=>{
    if(event.defaultPrevented||event.nativeEvent.isComposing)return;
    const id=activeNoteShortcutAction(event.nativeEvent);if(!id)return;
    const typing=!!(event.target as Element).closest("input,textarea,select,[contenteditable=true]");
    if(typing&&!["noteUndo","noteRedo","noteIndent","noteSibling","notePromote"].includes(id))return;
    if(typing&&((event.target as Element).closest("pre,table,li")||(event.target as Element).closest(".tiptap")?.getAttribute("data-note-native-keys")==="true"))return;
    if(disabled)return;
    if(!["noteSibling","noteIndent","notePromote","noteUndo","noteRedo","noteBold","noteItalic","noteUnderline","noteStrike","noteCode","noteLink"].includes(id)&&!/^note(Siblings)?(Heading|Paragraph)/.test(id))return;
    event.preventDefault();event.stopPropagation();
    try{
      if(id==="noteUndo"||id==="noteRedo"){id==="noteUndo"?editor.commands.undo():editor.commands.redo();return;}
      if(id==="noteSibling"){const value=insertStructureSibling(doc,node.index);apply(value.document);setSelected(value.document.content![value.index].attrs!.topicId);return;}
      if(node.index<0)throw Error("请先选择内容主题");
      if(id==="noteIndent"){const result=insertStructureChild(doc,node.index);apply(result.document);setSelected(result.document.content![result.index].attrs!.topicId);return;}
      if(id==="notePromote"){apply(indentStructure(doc,node.index,true));return;}
      const heading=/^note(Siblings)?(Heading([123])|Paragraph)$/.exec(id);
      if(heading){apply(formatStructure(doc,node.index,Number(heading[3]||0),!!heading[1]));setSelected(node.id);return;}
      const block=editor.state.doc.child(node.index);if(!block.isTextblock)throw Error("这个主题不是文字内容，请打开对应附件");
      let from=1;for(let i=0;i<node.index;i++)from+=editor.state.doc.child(i).nodeSize;
      editor.commands.setTextSelection({from,to:from+block.content.size});
      if(id==="noteLink"){
        const own=section.current?.querySelector<HTMLElement>(".structure-content .tiptap");
        own?.dispatchEvent(new CustomEvent("note-edit-link"));
      }else applyNoteFormat(editor,id);
    }catch(error){notify(String(error),true);}
  };
  const pointerDrag=(e:React.PointerEvent<HTMLElement>,id:string)=>{
    if(disabled||e.button!==0||(e.target as Element).closest(".structure-fold"))return;
    cleanupDrag.current?.();
    cleanupDrag.current=beginPointerReorder(e,{
      selector:"[data-structure-node]",
      placement:(target,point)=>{if(target.dataset.structureNode==="root")return "inside";const rect=target.getBoundingClientRect(),ratio=(point.y-rect.top)/Math.max(1,rect.height);return ratio<.25?"before":ratio>.75?"after":"inside";},
      canDrop:(target,zone)=>{try{const state=latest.current;moveStructure(state.doc,findStructure(state.root,id)!.index,findStructure(state.root,target.dataset.structureNode!)!.index,zone);return true;}catch{return false;}},
      onDrop:(target,_after,zone)=>{try{moveRef.current(id,target.dataset.structureNode!,zone);}catch(error){notify(String(error),true);}},
      onInvalid:()=>notify("不能放到自身或后代主题中，原内容未修改",true),
    });
  };
  useEffect(()=>()=>cleanupDrag.current?.(),[]);
  const tree=(n:StructureNode,depth=0):React.ReactNode=><div key={n.id}><div data-structure-node={n.id} className={`structure-row ${selected===n.id?"active":""} ${drop?.id===n.id?`drop-${drop.invalid?"invalid":drop.zone}`:""}`} style={{paddingLeft:depth*14+8}} onPointerDown={e=>{if(n.index>=0)pointerDrag(e,n.id);}}><button className="structure-fold" aria-label={`${folded.has(n.id)?"展开":"折叠"}${n.topic}`} disabled={!n.children.length} onClick={()=>setFolded(v=>{const next=new Set(v);if(next.has(n.id))next.delete(n.id);else next.add(n.id);return next;})}>{n.children.length?(folded.has(n.id)?"▸":"▾"):"·"}</button><button onClick={()=>setSelected(n.id)} title={n.topic}>{layout==="outline"?(n.text||n.topic):n.topic}</button><small>{n.children.length||""}</small></div>{layout==="outline"&&selected===n.id&&n.index>=0&&<div className="structure-inline-content" style={{marginLeft:depth*14+32}}><BlockEditor key={n.id} editor={editor} index={n.index} disabled={disabled} libraryId={libraryId} noteId={noteId} onInsert={onInsert} onAi={onAi} notify={notify} trackImport={trackImport}/></div>}{layout==="outline"&&selected!==n.id&&n.images?.map(src=><button key={src} className="structure-image" style={{marginLeft:depth*14+32}} onClick={()=>setImagePreview({images:n.images!,index:n.images!.indexOf(src)})}><img src={src} alt={n.topic}/></button>)}{!folded.has(n.id)&&n.children.map(child=>tree(child,depth+1))}</div>;
  return <section ref={section} onKeyDownCapture={keyboard} onClickCapture={e=>{const image=(e.target as Element).closest(".structure-canvas img");if(image){const src=image.getAttribute("src")||"";const ids:string[]=[];const collect=(n:StructureNode)=>{ids.push(...(n.images||[]));n.children.forEach(collect);};collect(root);if(ids.includes(src)){e.preventDefault();e.stopPropagation();setImagePreview({images:[...new Set(ids)],index:[...new Set(ids)].indexOf(src)});return;}}if(suppressClick.current){e.preventDefault();e.stopPropagation();}}} className="note-structure" aria-label="整篇笔记结构">{imagePreview&&<ImageLightbox images={imagePreview.images.map(src=>({id:src.split("/").at(-1)!,noteAttachmentId:src.split("/").at(-1)!,originalName:"笔记图片"}))} initialIndex={imagePreview.index} title={title} onClose={()=>setImagePreview(null)}/>}<header><div><strong>{layout==="mindMap"?"思维导图":"内容大纲"}</strong><span>与正文实时同步 · 点击节点编辑完整内容</span></div><div><button disabled={disabled} onClick={()=>editor.commands.undo()}>撤销</button><button disabled={disabled} onClick={()=>editor.commands.redo()}>重做</button><button disabled={disabled} onClick={()=>add("heading")}>子章节</button><button disabled={disabled} onClick={()=>add("paragraph")}>子主题</button><button disabled={disabled||node.index<0} onClick={remove}>删除分支</button>{layout==="mindMap"&&<><select aria-label="导图布局" value={direction} onChange={e=>{const value=Number(e.target.value) as 1|2|3;setDirection(value);if(!disabled)editor.view.dispatch(editor.state.tr.setDocAttribute("mapDirection",value));}}><option value={2}>中心双向</option><option value={1}>右向逻辑</option><option value={3}>向下树状</option></select><select aria-label="导图主题" value={mapTheme} onChange={e=>{setMapTheme(e.target.value);if(!disabled)editor.view.dispatch(editor.state.tr.setDocAttribute("mapTheme",e.target.value));}}><option value="graphite">石墨</option><option value="ocean">海蓝</option><option value="forest">森林</option></select><button onClick={()=>mind.current?.scaleFit()}>适应全部</button><button onClick={()=>{if(mind.current){if(node.index>=0){const target=mind.current.findEle(selected);const rect=target.getBoundingClientRect(),viewport=host.current!.getBoundingClientRect();mind.current.scale(Math.max(.2,Math.min(3,mind.current.scaleVal*Math.min(viewport.width/(rect.width+80),viewport.height/(rect.height+80)))));mind.current.scrollIntoView(target,true);}else mind.current.scaleFit();}}}>适应选中</button><button onClick={()=>mind.current?.scale(1)}>100%</button><label>分支颜色<input aria-label="分支颜色" type="color" disabled={disabled||node.index<0} value={node.branchColor||"#D99A42"} onChange={e=>{if(node.index<0)return;const block=editor.state.doc.child(node.index);let pos=0;for(let i=0;i<node.index;i++)pos+=editor.state.doc.child(i).nodeSize;editor.view.dispatch(editor.state.tr.setNodeMarkup(pos,undefined,{...block.attrs,topicColor:e.target.value}));}}/></label><button onClick={()=>onExport("svg")}>导出 SVG</button><button onClick={()=>onExport("png")}>导出 PNG</button></>}{layout==="mindMap"&&<button aria-pressed={details} onClick={()=>setDetails(v=>!v)}>内容面板</button>}</div></header><div className="structure-templates"><select aria-label="导图模板" value={template} onChange={e=>setTemplate(e.target.value as NoteTemplate)}>{["tutorial","concept","troubleshooting","cgWorkflow","retrospective"].map(id=><option key={id} value={id}>{noteTemplates[id as NoteTemplate].name}</option>)}</select><button disabled={disabled} onClick={()=>apply({...doc,content:[...(doc.content||[]),...(normalizeStructure(templateDocument(template)).content||[])]})}>追加模板章节</button><span>保留现有内容，不覆盖</span></div><div className="structure-columns">{layout==="outline"?<nav className="structure-tree" aria-label="完整内容大纲"><div style={{zoom:outlineZoom}}>{tree(root)}</div></nav>:<div className="structure-canvas" ref={host}/>}<aside hidden={!details||layout==="outline"} className="structure-content"><header><strong>{node.index<0?"选择一个内容节点":node.topic}</strong></header>{node.index>=0?<BlockEditor key={selected} editor={editor} index={node.index} disabled={disabled} libraryId={libraryId} noteId={noteId} onInsert={onInsert} onAi={onAi} notify={notify} trackImport={trackImport}/>:<p>主题可拥有任意类型的子内容；图片、表格、代码和独立导图保留原格式。拖动分支调整正文层级和顺序。</p>}</aside></div></section>;
}

function BlockEditor({editor:main,index,disabled,libraryId,noteId,onInsert,onAi,notify,trackImport}: {editor:Editor;index:number;disabled:boolean;libraryId:string;noteId:string;onInsert:(kind:"attachment"|"reference"|"screenshot")=>void;onAi:()=>void;notify:(message:string,error?:boolean)=>void;trackImport:(work:Promise<unknown>)=>Promise<unknown>}){
  const own=useRef(false);const range=useRef({from:0,to:0});
  const content=()=>{let from=0;for(let i=0;i<index&&i<main.state.doc.childCount;i++)from+=main.state.doc.child(i).nodeSize;const block=main.state.doc.maybeChild(index);range.current={from,to:from+(block?.nodeSize||0)};return {type:"doc",content:[block?.toJSON()||{type:"paragraph"}]};};
  const [initial]=useState(content);
  const nativeKeys=(editor:Editor)=>{const {$from}=editor.state.selection;editor.view.dom.setAttribute("data-note-native-keys",String($from.depth>1||($from.depth===1&&!["paragraph","heading"].includes($from.parent.type.name))));};
  const editor=useEditor({autofocus:disabled?false:"end",onSelectionUpdate:({editor})=>nativeKeys(editor),onCreate:({editor})=>{nativeKeys(editor);if(!disabled&&["新主题","新内容"].includes(editor.getText()))editor.commands.selectAll();},extensions:main.options.extensions.map(extension=>extension.name==="starterKit"?extension.configure({trailingNode:false}):extension.name==="learningDeletionGuard"?extension.configure({confirmDelete:()=>true}):extension),content:initial,editable:!disabled,editorProps:{...main.options.editorProps,attributes:{class:"knowledge-prose",role:"textbox","aria-label":"节点完整内容"}},onUpdate:({editor})=>{nativeKeys(editor);if(disabled)return;try{const fragment=Fragment.fromArray((editor.getJSON().content||[]).map(b=>main.schema.nodeFromJSON(b)));own.current=true;const previous=main.state.doc;main.view.dispatch(main.state.tr.replaceWith(range.current.from,range.current.to,fragment));if(main.state.doc===previous)editor.commands.setContent(content(),{emitUpdate:false});else range.current.to=range.current.from+fragment.size;}catch(e){notify(String(e),true);}finally{own.current=false;}}});
  useEffect(()=>{editor?.setEditable(!disabled,false);},[editor,disabled]);
  useEffect(()=>{const update=({transaction}: {transaction:{docChanged:boolean}})=>{if(transaction.docChanged&&!own.current)editor?.commands.setContent(content(),{emitUpdate:false});};main.on("transaction",update);return()=>{main.off("transaction",update);};},[main,editor,index]);
  const atCursor=(kind:"attachment"|"reference"|"screenshot")=>{if(editor)main.commands.setTextSelection(Math.min(main.state.doc.content.size,range.current.from+editor.state.selection.from));onInsert(kind);};
  return <div onCopy={event=>{if(editor&&!editor.state.selection.empty){event.preventDefault();void copyNoteSelection(editor,libraryId,noteId,false).catch(e=>notify(String(e),true));}}} onCut={event=>{if(editor&&!disabled&&!editor.state.selection.empty){event.preventDefault();void copyNoteSelection(editor,libraryId,noteId,true).catch(e=>notify(String(e),true));}}} onPasteCapture={event=>{if(editor&&event.clipboardData.getData("text/html").includes("data-stack-note-clip")){event.preventDefault();if(!disabled)void trackImport(pasteNoteSelection(editor,libraryId,noteId)).catch(e=>notify(String(e),true));}}}><NoteEditorTools editor={editor} libraryId={libraryId} noteId={noteId} disabled={disabled} onInsert={atCursor} onAi={onAi}/><EditorContent editor={editor}/></div>;
}
