export interface PanelState {mode:"closed"|"peek"|"pinned";width:number;tab:string}
export interface PanelLimits {min:number;max:number;defaultWidth:number}
export type PanelEvent={type:"toggle";tab:string}|{type:"pin"|"close"|"dismiss"|"resetWidth"}|{type:"resize";width:number};
export interface PanelLayout {left:PanelState;right:PanelState;focusSnapshot:{left:PanelState;right:PanelState}|null}
const boundedWidth=(width:number,limits:PanelLimits)=>Math.round(Math.max(limits.min,Math.min(limits.max,width)));
export function normalizePanel(raw:unknown,fallback:PanelState,limits:PanelLimits,tabs:readonly string[]):PanelState {
  const value=(raw&&typeof raw==="object"?raw:{}) as Record<string,unknown>;
  return {
    mode:value.mode==="closed"||value.mode==="peek"||value.mode==="pinned"?value.mode:fallback.mode,
    width:boundedWidth(typeof value.width==="number"&&Number.isFinite(value.width)?value.width:fallback.width,limits),
    tab:typeof value.tab==="string"&&tabs.includes(value.tab)?value.tab:fallback.tab,
  };
}
export function reducePanel(state:PanelState,event:PanelEvent,limits:PanelLimits):PanelState {
  switch(event.type){
    case "toggle":return {...state,tab:event.tab,mode:state.mode!=="closed"&&state.tab===event.tab?"closed":state.mode==="pinned"?"pinned":"peek"};
    case "pin":return {...state,mode:state.mode==="pinned"?"peek":"pinned"};
    case "close":return {...state,mode:"closed"};
    case "dismiss":return state.mode==="peek"?{...state,mode:"closed"}:state;
    case "resize":return state.mode==="pinned"&&Number.isFinite(event.width)?{...state,width:boundedWidth(event.width,limits)}:state;
    case "resetWidth":return {...state,width:boundedWidth(limits.defaultWidth,limits)};
  }
}
export function togglePanelFocus(layout:PanelLayout):PanelLayout {
  if(layout.focusSnapshot)return {...layout.focusSnapshot,focusSnapshot:null};
  return {left:{...layout.left,mode:"closed"},right:{...layout.right,mode:"closed"},focusSnapshot:{left:layout.left,right:layout.right}};
}
export function setLayoutPanel(layout:PanelLayout,side:"left"|"right",panel:PanelState):PanelLayout {
  return {...(layout.focusSnapshot||layout),[side]:panel,focusSnapshot:null};
}
export function persistPanels(layout:PanelLayout):{left:PanelState;right:PanelState} {
  const value=layout.focusSnapshot||layout;
  const stable=(panel:PanelState):PanelState=>({...panel,mode:panel.mode==="peek"?"closed":panel.mode});
  return {left:stable(value.left),right:stable(value.right)};
}
