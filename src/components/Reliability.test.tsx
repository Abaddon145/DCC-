import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {NoteRecovery} from "./NoteRecovery";
import {LearningReview} from "./LearningReview";
import {ProjectChecklist} from "./ProjectChecklist";
const invoke=vi.hoisted(()=>vi.fn());
vi.mock("@tauri-apps/api/core",()=>({invoke}));
beforeEach(()=>{invoke.mockReset();invoke.mockResolvedValue(null)});
it("blocks editing until a recovery copy is resolved and restores a separate note",async()=>{
  invoke.mockImplementation(async command=>command==="get_note_recovery"?{input:{title:"草稿"},savedAt:"2026-10-01T00:00:00Z"}:command==="restore_note_recovery_copy"?{id:"new-note"}:null);
  const pending=vi.fn(),open=vi.fn();render(<NoteRecovery libraryId="library" id="note" onPending={pending} onOpen={open} notify={vi.fn()}/>);
  await screen.findByText(/草稿/);expect(pending).toHaveBeenLastCalledWith(true);fireEvent.click(screen.getByText("恢复为新笔记"));await waitFor(()=>expect(open).toHaveBeenCalledWith("new-note"));expect(pending).toHaveBeenLastCalledWith(false);expect(invoke.mock.calls.map(v=>v[0])).not.toContain("save_note");
});
it("preserves the existing review date and mastery status",async()=>{
  invoke.mockImplementation(async command=>command==="get_note_review"?{dueDate:"2026-12-12",status:"mastered"}:null);
  render(<LearningReview libraryId="library" noteId="note" notify={vi.fn()}/>);fireEvent.click(screen.getByText("复习安排"));await waitFor(()=>expect(screen.getByLabelText("下次复习日期")).toHaveValue("2026-12-12"));expect(screen.getByLabelText("掌握情况")).toHaveValue("mastered");
});
it("previews a checklist and submits only selected tasks",async()=>{
  invoke.mockResolvedValue(4);render(<ProjectChecklist projectId="project" kind="still" onAdded={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()}/>);fireEvent.click(screen.getByText("交付清单模板"));fireEvent.click(screen.getAllByRole("checkbox")[0]);fireEvent.click(screen.getByText("添加到项目任务"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("add_project_checklist",{projectId:"project",titles:expect.any(Array)}));expect(invoke.mock.calls[0][1].titles).toHaveLength(4);
});
