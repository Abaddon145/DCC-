import {fireEvent,render,screen} from "@testing-library/react";
import {useEffect,useState} from "react";
import {expect,it,vi} from "vitest";
import {WorkspacePanel} from "./WorkspacePanel";
import type {PanelState} from "../lib/workspacePanels";
const limits={min:160,max:340,defaultWidth:200};
function Content({name,mount}:{name:string;mount:(name:string)=>void}) {
  useEffect(()=>{mount(name);},[name,mount]);
  return <input aria-label={`${name}搜索`} defaultValue=""/>;
}
function Harness({initial="closed",side="left",mount=()=>{},modal=false,label="工作面板"}:{initial?:PanelState["mode"];side?:"left"|"right";mount?:(name:string)=>void;modal?:boolean;label?:string}) {
  const [state,setState]=useState<PanelState>({mode:initial,width:200,tab:"navigation"});
  return <><WorkspacePanel label={label} side={side} state={state} limits={limits} onChange={setState} tabs={[
    {id:"navigation",label:"导航",icon:<span>N</span>,content:<Content name="导航" mount={mount}/>},
    {id:"history",label:"历史",icon:<span>H</span>,content:<Content name="历史" mount={mount}/>},
  ]}/>{modal&&<div role="dialog" aria-modal="true">另一个弹窗</div>}</>;
}
it("keeps a visible rail while closed and mounts only visited pages, preserving their search and scroll",()=>{
  const mount=vi.fn();
  render(<Harness mount={mount}/>);
  expect(screen.getByRole("button",{name:"导航"})).toHaveAttribute("aria-expanded","false");
  expect(mount).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"导航"}));
  fireEvent.change(screen.getByRole("textbox",{name:"导航搜索"}),{target:{value:"UE scene"}});
  const page=screen.getByRole("tabpanel",{name:"导航"});page.scrollTop=89;
  fireEvent.click(screen.getByRole("button",{name:"历史"}));
  expect(screen.getByRole("textbox",{name:"历史搜索"})).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"导航"}));
  expect(screen.getByRole("textbox",{name:"导航搜索"})).toHaveValue("UE scene");
  expect(screen.getByRole("tabpanel",{name:"导航"}).scrollTop).toBe(89);
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("textbox",{name:"导航搜索"})).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"导航"}));
  expect(screen.getByRole("textbox",{name:"导航搜索"})).toHaveValue("UE scene");
  expect(mount.mock.calls.map(([name])=>name)).toEqual(["导航","历史"]);
});
it("gives Escape to the temporary panel that contains keyboard focus",()=>{
  render(<><Harness initial="peek" label="左侧"/><Harness initial="peek" side="right" label="右侧"/></>);
  const close=screen.getByRole("button",{name:"收起右侧"});close.focus();
  fireEvent.keyDown(close,{key:"Escape"});
  expect(screen.getByRole("complementary",{name:"左侧"})).toBeVisible();
  expect(screen.queryByRole("complementary",{name:"右侧"})).toBeNull();
});
it("dismisses only peek panels and marks native occlusion only while temporary",()=>{
  render(<Harness initial="peek"/>);
  expect(screen.getByRole("complementary",{name:"工作面板"})).toHaveAttribute("data-native-overlay");
  screen.getByRole("button",{name:"收起工作面板"}).focus();
  fireEvent.keyDown(window,{key:"Escape"});
  expect(screen.queryByRole("complementary",{name:"工作面板"})).toBeNull();
  expect(screen.getByRole("button",{name:"导航"})).toHaveFocus();
  fireEvent.click(screen.getByRole("button",{name:"导航"}));
  fireEvent.click(screen.getByRole("button",{name:"固定工作面板"}));
  expect(screen.getByRole("complementary",{name:"工作面板"})).not.toHaveAttribute("data-native-overlay");
  fireEvent.pointerDown(document.body);fireEvent.keyDown(window,{key:"Escape"});
  expect(screen.getByRole("complementary",{name:"工作面板"})).toBeVisible();
});
it("lets input, IME, handled events and visible dialogs take priority over dismissing",()=>{
  const {rerender}=render(<Harness initial="peek"/>);
  const input=screen.getByRole("textbox",{name:"导航搜索"});
  fireEvent.keyDown(input,{key:"Escape"});
  expect(screen.getByRole("complementary",{name:"工作面板"})).toBeVisible();
  fireEvent.compositionStart(input);fireEvent.pointerDown(document.body);fireEvent.keyDown(window,{key:"Escape"});
  expect(screen.getByRole("complementary",{name:"工作面板"})).toBeVisible();
  fireEvent.compositionEnd(input);
  const handled=new KeyboardEvent("keydown",{key:"Escape",cancelable:true});handled.preventDefault();window.dispatchEvent(handled);
  expect(screen.getByRole("complementary",{name:"工作面板"})).toBeVisible();
  rerender(<Harness initial="peek" modal/>);
  fireEvent.pointerDown(document.body);fireEvent.keyDown(window,{key:"Escape"});
  expect(screen.getByRole("complementary",{name:"工作面板"})).toBeVisible();
  rerender(<Harness initial="peek"/>);fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("complementary",{name:"工作面板"})).toBeNull();
});
it.each(["left","right"] as const)("resizes the %s fixed panel with pointer and keyboard, and resets on double click",side=>{
  render(<Harness initial="pinned" side={side}/>);
  const splitter=screen.getByRole("separator",{name:"工作面板宽度"});
  fireEvent.keyDown(splitter,{key:side==="left"?"ArrowRight":"ArrowLeft"});
  expect(splitter).toHaveAttribute("aria-valuenow","210");
  const pointer=(type:string,x:number)=>{const event=new MouseEvent(type,{bubbles:true,clientX:x,button:0});Object.defineProperty(event,"pointerId",{value:1});fireEvent(splitter,event);};
  pointer("pointerdown",200);pointer("pointermove",side==="left"?900:-900);pointer("pointerup",200);
  expect(splitter).toHaveAttribute("aria-valuenow","340");
  fireEvent.doubleClick(splitter);
  expect(splitter).toHaveAttribute("aria-valuenow","200");
  fireEvent.keyDown(splitter,{key:side==="left"?"ArrowLeft":"ArrowRight"});
  expect(splitter).toHaveAttribute("aria-valuenow","190");
  fireEvent.keyDown(splitter,{key:"Home"});
  expect(splitter).toHaveAttribute("aria-valuenow","200");
});
it("does not consume a splitter key already handled by its workspace",()=>{
  render(<div onKeyDownCapture={event=>event.preventDefault()}><Harness initial="pinned"/></div>);
  const splitter=screen.getByRole("separator",{name:"工作面板宽度"});
  fireEvent.keyDown(splitter,{key:"ArrowRight"});
  expect(splitter).toHaveAttribute("aria-valuenow","200");
});
