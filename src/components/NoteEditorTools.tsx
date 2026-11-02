import { useEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import { KnowledgeMenu, type KnowledgeMenuState, type KnowledgeMenuAction } from "./KnowledgeMenu";
import { api } from "../lib/api";
import {copyNoteSelection,pasteNoteSelection} from "../lib/noteClipboard";
import {activeNoteShortcutAction} from "../lib/noteShortcuts";

export function NoteHeadingOutline({editor}:{editor:Editor|null}) {
  const [headings,setHeadings]=useState<Array<{position:number;level:number;text:string}>>([]),[active,setActive]=useState(-1);
  useEffect(()=>{if(!editor)return;const update=()=>{const items:typeof headings=[];editor.state.doc.descendants((node,position)=>{if(node.type.name==="heading")items.push({position,level:Number(node.attrs.level),text:node.textContent||"（空标题）"});});setHeadings(items);};update();editor.on("update",update);editor.on("selectionUpdate",update);return()=>{editor.off("update",update);editor.off("selectionUpdate",update);};},[editor]);
  useEffect(()=>{if(!editor)return;const scroller=editor.view.dom.closest(".knowledge-document")||editor.view.dom.closest(".knowledge-editor");const update=()=>{if(!scroller)return;const top=scroller.getBoundingClientRect().top+100;let position=-1;headings.forEach((heading,index)=>{const dom=editor.view.nodeDOM(heading.position);if(dom instanceof HTMLElement&&dom.getBoundingClientRect().top<=top)position=index;});setActive(position<0&&headings.length?0:position);};update();scroller?.addEventListener("scroll",update,{passive:true});return()=>scroller?.removeEventListener("scroll",update);},[editor,headings]);
  return <nav className="knowledge-outline" aria-label="笔记大纲"><strong>本页大纲</strong>{headings.length===0?<p>使用 #、##、### 加空格创建标题</p>:headings.map((heading,index)=><button key={heading.position} className={index===active?"active":""} style={{paddingLeft:10+(Math.min(heading.level,6)-1)*12}} onClick={()=>{const dom=editor?.view.nodeDOM(heading.position);if(dom instanceof HTMLElement)dom.scrollIntoView({block:"start",behavior:"auto"});setActive(index);}}>{heading.text}</button>)}</nav>;
}
export function NoteEditorTools({editor,libraryId,noteId="",disabled,onAi,onInsert,panelOutline=false}:{editor:Editor|null;libraryId:string;noteId?:string;disabled:boolean;onAi:()=>void;onInsert:(kind:"attachment"|"reference"|"screenshot")=>void;panelOutline?:boolean}) {
  const [menu,setMenu]=useState<KnowledgeMenuState|null>(null);
  const [outline,setOutline]=useState(false);
  const [level,setLevel]=useState(0);
  const [link,setLink]=useState<string|null>(null);
  const [linkText,setLinkText]=useState("");
  const root=useRef<HTMLDivElement>(null);
  const [error,setError]=useState("");
  useEffect(()=>{
    if(!editor)return;const dom=editor.view.dom;
    const openLink=(event?:Event)=>{if(disabled)return;if(event?.type==="note-edit-link")editor.commands.selectAll();setLinkText(editor.state.doc.textBetween(editor.state.selection.from,editor.state.selection.to,"\n"));setLink(String(editor.getAttributes("link").href||""));};
    const key=(event:KeyboardEvent)=>{if(event.isComposing||disabled||activeNoteShortcutAction(event)!=="noteLink")return;event.preventDefault();event.stopPropagation();openLink();};
    dom.addEventListener("keydown",key,true);dom.addEventListener("note-edit-link",openLink);
    return()=>{dom.removeEventListener("keydown",key,true);dom.removeEventListener("note-edit-link",openLink);};
  },[editor,disabled]);
  useEffect(()=>{if(disabled){setMenu(null);setLink(null);}},[disabled]);
  const act=(work:()=>Promise<void>|void)=>{void Promise.resolve().then(work).catch(()=>setError("操作失败。剪贴板请使用 Ctrl+C / Ctrl+V；链接仅支持 http/https。"));};
  useEffect(()=>{
    if(!editor)return;
    const update=()=>{setLevel(editor.isActive("heading")?Number(editor.getAttributes("heading").level):0);};
    update();editor.on("update",update);editor.on("selectionUpdate",update);
    return()=>{editor.off("update",update);editor.off("selectionUpdate",update);};
  },[editor]);
  useEffect(()=>{
    if(!editor)return;const dom=editor.view.dom;
    let selected:{from:number;to:number}|null=null;
    const remember=(event:MouseEvent)=>{if(event.button===2)selected={from:editor.state.selection.from,to:editor.state.selection.to};};
    const context=(event:MouseEvent)=>{
      event.preventDefault();event.stopPropagation();
      const selection=selected||{from:editor.state.selection.from,to:editor.state.selection.to};selected=null;
      const position=editor.view.posAtCoords({left:event.clientX,top:event.clientY})?.pos;
      const image=(event.target as Element).closest("img,.knowledge-reference-card,.knowledge-mind-map");
      let nodePosition:number|undefined;if(image)editor.state.doc.descendants((node,pos)=>{const dom=editor.view.nodeDOM(pos);if(dom===image)nodePosition=pos;});
      if(nodePosition!==undefined){const node=editor.state.doc.nodeAt(nodePosition);if(selection.from<selection.to&&selection.from<=nodePosition&&selection.to>=nodePosition+(node?.nodeSize||1))editor.commands.setTextSelection(selection);else editor.commands.setNodeSelection(nodePosition);}
      else if(position!==undefined){if(selection.from!==selection.to&&position>=selection.from&&position<=selection.to)editor.commands.setTextSelection(selection);else editor.commands.setTextSelection(position);}
      const hasSelection=!editor.state.selection.empty;
      const text=()=>editor.state.doc.textBetween(editor.state.selection.from,editor.state.selection.to,"\n");
      const actions:KnowledgeMenuAction[]=[
        {label:"撤销",disabled:disabled||!editor.can().undo(),run:()=>editor.chain().focus().undo().run()},
        {label:"重做",disabled:disabled||!editor.can().redo(),run:()=>editor.chain().focus().redo().run()},
        {label:"复制",disabled:!hasSelection,run:()=>act(()=>copyNoteSelection(editor,libraryId,noteId,false))},
        {label:"剪切",disabled:disabled||!hasSelection,run:()=>act(()=>copyNoteSelection(editor,libraryId,noteId,true))},
        {label:"粘贴",disabled,run:()=>act(async()=>{editor.commands.focus();if(await pasteNoteSelection(editor,libraryId,noteId))return;if(!document.execCommand?.("paste")){const value=await api.readClipboard();editor.chain().focus().insertContent(value.split(/\r?\n/).map(text=>({type:"paragraph",content:text?[{type:"text",text}]:[]}))).run();}})},
        {label:"粘贴为纯文本",disabled,run:()=>act(async()=>{const value=await api.readClipboard();editor.chain().focus().insertContent(value.split(/\r?\n/).map(text=>({type:"paragraph",content:text?[{type:"text",text}]:[]}))).run();})},
        {label:"正文",disabled,run:()=>editor.chain().focus().setParagraph().run()},
        ...([1,2,3] as const).map(level=>({label:`${["","一级","二级","三级"][level]}标题`,disabled,run:()=>editor.chain().focus().setHeading({level}).run()})),
        {label:"粗体 / 取消粗体",disabled:disabled||!hasSelection,run:()=>editor.chain().focus().toggleBold().run()},
        {label:"斜体 / 取消斜体",disabled:disabled||!hasSelection,run:()=>editor.chain().focus().toggleItalic().run()},
        {label:"项目列表",disabled,run:()=>editor.chain().focus().toggleBulletList().run()},
        {label:"编号列表",disabled,run:()=>editor.chain().focus().toggleOrderedList().run()},
        {label:"待办清单",disabled,run:()=>editor.chain().focus().toggleTaskList().run()},
        {label:"引用块",disabled,run:()=>editor.chain().focus().toggleBlockquote().run()},
        {label:"代码块",disabled,run:()=>editor.chain().focus().toggleCodeBlock().run()},
        {label:"插入 / 修改链接…",disabled,run:()=>{setLinkText(text());setLink(String(editor.getAttributes("link").href||""));}},
        {label:"插入图片 / 附件…",disabled,run:()=>onInsert("attachment")},
        {label:"粘贴截图",disabled,run:()=>onInsert("screenshot")},
        {label:"插入素材 / 笔记引用…",disabled,run:()=>onInsert("reference")},
        {label:"AI 整理选中文字…",disabled:disabled||!hasSelection,run:onAi}
      ];
      if(editor.isActive("table"))actions.push(...[
        {label:"在上方插入行",run:()=>editor.chain().focus().addRowBefore().run()},
        {label:"在下方插入行",run:()=>editor.chain().focus().addRowAfter().run()},
        {label:"在左侧插入列",run:()=>editor.chain().focus().addColumnBefore().run()},
        {label:"在右侧插入列",run:()=>editor.chain().focus().addColumnAfter().run()},
        {label:"删除当前行",run:()=>editor.chain().focus().deleteRow().run()},
        {label:"删除当前列",run:()=>editor.chain().focus().deleteColumn().run()}
      ].map(action=>({...action,disabled})));
      setMenu({x:event.clientX,y:event.clientY,actions});
    };
    dom.addEventListener("mousedown",remember,true);dom.addEventListener("contextmenu",context);
    return()=>{dom.removeEventListener("mousedown",remember,true);dom.removeEventListener("contextmenu",context);};
  },[editor,disabled,libraryId,noteId,onAi,onInsert]);
  return <div ref={root} className="knowledge-editor-tools">
    <label>段落格式 <select aria-label="段落格式" disabled={disabled} value={level} onChange={e=>{const level=Number(e.target.value);if(level===0)editor?.chain().focus().setParagraph().run();else editor?.chain().focus().setHeading({level:level as 1|2|3}).run();}}><option value={0}>正文</option><option value={1}>一级标题</option><option value={2}>二级标题</option><option value={3}>三级标题</option>{level>3&&<option value={level}>{level} 级标题（已有内容）</option>}</select></label>
    {!panelOutline&&<button aria-expanded={outline} onClick={()=>setOutline(v=>!v)}>标题大纲</button>}
    {!panelOutline&&outline&&<NoteHeadingOutline editor={editor}/>}
    {error&&<p role="alert">{error}<button onClick={()=>setError("")}>关闭</button></p>}
    {menu&&<KnowledgeMenu menu={menu} onClose={()=>setMenu(null)}/>}
    {link!==null&&<div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-label="编辑链接" onSubmit={e=>{e.preventDefault();try{const url=new URL(link);if(!["http:","https:"].includes(url.protocol))throw Error();if(editor?.state.selection.empty&&!editor.isActive("link"))editor?.chain().focus().insertContent({type:"text",text:linkText.trim()||url.href,marks:[{type:"link",attrs:{href:url.href}}]}).run();else editor?.chain().focus().extendMarkRange("link").setLink({href:url.href}).run();setLink(null);}catch{setError("链接仅支持有效的 http/https 地址");}}}><h2>插入或修改链接</h2><input aria-label="链接地址" autoFocus value={link} onChange={e=>setLink(e.target.value)}/><input aria-label="链接文字" value={linkText} onChange={e=>setLinkText(e.target.value)} placeholder="未选择文字时作为链接名称"/><footer><button type="button" onClick={()=>setLink(null)}>取消</button><button type="button" onClick={()=>{editor?.chain().focus().extendMarkRange("link").unsetLink().run();setLink(null);}}>移除链接</button><button>保存</button></footer></form></div>}
  </div>;
}
