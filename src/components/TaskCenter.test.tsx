import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { LibraryTask } from "../types";
import { TaskCenter } from "./TaskCenter";
import {useState} from "react";
const events=vi.hoisted(()=>new Map<string,(event:{payload:string})=>void>());
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn((name:string,callback:(event:{payload:string})=>void)=>{events.set(name,callback);return Promise.resolve(()=>events.delete(name));})}));
vi.mock("../lib/api",()=>({api:{getLibraryTask:vi.fn(),listLibraryTasks:vi.fn(),queueModelThumbnailTasks:vi.fn().mockResolvedValue(0),cancelLibraryTask:vi.fn(),retryLibraryTask:vi.fn(),pauseDownloadTask:vi.fn()}}));
vi.mock("./ModelThumbnailTask",()=>({ModelThumbnailTask:()=>null}));
const task=(patch:Partial<LibraryTask>={}):LibraryTask=>({id:"task",libraryId:"lib",label:"媒体导入",status:"completed",phase:"",current:201,total:201,currentName:"",createdAt:"",updatedAt:"",cancellable:true,message:"",result:{rows:Array.from({length:201},(_,index)=>({name:`文件-${index}`,status:"imported",message:"已保存"}))},payload:{kind:"mediaImport",request:{kind:"image",paths:[],folderId:null,tags:[],favorite:false}},...patch});
describe("TaskCenter",()=>{
 it("reports a validated completed course import and requires repreview for retries",async()=>{
   const completed=task({label:"课程迁入",result:{batchId:"batch",status:"committed",receipt:{coursesCommitted:2,courseConflicts:1,seriesCommitted:1,seriesConflicts:0}},payload:{kind:"learningImport",request:{action:"commitCourse",selectionId:"selection",previewId:"preview"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入"}));
   expect(await screen.findByText("课程已提交 2，冲突 1；系列已提交 1，冲突 0。批次：batch")).toBeInTheDocument();
   expect(screen.getByRole("button",{name:"返回课程来源重新预检"})).toBeDisabled();
 });
 it.each([
   {status:"committed",receipt:{coursesCommitted:-1,courseConflicts:0,seriesCommitted:0,seriesConflicts:0}},
   {status:"committed",receipt:{coursesCommitted:1.2,courseConflicts:0,seriesCommitted:0,seriesConflicts:0}},
   {status:"committed",receipt:{coursesCommitted:1}},
   {status:"unexpected",receipt:{coursesCommitted:1,courseConflicts:0,seriesCommitted:0,seriesConflicts:0}},
 ])("does not invent a course import receipt from malformed counts or status",async result=>{
   const completed=task({label:"课程迁入",result:{batchId:"batch",...result},payload:{kind:"learningImport",request:{action:"commitCourse",selectionId:"selection",previewId:"preview"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入"}));
   expect(await screen.findByText("课程提交记录不完整，请检查批次。")).toBeInTheDocument();
 });
 it.each(["queued","running","failed","stopped","interrupted","cancelled"] as const)("does not claim a commitCourse task succeeded while %s",async status=>{
   const current=task({label:"课程迁入",status,result:{batchId:"batch",status:"committed",receipt:{coursesCommitted:1,courseConflicts:0,seriesCommitted:1,seriesConflicts:0}},payload:{kind:"learningImport",request:{action:"commitCourse",selectionId:"selection",previewId:"preview"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([current]);vi.mocked(api.getLibraryTask).mockResolvedValue(current);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:/任务中心/}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入"}));
   expect(await screen.findByText(["queued","running"].includes(status)?"课程批次任务尚未完成。":"课程批次任务未显示完成，请检查批次状态。")).toBeInTheDocument();
   expect(screen.queryByText(/课程已提交/)).not.toBeInTheDocument();
 });
 it("keeps refreshed results on an existing page when the row count shrinks",async()=>{
   const initial=task({updatedAt:"first"});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([initial]);vi.mocked(api.getLibraryTask).mockResolvedValue(initial);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   fireEvent.click(await screen.findByRole("button",{name:"媒体导入"}));await screen.findByText("文件-0");
   fireEvent.click(screen.getByRole("button",{name:"下一页"}));fireEvent.click(screen.getByRole("button",{name:"下一页"}));
   expect(screen.getByText("文件-200")).toBeInTheDocument();
   const refreshed=task({updatedAt:"second",result:{rows:[{name:"刷新后结果"}]}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([refreshed]);vi.mocked(api.getLibraryTask).mockResolvedValue(refreshed);
   act(()=>events.get("library-task-updated")?.({payload:refreshed as never}));
   expect(await screen.findByText("刷新后结果")).toBeInTheDocument();
   expect(screen.getByText("第 1 / 1 页 · 1 项")).toBeInTheDocument();
   expect(screen.getByRole("button",{name:"上一页"})).toBeDisabled();
 });
 it.each([[null],{invalid:true}])("shows an error instead of crashing on malformed task result rows",async rows=>{
   const broken=task({result:{rows,outputPath:{invalid:true}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([broken]);vi.mocked(api.getLibraryTask).mockResolvedValue(broken);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   fireEvent.click(await screen.findByRole("button",{name:"媒体导入"}));
   expect(await screen.findByText("任务逐项结果损坏，请检查任务记录。")).toBeInTheDocument();
   expect(screen.getByRole("button",{name:"收起，继续工作"})).toBeEnabled();
 });
 it("clears the old library and ignores an older list response after switching libraries",async()=>{
   const oldTask=task({id:"old",label:"旧库课程",result:{rows:[{name:"旧库结果"}]}});
   const newTask=task({id:"new",libraryId:"new-lib",label:"新库课程"});
   let finishOld!:(tasks:LibraryTask[])=>void;
   vi.mocked(api.listLibraryTasks).mockResolvedValueOnce([oldTask]).mockImplementationOnce(()=>new Promise(resolve=>{finishOld=resolve})).mockResolvedValue([newTask]);
   vi.mocked(api.getLibraryTask).mockResolvedValue(oldTask);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   fireEvent.click(await screen.findByRole("button",{name:"旧库课程"}));expect(await screen.findByText("旧库结果")).toBeInTheDocument();
   act(()=>window.dispatchEvent(new Event("library-task-started")));
   act(()=>window.dispatchEvent(new Event("library-location-changed")));
   expect(screen.queryByText("旧库结果")).not.toBeInTheDocument();
   expect(await screen.findByRole("button",{name:"新库课程"})).toBeInTheDocument();
   await act(async()=>finishOld([oldTask]));
   expect(screen.queryByRole("button",{name:"旧库课程"})).not.toBeInTheDocument();
   expect(screen.getByRole("button",{name:"新库课程"})).toBeInTheDocument();
 });
 it.each(["queued","running"] as const)("does not report a damaged course receipt while the task is %s",async status=>{
   const current=task({label:"课程迁入撤回",status,result:null,payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([current]);vi.mocked(api.getLibraryTask).mockResolvedValue(current);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:/任务中心/}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程批次任务尚未完成。")).toBeInTheDocument();expect(screen.queryByText("课程撤回记录不完整，请检查批次。")).not.toBeInTheDocument();
 });
 it.each(["failed","stopped","interrupted","cancelled"] as const)("does not claim course success when the task is %s",async status=>{
   const current=task({label:"课程迁入撤回",status,result:{batchId:"batch",status:"reverted",report:{batchId:"batch",reverted:["a"],kept:[],archivesKept:true}},payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([current]);vi.mocked(api.getLibraryTask).mockResolvedValue(current);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程批次任务未显示完成，请检查批次状态。")).toBeInTheDocument();expect(screen.queryByText(/已撤回 1 项/)).not.toBeInTheDocument();
 });
 it("refreshes the selected course task when its status changes without a new timestamp",async()=>{
   const queued=task({label:"课程迁入撤回",status:"queued",updatedAt:"same",result:null,payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   const stopped={...queued,status:"stopped" as const};
   vi.mocked(api.listLibraryTasks).mockResolvedValue([queued]);vi.mocked(api.getLibraryTask).mockResolvedValue(queued);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:/任务中心/}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程批次任务尚未完成。")).toBeInTheDocument();
   vi.mocked(api.listLibraryTasks).mockResolvedValue([stopped]);vi.mocked(api.getLibraryTask).mockResolvedValue(stopped);
   act(()=>events.get("library-task-updated")?.({payload:stopped as never}));
   expect(await screen.findByText("课程批次任务未显示完成，请检查批次状态。")).toBeInTheDocument();
 });
 it("clears the previous result when switching tasks and the next read fails",async()=>{
   const first=task({id:"first",label:"课程迁入一",result:{rows:[{name:"第一批结果"}]}});
   const second=task({id:"second",label:"课程迁入二"});
   let failSecond!:(reason:Error)=>void;
   vi.mocked(api.listLibraryTasks).mockResolvedValue([first,second]);
   vi.mocked(api.getLibraryTask).mockImplementation(id=>id==="first"?Promise.resolve(first):new Promise((_,reject)=>{failSecond=reject}));
   const notify=vi.fn();render(<TaskCenter notify={notify}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   fireEvent.click(await screen.findByRole("button",{name:"课程迁入一"}));expect(await screen.findByText("第一批结果")).toBeInTheDocument();
   fireEvent.click(screen.getByRole("button",{name:"课程迁入二"}));expect(screen.queryByText("第一批结果")).not.toBeInTheDocument();expect(screen.getByRole("status")).toHaveTextContent("正在读取任务结果");
   await act(async()=>failSecond(new Error("读取失败")));
   expect(notify).toHaveBeenCalledWith("Error: 读取失败",true);expect(screen.queryByText("第一批结果")).not.toBeInTheDocument();
   vi.mocked(api.getLibraryTask).mockResolvedValue(second);fireEvent.click(screen.getByRole("button",{name:"重新读取任务结果"}));
   expect(await screen.findByRole("heading",{name:"课程迁入二 · 结果"})).toBeInTheDocument();
 });
 it("ignores errors from a task that is no longer selected",async()=>{
   const first=task({id:"first",label:"课程迁入一"});const second=task({id:"second",label:"课程迁入二"});
   let failSecond!:(reason:Error)=>void;
   vi.mocked(api.listLibraryTasks).mockResolvedValue([first,second]);
   vi.mocked(api.getLibraryTask).mockImplementation(id=>id==="first"?Promise.resolve(first):new Promise((_,reject)=>{failSecond=reject}));
   const notify=vi.fn();render(<TaskCenter notify={notify}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   fireEvent.click(await screen.findByRole("button",{name:"课程迁入二"}));fireEvent.click(screen.getByRole("button",{name:"课程迁入一"}));
   await screen.findByText("文件-0");await act(async()=>failSecond(new Error("过期请求")));
   expect(notify).not.toHaveBeenCalled();
 });
 it("shows the course revert receipt instead of an empty result",async()=>{
   const completed=task({label:"课程迁入撤回",result:{batchId:"batch",status:"reverted",report:{batchId:"batch",reverted:["a","b"],kept:[{entryId:"c",outcome:"kept_modified",targetKind:"course",targetId:"target"}],archivesKept:true}},payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("已撤回 2 项，保留 1 项；原媒体已保留。" )).toBeInTheDocument();expect(screen.queryByText("当前步骤尚未产生逐项结果。")).not.toBeInTheDocument();
 });
 it("shows a compact course revert summary",async()=>{
   const completed=task({label:"课程迁入撤回",result:{batchId:"batch",status:"reverted",summary:{revertedCount:2,keptCount:1,archivesKept:true}},payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("已撤回 2 项，保留 1 项；原媒体已保留。" )).toBeInTheDocument();
 });
 it.each([
   {batchId:"other",status:"reverted",summary:{revertedCount:2,keptCount:1,archivesKept:true}},
   {batchId:"batch",status:"reverted",summary:{revertedCount:-1,keptCount:1,archivesKept:true}},
   {batchId:"batch",status:"reverted",summary:{revertedCount:2,keptCount:1.5,archivesKept:true}},
   {batchId:"batch",status:"reverted",summary:{revertedCount:2,keptCount:1,archivesKept:false}},
 ])("rejects invalid compact revert summaries",async result=>{
   const completed=task({label:"课程迁入撤回",result,payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程撤回记录不完整，请检查批次。")).toBeInTheDocument();
 });
 it("does not invent counts for a damaged course receipt",async()=>{
   const completed=task({label:"课程迁入撤回",result:{status:"reverted",report:{}},payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程撤回记录不完整，请检查批次。")).toBeInTheDocument();
 });
 it.each([
   {batchId:"other",status:"reverted",report:{batchId:"batch",reverted:["a"],kept:[],archivesKept:true}},
   {batchId:"batch",status:"reverted",report:{batchId:"other",reverted:["a"],kept:[],archivesKept:true}},
   {batchId:"batch",status:"reverted",report:{batchId:"batch",reverted:[null],kept:[],archivesKept:true}},
   {batchId:"batch",status:"reverted",report:{batchId:"batch",reverted:[],kept:[{entryId:"c"}],archivesKept:true}},
 ])("rejects mismatched or malformed course revert receipts",async result=>{
   const completed=task({label:"课程迁入撤回",result,payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程撤回记录不完整，请检查批次。")).toBeInTheDocument();
   expect(screen.queryByText(/已撤回 \d+ 项/)).not.toBeInTheDocument();
 });
 it("does not report cancellation for a different course batch",async()=>{
   const completed=task({label:"课程迁入取消",result:{batchId:"other",status:"cancelled"},payload:{kind:"learningImport",request:{action:"cancelCourse",batchId:"batch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入取消"}));
   expect(await screen.findByText("无法确认课程批次状态。")).toBeInTheDocument();
   expect(screen.queryByText("课程批次已取消。")).not.toBeInTheDocument();
 });
 it.each([["cancelled","课程批次已取消。"],["committed","课程批次已提交，未取消。"]])("shows a confirmed course cancellation result: %s",async(status,message)=>{
   const completed=task({label:"课程迁入取消",result:{batchId:"batch",status},payload:{kind:"learningImport",request:{action:"cancelCourse",batchId:"batch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入取消"}));
   expect(await screen.findByText(message)).toBeInTheDocument();
 });
 it("does not treat a cancellation receipt as a course revert",async()=>{
   const completed=task({label:"课程迁入撤回",result:{batchId:"batch",status:"cancelled"},payload:{kind:"learningImport",request:{action:"revertCourse",batchId:"batch",expectedEpoch:"epoch"}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([completed]);vi.mocked(api.getLibraryTask).mockResolvedValue(completed);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"课程迁入撤回"}));
   expect(await screen.findByText("课程撤回记录不完整，请检查批次。")).toBeInTheDocument();
   expect(screen.queryByText("课程批次已取消。")).not.toBeInTheDocument();
 });
 it.each(["revert", "revertCourse"] as const)("requires a new confirmation for %s",async action=>{
   vi.mocked(api.listLibraryTasks).mockResolvedValue([task({label:"撤回迁入",status:"failed",payload:{kind:"learningImport",request:{action,batchId:"batch",expectedEpoch:"epoch"}}})]);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   const button=await screen.findByRole("button",{name:"返回批次重新确认"});
   expect(button).toBeDisabled();fireEvent.click(button);expect(api.retryLibraryTask).not.toHaveBeenCalled();
 });
 it("does not replay a paid course approval through generic retry",async()=>{
   vi.mocked(api.listLibraryTasks).mockResolvedValue([task({label:"课程分段分析",status:"failed",payload:{kind:"courseAnalysis",request:{courseId:"course",previewId:"used-preview",stage:"segments"}}})]);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   const action=await screen.findByRole("button",{name:"需重新预览确认"});
   expect(action).toBeDisabled();fireEvent.click(action);expect(api.retryLibraryTask).not.toHaveBeenCalled();
   expect(screen.getByText(/旧批准不能重试/)).toBeInTheDocument();
 });
 it("shows a native exit-save failure without closing the workspace",async()=>{
   function Workspace(){const [message,setMessage]=useState("");return <><TaskCenter notify={setMessage}/>{message&&<p role="alert">{message}</p>}<button>继续编辑</button></>}
   render(<Workspace/>);
   events.get("learning-close-failed")?.({payload:"学习进度未保存，已取消退出"});
   expect(await screen.findByRole("alert")).toHaveTextContent("学习进度未保存，已取消退出");
   expect(screen.getByRole("button",{name:"继续编辑"})).toBeEnabled();
 });
 it("requires quota confirmation before retrying asset analysis",async()=>{
   vi.mocked(api.listLibraryTasks).mockResolvedValue([task({status:"failed",payload:{kind:"aiAnalyze",request:{assetId:"asset",previewId:"preview",force:false}} as never})]);vi.spyOn(window,"confirm").mockReturnValue(false);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"继续 / 重试失败项"}));expect(api.retryLibraryTask).not.toHaveBeenCalled();
 });
 it("offers pause for a download without stopping unrelated tasks",async()=>{vi.mocked(api.listLibraryTasks).mockResolvedValue([task({label:"下载",status:"running",payload:{kind:"download",request:{url:"https://example.com/video",format:"best",quality:"best",audioOnly:false,subtitles:false,outputMode:"library"}}})]);render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:/任务中心/}));fireEvent.click(await screen.findByRole("button",{name:"暂停"}));await waitFor(()=>expect(api.pauseDownloadTask).toHaveBeenCalledWith("task"));expect(api.cancelLibraryTask).not.toHaveBeenCalled();});
 it("backs off failed polling and supports explicit retry",async()=>{
   vi.mocked(api.listLibraryTasks).mockRejectedValueOnce(new Error("unavailable"));render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));await screen.findByRole("button",{name:"重新读取"});window.dispatchEvent(new Event("library-task-started"));expect(api.listLibraryTasks).toHaveBeenCalledTimes(1);fireEvent.click(screen.getByRole("button",{name:"重新读取"}));await waitFor(()=>expect(api.listLibraryTasks).toHaveBeenCalledTimes(2));expect(await screen.findByRole("button",{name:"媒体导入"})).toBeInTheDocument();
 });
 beforeEach(()=>{vi.clearAllMocks();events.clear();vi.mocked(api.listLibraryTasks).mockResolvedValue([task()]);vi.mocked(api.getLibraryTask).mockResolvedValue(task())});
 it("keeps task results paged and allows continuing work",async()=>{
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"媒体导入"}));
   expect(await screen.findByText("文件-99")).toBeInTheDocument();expect(screen.queryByText("文件-100")).not.toBeInTheDocument();
   fireEvent.click(screen.getByRole("button",{name:"下一页"}));expect(screen.getByText("文件-100")).toBeInTheDocument();
   fireEvent.click(screen.getByRole("button",{name:"收起，继续工作"}));expect(screen.queryByRole("dialog")).not.toBeInTheDocument();expect(api.cancelLibraryTask).not.toHaveBeenCalled();
 });
 it("does not offer a stop during critical maintenance",async()=>{
   vi.mocked(api.listLibraryTasks).mockResolvedValue([task({label:"备份恢复",status:"running",cancellable:false,payload:{kind:"restore",request:{path:"backup.dccassetlib"}}})]);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));expect(await screen.findByRole("button",{name:"安全维护中"})).toBeDisabled();
 });
 it("requires an explicit quota confirmation before retrying translation",async()=>{
   vi.mocked(api.listLibraryTasks).mockResolvedValue([task({status:"interrupted",payload:{kind:"translation",request:{ids:["a"]}}})]);vi.spyOn(window,"confirm").mockReturnValue(false);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));fireEvent.click(await screen.findByRole("button",{name:"继续 / 重试失败项"}));expect(api.retryLibraryTask).not.toHaveBeenCalled();
 });
 it.each(["completed","interrupted"] as const)("keeps %s UE history inspectable without replay",async status=>{
   const history=task({label:"UE 原生资产",status,payload:{kind:"ueAssets",request:{operation:"index",futureField:{nested:[1,true,null]}}}});
   vi.mocked(api.listLibraryTasks).mockResolvedValue([history]);vi.mocked(api.getLibraryTask).mockResolvedValue(history);
   render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));
   expect(await screen.findByText("旧版 UE 任务 · 只读历史")).toBeInTheDocument();
   const readOnly=screen.getByRole("button",{name:"只读历史"});expect(readOnly).toBeDisabled();fireEvent.click(readOnly);
   expect(api.retryLibraryTask).not.toHaveBeenCalled();expect(api.cancelLibraryTask).not.toHaveBeenCalled();
   fireEvent.click(screen.getByRole("button",{name:/UE 原生资产/}));expect(await screen.findByText("文件-0")).toBeInTheDocument();
 });
});
