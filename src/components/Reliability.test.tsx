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
  invoke.mockImplementation(async command=>command==="list_workflow_templates"?{libraryId:"library",templates:[]}:4);render(<ProjectChecklist projectId="project" kind="still" onAdded={vi.fn().mockResolvedValue(undefined)} notify={vi.fn()}/>);fireEvent.click(screen.getByText("工作流程模板"));await screen.findByLabelText("模板名称");fireEvent.click(screen.getAllByRole("checkbox")[0]);fireEvent.click(screen.getByText("添加 4 项到项目"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("add_project_checklist",{projectId:"project",libraryId:"library",titles:expect.any(Array)}));expect(invoke.mock.calls.find(v=>v[0]==="add_project_checklist")![1].titles).toHaveLength(4);
});
it("saves reordered custom workflows in the current library without creating tasks",async()=>{
 invoke.mockImplementation(async(command,args)=>command==="list_workflow_templates"?{libraryId:"library",templates:[]}:command==="save_workflow_template"?{...args.input,id:"saved"}:null);
 render(<ProjectChecklist projectId="project" kind="scene" onAdded={vi.fn()} notify={vi.fn()}/>);fireEvent.click(screen.getByText("工作流程模板"));await screen.findByLabelText("模板名称");fireEvent.change(screen.getByLabelText("模板名称"),{target:{value:"我的 UE 流程"}});fireEvent.click(screen.getByLabelText("下移任务 1"));fireEvent.click(screen.getByText("添加任务标题"));fireEvent.change(screen.getByLabelText("清单任务 6"),{target:{value:"检查最终画面"}});fireEvent.click(screen.getByText("保存模板"));await waitFor(()=>expect(invoke).toHaveBeenCalledWith("save_workflow_template",{libraryId:"library",input:expect.objectContaining({name:"我的 UE 流程",titles:["模型拓扑与 UV 检查","场景比例与白盒检查","材质与贴图路径检查","灯光与性能检查","工程与最终输出检查","检查最终画面"]})}));expect(invoke.mock.calls.some(call=>call[0]==="add_project_checklist")).toBe(false);
});
