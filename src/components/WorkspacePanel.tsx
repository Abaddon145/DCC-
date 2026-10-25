import {useEffect,useId,useRef,useState,type CSSProperties,type ReactNode} from "react";
import {Pin,PinOff,X} from "lucide-react";
import {reducePanel,type PanelEvent,type PanelLimits,type PanelState} from "../lib/workspacePanels";
import {hasWorkspaceOverlay} from "../lib/workspace";
import "./WorkspacePanel.css";
export interface WorkspacePanelProps {label:string;side:"left"|"right";state:PanelState;limits:PanelLimits;tabs:{id:string;label:string;icon:ReactNode;content:ReactNode}[];onChange:(next:PanelState)=>void}
export function WorkspacePanel({label,side,state,limits,tabs,onChange}:WorkspacePanelProps) {
  const host=useRef<HTMLDivElement>(null),id=useId();
  const [visited,setVisited]=useState<string[]>(()=>state.mode==="closed"?[]:[state.tab]);
  const latest=useRef({state,limits,onChange});latest.current={state,limits,onChange};
  const drag=useRef<{id:number;x:number;width:number}|null>(null);
  const dispatch=(event:PanelEvent)=>{const value=latest.current;value.onChange(reducePanel(value.state,event,value.limits));};
  const returnFocus=()=>document.getElementById(`${id}-trigger-${latest.current.state.tab}`)?.focus();
  const close=()=>{dispatch({type:"close"});returnFocus();};
  const open=state.mode!=="closed";
  useEffect(()=>{if(open)setVisited(value=>value.includes(state.tab)?value:[...value,state.tab]);},[open,state.tab]);
  useEffect(()=>{
    if(state.mode!=="peek")return;
    let composing=false;
    const modal=()=>hasWorkspaceOverlay("[role=dialog][aria-modal=true],[role=alertdialog],[role=menu],dialog[open],.modal-backdrop");
    const start=()=>{composing=true;},end=()=>{composing=false;};
    const outside=(event:PointerEvent)=>{if(!composing&&!event.defaultPrevented&&!modal()&&!host.current?.contains(event.target as Node))dispatch({type:"dismiss"});};
    const key=(event:KeyboardEvent)=>{
      if(event.key!=="Escape"||event.defaultPrevented||event.isComposing||composing||modal()||(event.target instanceof Element&&event.target.closest("input,textarea,select,[contenteditable=true]")))return;
      if(event.target instanceof Element&&event.target.closest(".workspace-panel")&&!host.current?.contains(event.target))return;
      event.preventDefault();event.stopPropagation();dispatch({type:"dismiss"});returnFocus();
    };
    window.addEventListener("pointerdown",outside,true);window.addEventListener("keydown",key,true);window.addEventListener("compositionstart",start);window.addEventListener("compositionend",end);
    return()=>{window.removeEventListener("pointerdown",outside,true);window.removeEventListener("keydown",key,true);window.removeEventListener("compositionstart",start);window.removeEventListener("compositionend",end);};
  },[state.mode]);
  return <div ref={host} className={`workspace-panel mode-${state.mode}`} data-side={side} style={{"--workspace-panel-width":`${state.width}px`} as CSSProperties}>
    <nav className="workspace-panel-rail" aria-label={`${label}入口`}>{tabs.map(tab=><button key={tab.id} id={`${id}-trigger-${tab.id}`} type="button" aria-label={tab.label} title={tab.label} aria-controls={`${id}-${tab.id}`} aria-expanded={open&&state.tab===tab.id} onClick={()=>dispatch({type:"toggle",tab:tab.id})}><span aria-hidden="true">{tab.icon}</span></button>)}</nav>
    <aside className="workspace-panel-pane" aria-label={label} hidden={!open} data-native-overlay={state.mode==="peek"?"":undefined}>
      <header><strong>{tabs.find(tab=>tab.id===state.tab)?.label||label}</strong><button type="button" className="icon-button" aria-label={`${state.mode==="pinned"?"取消固定":"固定"}${label}`} aria-pressed={state.mode==="pinned"} onClick={()=>dispatch({type:"pin"})}>{state.mode==="pinned"?<PinOff size={16}/>:<Pin size={16}/>}</button><button type="button" className="icon-button" aria-label={`收起${label}`} onClick={close}><X size={16}/></button></header>
      {tabs.map(tab=>(visited.includes(tab.id)||open&&state.tab===tab.id)&&<div className="workspace-panel-page" id={`${id}-${tab.id}`} key={tab.id} role="tabpanel" aria-label={tab.label} hidden={!open||state.tab!==tab.id}>{tab.content}</div>)}
      {state.mode==="pinned"&&<div className="workspace-panel-splitter" role="separator" tabIndex={0} aria-label={`${label}宽度`} aria-orientation="vertical" aria-valuemin={limits.min} aria-valuemax={limits.max} aria-valuenow={state.width} aria-describedby={`${id}-resize-help`} onDoubleClick={()=>dispatch({type:"resetWidth"})} onKeyDown={event=>{
        if(event.nativeEvent.isComposing)return;
        if(event.key==="Home"){event.preventDefault();dispatch({type:"resetWidth"});}
        else if(event.key==="ArrowLeft"||event.key==="ArrowRight"){event.preventDefault();dispatch({type:"resize",width:state.width+(event.key==="ArrowRight"?1:-1)*(side==="left"?1:-1)*10});}
      }} onPointerDown={event=>{if(event.button!==0)return;event.preventDefault();drag.current={id:event.pointerId,x:event.clientX,width:state.width};event.currentTarget.setPointerCapture?.(event.pointerId);}} onPointerMove={event=>{
        const start=drag.current;if(!start||event.pointerId!==start.id)return;
        dispatch({type:"resize",width:start.width+(event.clientX-start.x)*(side==="left"?1:-1)});
      }} onPointerUp={event=>{if(drag.current?.id===event.pointerId){drag.current=null;if(event.currentTarget.hasPointerCapture?.(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);}}} onPointerCancel={()=>{drag.current=null;}}/>}
      <span className="workspace-panel-help" id={`${id}-resize-help`}>方向键调宽，Home 或双击恢复默认宽度。</span>
    </aside>
  </div>;
}
