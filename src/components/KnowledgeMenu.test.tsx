import {fireEvent,render,screen,cleanup} from "@testing-library/react";
import {afterEach,describe,it,expect,vi} from "vitest";
import {KnowledgeMenu} from "./KnowledgeMenu";
afterEach(()=>{cleanup();vi.restoreAllMocks()});
describe("knowledge action menu",()=>{
  it("dismisses outside and on Escape but not when the mouse merely leaves",()=>{
    const close=vi.fn();render(<KnowledgeMenu menu={{x:100,y:100,actions:[{label:"新建",run:vi.fn()}]}} onClose={close}/>);
    fireEvent.mouseLeave(screen.getByRole("menu"));fireEvent.pointerDown(screen.getByRole("menu"));expect(close).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body);expect(close).toHaveBeenCalledTimes(1);fireEvent.keyDown(document,{key:"Escape"});expect(close).toHaveBeenCalledTimes(2);
  });
  it("does not run disabled actions and closes after choosing an action",()=>{
    const run=vi.fn(),close=vi.fn();render(<KnowledgeMenu menu={{x:100,y:100,actions:[{label:"删除",run,disabled:true},{label:"打开",run}]}} onClose={close}/>);
    fireEvent.click(screen.getByRole("menuitem",{name:"删除"}));expect(run).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("menuitem",{name:"打开"}));expect(run).toHaveBeenCalledOnce();expect(close).toHaveBeenCalledOnce();
  });
  it("keeps the popup inside the window edge",()=>{
    vi.spyOn(HTMLElement.prototype,"getBoundingClientRect").mockReturnValue({x:0,y:0,top:0,left:0,right:220,bottom:300,width:220,height:300,toJSON:()=>{}});
    render(<KnowledgeMenu menu={{x:5000,y:5000,actions:[]}} onClose={vi.fn()}/>);
    const menu=screen.getByRole("menu");expect(parseInt(menu.style.left)).toBe(window.innerWidth-228);expect(parseInt(menu.style.top)).toBe(window.innerHeight-308);
  });
});
