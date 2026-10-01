import {cleanup,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {NoteTrashList} from "./NoteTrashList";
const invoke=vi.hoisted(()=>vi.fn());vi.mock("@tauri-apps/api/core",()=>({invoke}));
const note={id:"11111111-1111-4111-8111-111111111111",title:"已删除学习笔记",plainText:"材质学习",tags:[],favorite:false,updatedAt:"2026-09-30T00:00:00Z",revision:3,deletedAt:"now"};
beforeEach(()=>{invoke.mockReset();invoke.mockImplementation(async(command)=>{if(command==="get_workspace_context")return {libraryId:"library-a"};if(command==="search_notes")return {items:[note],total:1};if(command==="get_note")return note;return null})});afterEach(()=>{cleanup();vi.restoreAllMocks()});
it("restores a deleted note with the current revision and library",async()=>{
  render(<NoteTrashList notify={vi.fn()}/>);fireEvent.click(await screen.findByRole("button",{name:"恢复笔记"}));
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith("set_note_deleted",{libraryId:"library-a",id:note.id,expectedRevision:3,deleted:false}));
});
it("requires permanent-deletion confirmation and opens the selected note without changing it",async()=>{
  const open=vi.fn();vi.spyOn(window,"confirm").mockReturnValue(false);render(<NoteTrashList notify={vi.fn()} onOpen={open}/>);
  fireEvent.click(await screen.findByRole("button",{name:"查看笔记"}));expect(open).toHaveBeenCalledWith(note.id);
  fireEvent.click(screen.getByRole("button",{name:"永久删除笔记"}));expect(invoke.mock.calls.some(([command])=>command==="purge_note")).toBe(false);
});
