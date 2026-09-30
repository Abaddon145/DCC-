import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { menuPosition } from "./ContextMenu";

export interface KnowledgeMenuAction { label:string; run:()=>void; disabled?:boolean; danger?:boolean }
export interface KnowledgeMenuState { x:number; y:number; actions:KnowledgeMenuAction[] }

/** One local action menu; never takes focus away from the editor selection. */
export function KnowledgeMenu({menu,onClose}:{menu:KnowledgeMenuState;onClose:()=>void}) {
  const ref=useRef<HTMLDivElement>(null);
  const [position,setPosition]=useState({x:menu.x,y:menu.y});
  useLayoutEffect(()=>{const size=ref.current?.getBoundingClientRect();if(size){const position=menuPosition(menu.x,menu.y,size.width,size.height,window.innerWidth,window.innerHeight);setPosition({x:position.left,y:position.top});}},[menu]);
  useEffect(()=>{
    const outside=(event:PointerEvent)=>{if(!ref.current?.contains(event.target as Node))onClose();};
    const escape=(event:KeyboardEvent)=>{
      if(event.key==="Escape"){event.preventDefault();event.stopPropagation();onClose();}
      if(event.key==="Tab")onClose();
      if(["ArrowDown","ArrowUp","Home","End"].includes(event.key)){const buttons=Array.from(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")||[]);if(!buttons.length)return;event.preventDefault();const current=buttons.indexOf(document.activeElement as HTMLButtonElement);const index=event.key==="Home"?0:event.key==="End"?buttons.length-1:event.key==="ArrowUp"?(current<=0?buttons.length-1:current-1):(current+1)%buttons.length;buttons[index].focus();}
    };
    const dismiss=()=>onClose();
    document.addEventListener("pointerdown",outside,true);document.addEventListener("keydown",escape,true);window.addEventListener("resize",dismiss);window.addEventListener("blur",dismiss);
    return()=>{document.removeEventListener("pointerdown",outside,true);document.removeEventListener("keydown",escape,true);window.removeEventListener("resize",dismiss);window.removeEventListener("blur",dismiss);};
  },[onClose]);
  return createPortal(<div ref={ref} role="menu" aria-label="知识库操作" className="knowledge-context-menu" style={{left:position.x,top:position.y}} onPointerDown={e=>e.preventDefault()} onContextMenu={e=>e.preventDefault()}>{menu.actions.map((action,index)=><button key={index} role="menuitem" disabled={action.disabled} className={action.danger?"danger":""} onClick={()=>{onClose();action.run();}}>{action.label}</button>)}</div>,document.body);
}
