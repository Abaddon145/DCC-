import { act, fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import KnowledgeWorkspace from "./KnowledgeWorkspace";
import { templateDocument, type KnowledgeNote } from "../lib/knowledge";
import { withKnowledgeBarrier } from "../lib/noteSaveGuard";
import MindElixir from "mind-elixir";
import App from "../App";
import { StrictMode } from "react";
import {defaultGlobalPreferences,defaultLibraryPreferences} from "../lib/personalization";

const mocks=vi.hoisted(()=>({invoke:vi.fn(),notify:vi.fn()}));
Object.defineProperty(window,"matchMedia",{configurable:true,value:()=>({matches:false,addEventListener:()=>{},removeEventListener:()=>{}})});
Object.defineProperty(Range.prototype,"getClientRects",{configurable:true,value:()=>[]});
Object.defineProperty(document,"elementFromPoint",{configurable:true,value:()=>null});
Object.defineProperty(Range.prototype,"getBoundingClientRect",{configurable:true,value:()=>({top:0,left:0,right:0,bottom:0,width:0,height:0})});
vi.mock("@tauri-apps/api/core",()=>({invoke:mocks.invoke}));
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn().mockResolvedValue(()=>{})}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn().mockResolvedValue(null),save:vi.fn().mockResolvedValue(null)}));
const note:KnowledgeNote={id:"11111111-1111-4111-8111-111111111111",notebookId:null,title:"UE 学习笔记",document:templateDocument("tutorial"),plainText:"学习目标",tags:[],favorite:false,revision:1,journalDate:null,deletedAt:null,createdAt:"2026-09-30T00:00:00Z",updatedAt:"2026-09-30T00:00:00Z"};
beforeEach(()=>{mocks.invoke.mockReset();mocks.notify.mockReset();mocks.invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{
  if(command==="get_workspace_context")return {libraryId:"test-library",enabled:true,value:null};
  if(command==="search_notes")return {items:[note],total:1};
  if(command==="list_notebooks")return [];
  if(command==="get_note_backlinks")return [];
  if(command==="get_note_references")return [];
  if(command==="get_note")return note;
  if(command==="save_note")return {...note,...args?.input as object,revision:2};
  if(command==="list_note_revisions")return [{id:"history",noteId:note.id,title:"旧版标题",pinned:false,createdAt:note.createdAt}];
  if(command==="get_note_revision")return {id:"history",noteId:note.id,snapshot:{...note,title:"旧版标题"},pinned:false,createdAt:note.createdAt};
  return null;
});});
afterEach(cleanup);
describe("knowledge editing",()=>{
  it("opens document and global search without first clicking a mode",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());
    const content=document.querySelector<HTMLElement>(".knowledge-editor .tiptap")!;
    fireEvent.keyDown(content,{key:"f",ctrlKey:true});
    expect(screen.getByRole("search",{name:"文档内搜索"})).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox",{name:"查找当前笔记"}),{target:{value:"学习目标"}});
    expect(document.querySelector(".note-search-hit")).not.toBeNull();
    fireEvent.keyDown(content,{key:"f",ctrlKey:true,shiftKey:true});
    expect(screen.getByRole("dialog",{name:"搜索全部笔记"})).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("dialog",{name:"搜索全部笔记"}),{key:"Escape"});
    expect(screen.queryByRole("dialog",{name:"搜索全部笔记"})).toBeNull();
  });
  it("restores the knowledge workspace through the real application startup",async()=>{
    vi.stubGlobal("ResizeObserver",class {observe(){}unobserve(){}disconnect(){}});
    const original=mocks.invoke.getMockImplementation()!;
    const context={activeId:note.id,tabs:[{id:note.id,title:note.title,layout:"document",view:"edit",from:1,to:1,scroll:0}]};
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>{
      if(cmd==="get_workspace_context")return Promise.resolve({libraryId:"test-library",enabled:true,value:args?.moduleId==="navigation"?{view:"knowledge"}:args?.moduleId==="knowledge"?context:null});
      if(cmd==="get_personalization_settings")return Promise.resolve({global:defaultGlobalPreferences,library:{...defaultLibraryPreferences,workspaceContexts:{navigation:{view:"knowledge"},knowledge:context}}});
      if(cmd==="get_translation_settings")return Promise.resolve({contentLanguage:"zh-CN"});
      if(cmd==="get_library_meta")return Promise.resolve({categories:[],filters:{tags:[],dccTools:[],versions:[],formats:[],licenses:[]},totalAssets:723});
      if(cmd==="list_smart_collections"||cmd==="list_library_tasks")return Promise.resolve([]);
      if(cmd==="search_assets")return Promise.resolve({items:[],total:0});
      return original(cmd,args);
    });
    render(<StrictMode><App/></StrictMode>);
    await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());
    await act(async()=>{await new Promise(resolve=>setTimeout(resolve,800));});
    expect(mocks.invoke.mock.calls.length).toBeLessThan(60);
    fireEvent.click(screen.getByRole("button",{name:"阅读"}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeDisabled());
    fireEvent.click(screen.getByRole("button",{name:/^素材库$/}));
    await waitFor(()=>expect(screen.queryByRole("textbox",{name:"笔记标题"})).not.toBeInTheDocument());
  });
  it.each(["document","outline","mindMap"])("keeps restored %s startup responsive without repeated IPC or writes",async layout=>{
    const original=mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="get_workspace_context"?Promise.resolve({libraryId:"test-library",enabled:true,value:{activeId:note.id,tabs:[{id:note.id,title:note.title,layout,view:"edit",from:1,to:1,scroll:0}]}}):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());
    if(layout!=="document")await screen.findByRole("region",{name:"整篇笔记结构"});
    await act(async()=>{await new Promise(resolve=>setTimeout(resolve,800));});
    expect(mocks.invoke.mock.calls.length).toBeLessThan(30);
    expect(mocks.invoke.mock.calls.filter(([cmd])=>cmd==="save_note")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button",{name:"阅读"}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeDisabled());
  });
  it("writes real mind-map branch moves back to canonical headings and can undo them",async()=>{
    const original=MindElixir.prototype.init;let mind:MindElixir|null=null;
    const init=vi.spyOn(MindElixir.prototype,"init").mockImplementation(function(this:MindElixir,data){mind=this;return original.call(this,data);});
    try{
      render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());fireEvent.click(screen.getByRole("button",{name:"导图"}));await waitFor(()=>expect(mind).not.toBeNull());
      await act(async()=>{const instance=mind!;await instance.moveNodeIn([instance.findEle("b0")],instance.findEle("b6"));});fireEvent.click(screen.getByRole("button",{name:"保存"}));
      await waitFor(()=>{const input=mocks.invoke.mock.calls.filter(([cmd])=>cmd==="save_note").at(-1)?.[1].input;expect(input.document.content[6].attrs.level).toBe(3);expect(input.document.content[6].content[0].text).toBe("学习目标");});
      fireEvent.click(screen.getByRole("button",{name:"撤销"}));fireEvent.click(screen.getByRole("button",{name:"保存"}));await waitFor(()=>expect(mocks.invoke.mock.calls.filter(([cmd])=>cmd==="save_note").at(-1)?.[1].input.document.content[0].content[0].text).toBe("学习目标"));
    }finally{init.mockRestore();}
  });
  it("opens the whole-note mind map and preserves the original body on view changes",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());
    fireEvent.click(screen.getByRole("button",{name:"导图"}));await screen.findByRole("region",{name:"整篇笔记结构"});await waitFor(()=>expect(document.querySelectorAll(".structure-canvas me-tpc").length).toBeGreaterThan(1));
    fireEvent.click(screen.getByRole("button",{name:"正文"}));await screen.findByRole("textbox",{name:"笔记正文"});expect(mocks.invoke.mock.calls.some(([cmd])=>cmd==="save_note")).toBe(false);
  });
  it("moves a held note into a notebook with pointer events rather than native HTML drag",async()=>{
    const original=mocks.invoke.getMockImplementation()!;const id="22222222-2222-4222-8222-222222222222";
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="list_notebooks"?Promise.resolve([{id,name:"目标笔记本",parentId:null,sortOrder:0}]):cmd==="move_note_item"?Promise.resolve("undo-token"):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);const card=await screen.findByRole("button",{name:/UE 学习笔记/});const row=(await screen.findByRole("button",{name:"目标笔记本"})).closest("[data-notebook-drop]")!;
    const point=vi.spyOn(document,"elementFromPoint").mockReturnValue(row);
    const pointer=(target:Element|Window,type:string,x:number)=>{const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:100});Object.defineProperty(event,"pointerId",{value:1});fireEvent(target,event);};
    pointer(card,"pointerdown",100);pointer(window,"pointermove",120);expect(row).toHaveClass("drop-inside");pointer(window,"pointerup",120);
    await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("move_note_item",{libraryId:"test-library",kind:"note",id:note.id,parent:id,index:0}));expect(card).toHaveAttribute("draggable","false");
    fireEvent.click(await screen.findByRole("button",{name:"撤销移动"}));await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("undo_note_move",{libraryId:"test-library",token:"undo-token"}));point.mockRestore();
  });
  it("edits canonical rich content from outline and appends a template without overwriting",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeEnabled());
    fireEvent.click(screen.getByRole("button",{name:"大纲"}));const outline=await screen.findByRole("navigation",{name:"完整内容大纲"});fireEvent.click(await screen.findByRole("button",{name:"学习目标"}));
    await screen.findByRole("textbox",{name:"节点完整内容"});fireEvent.change(screen.getByRole("combobox",{name:"段落格式"}),{target:{value:"3"}});
    fireEvent.click(screen.getByRole("button",{name:"追加模板章节"}));fireEvent.click(screen.getByRole("button",{name:"保存"}));
    await waitFor(()=>{const input=mocks.invoke.mock.calls.filter(([cmd])=>cmd==="save_note").at(-1)?.[1].input;expect(input.document.content[0].attrs.level).toBe(3);expect(input.document.content).toHaveLength((note.document.content?.length||0)*2);});expect(outline).toBeInTheDocument();
  });
  it("changes note navigation color without sending color through the strict note save input",async()=>{
    const original=mocks.invoke.getMockImplementation()!;mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="set_note_color"?Promise.resolve({...note,color:args?.color}):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.contextMenu(await screen.findByRole("button",{name:/UE 学习笔记/}));fireEvent.click(screen.getByRole("menuitem",{name:"笔记颜色…"}));fireEvent.click(screen.getByRole("button",{name:"选择 #7CACCB"}));fireEvent.click(screen.getByRole("button",{name:"应用"}));
    await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("set_note_color",{libraryId:"test-library",id:note.id,color:"#7CACCB"}));expect(mocks.invoke.mock.calls.some(([cmd])=>cmd==="save_note")).toBe(false);
  });
  it("switches reading with Ctrl+E while focus remains in the note list",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    const card=await screen.findByRole("button",{name:/UE 学习笔记/});fireEvent.click(card);
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());
    fireEvent.keyDown(card,{key:"e",ctrlKey:true,isComposing:true});expect(title).toBeEnabled();
    card.focus();fireEvent.keyDown(card,{key:"e",ctrlKey:true});await waitFor(()=>expect(title).toBeDisabled());
    fireEvent.keyDown(card,{key:"e",ctrlKey:true});await waitFor(()=>expect(title).toBeEnabled());
  });
  it("switches to reading without changing the document and locks editing controls",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());
    fireEvent.click(screen.getByRole("button",{name:"阅读"}));await waitFor(()=>expect(title).toBeDisabled());
    expect(screen.getByRole("textbox",{name:"笔记正文"})).toHaveAttribute("contenteditable","false");
    expect(mocks.invoke.mock.calls.filter(([cmd])=>cmd==="save_note")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button",{name:"编辑"}));await waitFor(()=>expect(title).toBeEnabled());
  });
  it("opens each note in one tab and retains tabs when switching",async()=>{
    const second={...note,id:"22222222-2222-4222-8222-222222222222",title:"材质学习"};const original=mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="search_notes"?Promise.resolve({items:[note,second],total:2}):cmd==="get_note"?Promise.resolve(args?.id===second.id?second:note):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));fireEvent.click(await screen.findByRole("button",{name:/材质学习/}));
    await waitFor(()=>expect(screen.getAllByRole("tab")).toHaveLength(2));fireEvent.click(screen.getByRole("tab",{name:"UE 学习笔记"}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toHaveValue(note.title));expect(screen.getAllByRole("tab")).toHaveLength(2);
  });
  it("does not reopen history after a slow request finishes outside the dismissed panel",async()=>{
    const original=mocks.invoke.getMockImplementation()!;let finish!:(value:unknown)=>void;
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="list_note_revisions"?new Promise(resolve=>{finish=resolve}):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));fireEvent.click(await screen.findByRole("button",{name:"历史"}));
    await screen.findByText("正在读取历史版本…");await waitFor(()=>expect(finish).toBeDefined());fireEvent.pointerDown(screen.getByRole("textbox",{name:"搜索笔记"}));
    await act(async()=>{finish([{id:"history",title:"旧版标题",createdAt:note.createdAt,pinned:false}]);});
    expect(screen.queryByRole("complementary",{name:"历史版本"})).not.toBeInTheDocument();
  });
  it("deletes the targeted note into trash and restores it with the undo button",async()=>{
    const confirm=vi.spyOn(window,"confirm").mockReturnValue(true);const original=mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation((cmd:string,args?:Record<string,unknown>)=>cmd==="set_note_deleted"?Promise.resolve({...note,deletedAt:args?.deleted?note.createdAt:null,revision:2}):cmd==="get_note"?Promise.resolve({...note,deletedAt:note.createdAt,revision:2}):original(cmd,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.contextMenu(await screen.findByRole("button",{name:/UE 学习笔记/}));fireEvent.click(screen.getByRole("menuitem",{name:"移入回收站…"}));
    fireEvent.click(await screen.findByRole("button",{name:"撤销删除"}));await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("set_note_deleted",expect.objectContaining({id:note.id,deleted:false,expectedRevision:2})));confirm.mockRestore();
  });
  it("dismisses history outside its bounds and restores the current draft without restoring a revision",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());
    fireEvent.change(title,{target:{value:"当前草稿"}});fireEvent.click(screen.getByRole("button",{name:"历史"}));
    fireEvent.click(await screen.findByRole("button",{name:/旧版标题/}));await waitFor(()=>expect(title).toHaveValue("旧版标题"));
    fireEvent.pointerDown(screen.getByRole("textbox",{name:"搜索笔记"}));
    expect(screen.queryByRole("complementary",{name:"历史版本"})).not.toBeInTheDocument();expect(title).toHaveValue("当前草稿");await waitFor(()=>expect(title).toBeEnabled());
    expect(mocks.invoke.mock.calls.some(([cmd])=>cmd==="restore_note_revision")).toBe(false);
    fireEvent.click(screen.getByRole("button",{name:"历史"}));await screen.findByRole("complementary",{name:"历史版本"});fireEvent.keyDown(document,{key:"Escape"});
    expect(screen.queryByRole("complementary",{name:"历史版本"})).not.toBeInTheDocument();
  });
  it("keeps history open for inside interactions but closes on the history button",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    fireEvent.click(await screen.findByRole("button",{name:"历史"}));const panel=await screen.findByRole("complementary",{name:"历史版本"});fireEvent.pointerDown(panel);
    expect(panel).toBeInTheDocument();fireEvent.click(screen.getByRole("button",{name:"历史"}));expect(panel).not.toBeInTheDocument();
  });
  it("renames the right-clicked note without requiring it to be opened",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.contextMenu(await screen.findByRole("button",{name:/UE 学习笔记/}),{clientX:400,clientY:200});
    fireEvent.click(screen.getByRole("menuitem",{name:"重命名…"}));const input=await screen.findByRole("textbox",{name:"新笔记名称"});fireEvent.change(input,{target:{value:"新的标题"}});
    fireEvent.click(screen.getByRole("button",{name:"保存"}));await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("save_note",expect.objectContaining({input:expect.objectContaining({id:note.id,title:"新的标题",expectedRevision:1})})));
  });
  it("exposes paragraph levels, a heading outline and context-dependent editing actions",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const format=await screen.findByRole("combobox",{name:"段落格式"});await waitFor(()=>expect(format).toBeEnabled());
    fireEvent.change(format,{target:{value:"1"}});fireEvent.click(screen.getByRole("button",{name:"标题大纲"}));expect(await screen.findByRole("navigation",{name:"笔记大纲"})).toHaveTextContent("学习目标");
    fireEvent.contextMenu(screen.getByRole("textbox",{name:"笔记正文"}),{clientX:700,clientY:500});expect(screen.getByRole("menuitem",{name:"三级标题"})).toBeEnabled();
    fireEvent.click(screen.getByRole("menuitem",{name:"三级标题"}));expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button",{name:"保存"}));await waitFor(()=>expect(mocks.invoke.mock.calls.some(([cmd,args])=>cmd==="save_note"&&JSON.stringify(args.input.document).includes('"level":3'))).toBe(true));
  });
  it("shows a recoverable context error instead of remaining on the loading screen",async()=>{
    const original=mocks.invoke.getMockImplementation()!;let fail=true;
    mocks.invoke.mockImplementation((command:string,args?:Record<string,unknown>)=>command==="get_workspace_context"&&fail?Promise.reject("未知的浏览模块"):original(command,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    expect(await screen.findByRole("alert")).toHaveTextContent("未知的浏览模块");
    expect(screen.queryByText("正在打开知识库…")).not.toBeInTheDocument();
    fail=false;fireEvent.click(screen.getByRole("button",{name:"重新打开知识库"}));
    expect(await screen.findByRole("button",{name:/UE 学习笔记/})).toBeInTheDocument();
  });
  it("opens a note and saves title with library and revision guards",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());
    fireEvent.change(title,{target:{value:"材质实例学习"}});fireEvent.click(screen.getByRole("button",{name:/^保存$/}));
    await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("save_note",expect.objectContaining({libraryId:"test-library",input:expect.objectContaining({id:note.id,title:"材质实例学习",expectedRevision:1})})));
    const savedInput=mocks.invoke.mock.calls.find(([cmd])=>cmd==="save_note")?.[1].input;
    expect(Object.keys(savedInput).sort()).toEqual(["id","expectedRevision","notebookId","title","document","tags","favorite","journalDate"].sort());
    await waitFor(()=>expect(screen.getByRole("status")).toHaveTextContent("已保存"));
  });
  it("retains content and exposes retry when saving fails",async()=>{
    const original=mocks.invoke.getMockImplementation()!;mocks.invoke.mockImplementation((command:string,args?:Record<string,unknown>)=>command==="save_note"?Promise.reject(new Error("磁盘空间不足")):original(command,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());fireEvent.change(title,{target:{value:"不能丢失"}});fireEvent.click(screen.getByRole("button",{name:/^保存$/}));
    expect(await screen.findByRole("alert")).toHaveTextContent("磁盘空间不足");expect(title).toHaveValue("不能丢失");expect(screen.getByRole("button",{name:"重试保存"})).toBeEnabled();
  });
  it("previews history without saving or silently restoring it",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    fireEvent.click(await screen.findByRole("button",{name:"历史"}));fireEvent.click(await screen.findByRole("button",{name:/旧版标题/}));
    await waitFor(()=>expect(screen.getByRole("textbox",{name:"笔记标题"})).toHaveValue("旧版标题"));expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeDisabled();
    expect(mocks.invoke.mock.calls.some(([cmd])=>cmd==="save_note"||cmd==="restore_note_revision")).toBe(false);
    fireEvent.click(screen.getByRole("button",{name:"返回当前版本"}));expect(screen.getByRole("textbox",{name:"笔记标题"})).toHaveValue(note.title);
  });
  it("waits for admitted screenshot imports before a library backup and saves the inserted image",async()=>{
    const original=mocks.invoke.getMockImplementation()!;let finish!:(value:unknown)=>void;
    mocks.invoke.mockImplementation((command:string,args?:Record<string,unknown>)=>command==="paste_note_clipboard_image"?new Promise(resolve=>{finish=resolve}):original(command,args));
    render(<KnowledgeWorkspace notify={mocks.notify}/>);fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    fireEvent.click(await screen.findByRole("button",{name:"粘贴截图"}));
    const backup=vi.fn();let barrier!:Promise<void>;
    act(()=>{barrier=withKnowledgeBarrier(async()=>{backup()})});
    expect(backup).not.toHaveBeenCalled();
    await act(async()=>{finish({id:"22222222-2222-4222-8222-222222222222",filename:"截图.png",mime:"image/png"});await barrier});
    expect(backup).toHaveBeenCalledOnce();
    const saved=mocks.invoke.mock.calls.find(([command])=>command==="save_note");
    expect(JSON.stringify(saved?.[1].input.document)).toContain("http://dcc-media.localhost/note/22222222-2222-4222-8222-222222222222");
  });
});
