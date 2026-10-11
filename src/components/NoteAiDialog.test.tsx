import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {NoteAiDialog} from "./NoteAiDialog";
import {templateDocument,type KnowledgeNote} from "../lib/knowledge";
const invoke=vi.hoisted(()=>vi.fn());vi.mock("@tauri-apps/api/core",()=>({invoke}));
const note:KnowledgeNote={id:"note",notebookId:null,title:"材质学习",document:templateDocument("blank"),plainText:"材质实例可复用参数。",tags:[],favorite:false,revision:2,journalDate:null,deletedAt:null,createdAt:"",updatedAt:""};
beforeEach(()=>{invoke.mockReset();});afterEach(cleanup);
it("does not send note text before explicit confirmation and never applies the response automatically",async()=>{
  invoke.mockResolvedValue({text:"复习结果",citations:["note"],insufficientEvidence:false});const apply=vi.fn();
  render(<NoteAiDialog libraryId="library" note={note} selection="" onApply={apply} onClose={vi.fn()} onNavigate={vi.fn()}/>);
  expect(invoke).not.toHaveBeenCalled();expect(screen.getByText(note.plainText)).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"确认发送并生成"}));
  await screen.findByText("复习结果");expect(apply).not.toHaveBeenCalled();
  expect(invoke).toHaveBeenCalledWith("generate_note_ai",expect.objectContaining({request:expect.objectContaining({libraryId:"library",sources:[{id:"note",revision:2,title:note.title,text:note.plainText}]})}));
  fireEvent.click(screen.getByRole("button",{name:"追加到当前笔记"}));expect(apply).toHaveBeenCalledWith("复习结果");
});
it("limits selected text to the visible excerpt and reports service failure without writing",async()=>{
  invoke.mockRejectedValue(new Error("本月额度不足"));const apply=vi.fn();render(<NoteAiDialog libraryId="library" note={note} selection="复用参数" onApply={apply} onClose={vi.fn()} onNavigate={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"确认发送并生成"}));await waitFor(()=>expect(screen.getByRole("alert")).toHaveTextContent("本月额度不足"));expect(apply).not.toHaveBeenCalled();
  const request=invoke.mock.calls[0][1].request;expect(request.sources[0].text).toBe("复用参数");expect(JSON.stringify(request)).not.toContain("document");
});
