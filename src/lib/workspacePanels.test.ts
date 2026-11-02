import {describe,expect,it} from "vitest";
import {normalizePanel,persistPanels,reducePanel,setLayoutPanel,togglePanelFocus,type PanelLayout,type PanelState} from "./workspacePanels";
const limits={min:160,max:340,defaultWidth:200};
const pinned:PanelState={mode:"pinned",width:280,tab:"navigation"};
const closed:PanelState={mode:"closed",width:240,tab:"history"};
describe("workspace panel transitions",()=>{
  it("keeps existing appearance widths for old contexts and normalizes corrupt saved fields",()=>{
    expect(normalizePanel(undefined,pinned,limits,["navigation","list"])).toEqual(pinned);
    expect(normalizePanel({mode:"broken",width:Infinity,tab:"unknown"},pinned,limits,["navigation","list"])).toEqual(pinned);
    expect(normalizePanel({mode:"closed",width:999.2,tab:"list"},pinned,limits,["navigation","list"])).toEqual({mode:"closed",width:340,tab:"list"});
    expect(normalizePanel({mode:"peek",width:150.2,tab:"navigation"},pinned,limits,["navigation"])).toEqual({mode:"peek",width:160,tab:"navigation"});
    expect(pinned.width).toBe(280);
  });
  it("opens temporary tabs but keeps a fixed panel fixed when changing tabs",()=>{
    const opened=reducePanel(closed,{type:"toggle",tab:"navigation"},limits);
    expect(opened).toEqual({mode:"peek",width:240,tab:"navigation"});
    const changed=reducePanel(opened,{type:"toggle",tab:"list"},limits);
    expect(changed).toEqual({mode:"peek",width:240,tab:"list"});
    const fixed=reducePanel(changed,{type:"pin"},limits);
    expect(fixed.mode).toBe("pinned");
    expect(reducePanel(fixed,{type:"toggle",tab:"navigation"},limits)).toEqual({mode:"pinned",width:240,tab:"navigation"});
    expect(reducePanel(fixed,{type:"toggle",tab:"list"},limits).mode).toBe("closed");
  });
  it("dismisses only temporary panels and allows explicit unpin or close",()=>{
    expect(reducePanel(pinned,{type:"dismiss"},limits)).toEqual(pinned);
    const temporary=reducePanel(pinned,{type:"pin"},limits);
    expect(temporary.mode).toBe("peek");
    expect(reducePanel(temporary,{type:"dismiss"},limits)).toEqual({...pinned,mode:"closed"});
    expect(reducePanel(pinned,{type:"close"},limits).mode).toBe("closed");
  });
  it("changes width only while pinned and bounds drag and reset values",()=>{
    expect(reducePanel(closed,{type:"resize",width:300},limits)).toEqual(closed);
    expect(reducePanel({...closed,mode:"peek"},{type:"resize",width:300},limits)).toEqual({...closed,mode:"peek"});
    expect(reducePanel(pinned,{type:"resize",width:900},limits).width).toBe(340);
    expect(reducePanel(pinned,{type:"resize",width:-2},limits).width).toBe(160);
    expect(reducePanel(pinned,{type:"resize",width:260.7},limits).width).toBe(261);
    for(const width of [NaN,Infinity,-Infinity])expect(reducePanel(pinned,{type:"resize",width},limits)).toEqual(pinned);
    expect(reducePanel(pinned,{type:"resetWidth"},limits).width).toBe(200);
  });
  it("restores the exact layout after focus and persists the unfocused fixed preferences",()=>{
    const layout:PanelLayout={left:pinned,right:{...closed,mode:"peek"},focusSnapshot:null};
    const focused=togglePanelFocus(layout);
    expect([focused.left.mode,focused.right.mode]).toEqual(["closed","closed"]);
    expect(persistPanels(focused)).toEqual({left:pinned,right:closed});
    expect(togglePanelFocus(focused)).toEqual(layout);
    expect(layout.focusSnapshot).toBeNull();
  });
  it("exits focus on an explicit panel operation while restoring the other side",()=>{
    const layout:PanelLayout={left:pinned,right:{...closed,mode:"pinned"},focusSnapshot:null};
    const changed=setLayoutPanel(togglePanelFocus(layout),"left",{mode:"peek",width:280,tab:"list"});
    expect(changed).toEqual({left:{mode:"peek",width:280,tab:"list"},right:{...closed,mode:"pinned"},focusSnapshot:null});
    expect(persistPanels(changed)).toEqual({left:{mode:"closed",width:280,tab:"list"},right:{...closed,mode:"pinned"}});
  });
});
