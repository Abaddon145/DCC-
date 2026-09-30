import { act, fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import KnowledgeWorkspace from "./KnowledgeWorkspace";
import { templateDocument, type KnowledgeNote } from "../lib/knowledge";
import { withKnowledgeBarrier } from "../lib/noteSaveGuard";

const mocks=vi.hoisted(()=>({invoke:vi.fn(),notify:vi.fn()}));
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
