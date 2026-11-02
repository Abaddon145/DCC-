import {beforeEach,expect,it,vi} from "vitest";
import {invoke} from "@tauri-apps/api/core";
import {learningApi} from "./api";
import type {CommitExcerptRequest,ExcerptSelection} from "./types";
vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn()}));
beforeEach(()=>vi.mocked(invoke).mockReset().mockResolvedValue({id:"task"}));
it("sends preparation through the shared queue and strips runtime source fields",async()=>{
  const range=Object.assign({startMs:100,endMs:200},{path:"must not cross IPC"});
  const selection=Object.assign<ExcerptSelection,object>({subtitleId:"subtitle",cueIds:["cue"],ranges:[range],comment:"原文备注",frameTimeMs:150},{document:{type:"doc"},createdAt:"ignored"});
  await learningApi.startExcerptTask("library",{action:"prepare",courseId:"course",selection});
  expect(invoke).toHaveBeenCalledWith("start_library_task",{expectedLibraryId:"library",payload:{kind:"courseExcerpt",request:{action:"prepare",courseId:"course",selection:{subtitleId:"subtitle",cueIds:["cue"],ranges:[{startMs:100,endMs:200}],comment:"原文备注",frameTimeMs:150}}}});
});
it("sends only the frozen commit fields and preserves the operation identity",async()=>{
  const destination=Object.assign({kind:"append" as const,noteId:"note",expectedRevision:7,insertIndex:2},{document:{type:"doc"},createdAt:"ignored"});
  const request=Object.assign<CommitExcerptRequest,object>({operationId:"operation",excerptId:"preview",destination,includeFrame:true},{status:"pending",path:"ignored"});
  await learningApi.startExcerptTask("library",{action:"commit",request});
  expect(invoke).toHaveBeenCalledWith("start_library_task",{expectedLibraryId:"library",payload:{kind:"courseExcerpt",request:{action:"commit",request:{operationId:"operation",excerptId:"preview",destination:{kind:"append",noteId:"note",expectedRevision:7,insertIndex:2},includeFrame:true}}}});
});
it("scopes preview receipt history targets and cancellation calls without paths",async()=>{
  await learningApi.getExcerpt("library","preview");expect(invoke).toHaveBeenLastCalledWith("get_learning_excerpt",{expectedLibraryId:"library",previewId:"preview"});
  await learningApi.getExcerptOperation("library","operation");expect(invoke).toHaveBeenLastCalledWith("get_learning_excerpt_operation",{expectedLibraryId:"library",operationId:"operation"});
  await learningApi.listExcerptOperations("library","course");expect(invoke).toHaveBeenLastCalledWith("list_learning_excerpt_operations",{expectedLibraryId:"library",courseId:"course",offset:0,limit:20});
  await learningApi.excerptTargets("library");expect(invoke).toHaveBeenLastCalledWith("get_learning_excerpt_targets",{expectedLibraryId:"library"});
  await learningApi.cancelExcerptPreview("library","preview");expect(invoke).toHaveBeenLastCalledWith("cancel_learning_excerpt_preview",{expectedLibraryId:"library",previewId:"preview"});
  await learningApi.cancelExcerptOperation("library","operation");expect(invoke).toHaveBeenLastCalledWith("cancel_learning_excerpt_operation",{expectedLibraryId:"library",operationId:"operation"});
});
it("scopes project learning links by library, project and note identities",async()=>{
  await learningApi.listProjectNotes("library","project",50,50);expect(invoke).toHaveBeenLastCalledWith("list_project_learning_notes",{expectedLibraryId:"library",projectId:"project",offset:50,limit:50});
  await learningApi.linkProjectNote("library","project","note");expect(invoke).toHaveBeenLastCalledWith("link_project_learning_note",{expectedLibraryId:"library",projectId:"project",noteId:"note"});
  await learningApi.unlinkProjectNote("library","project","note");expect(invoke).toHaveBeenLastCalledWith("unlink_project_learning_note",{expectedLibraryId:"library",projectId:"project",noteId:"note"});
});
it("requests board source verification with only scoped board and item identities",async()=>{
  await learningApi.prepareBoardExcerptJump("library","board","item");expect(invoke).toHaveBeenCalledWith("prepare_learning_board_excerpt_jump",{expectedLibraryId:"library",boardId:"board",itemId:"item"});
});
it("requests a scoped page of course import entries",async()=>{
  await learningApi.listCourseBatchEntries("library","batch",100);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("list_learning_course_batch_entries",{expectedLibraryId:"library",batchId:"batch",offset:100});
});
it("sends only the scoped cursor fields for course batch pages",async()=>{
  const cursor=Object.assign({createdAt:"2026-01-01",id:"batch"},{path:"must not cross IPC"});
  await learningApi.listCourseImportBatches("library",cursor);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("list_learning_course_batches",{expectedLibraryId:"library",cursor:{createdAt:"2026-01-01",id:"batch"}});
});
