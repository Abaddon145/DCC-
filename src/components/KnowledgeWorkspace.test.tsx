import { fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import KnowledgeWorkspace from "./KnowledgeWorkspace";
import { templateDocument, type KnowledgeNote } from "../lib/knowledge";

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
  if(command==="list_note_revisions")return [{id:"history",noteId:note.id,snapshot:{...note,title:"旧版标题"},pinned:false,createdAt:note.createdAt}];
  return null;
});});
afterEach(cleanup);
describe("knowledge editing",()=>{
  it("opens a note and saves title with library and revision guards",async()=>{
    render(<KnowledgeWorkspace notify={mocks.notify}/>);
    fireEvent.click(await screen.findByRole("button",{name:/UE 学习笔记/}));
    const title=await screen.findByRole("textbox",{name:"笔记标题"});await waitFor(()=>expect(title).toBeEnabled());
    fireEvent.change(title,{target:{value:"材质实例学习"}});fireEvent.click(screen.getByRole("button",{name:/^保存$/}));
    await waitFor(()=>expect(mocks.invoke).toHaveBeenCalledWith("save_note",expect.objectContaining({libraryId:"test-library",input:expect.objectContaining({id:note.id,title:"材质实例学习",expectedRevision:1})})));
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
    expect(screen.getByRole("textbox",{name:"笔记标题"})).toHaveValue("旧版标题");expect(screen.getByRole("textbox",{name:"笔记标题"})).toBeDisabled();
    expect(mocks.invoke.mock.calls.some(([cmd])=>cmd==="save_note"||cmd==="restore_note_revision")).toBe(false);
    fireEvent.click(screen.getByRole("button",{name:"返回当前版本"}));expect(screen.getByRole("textbox",{name:"笔记标题"})).toHaveValue(note.title);
  });
});
