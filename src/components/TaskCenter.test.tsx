import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { LibraryTask } from "../types";
import { TaskCenter } from "./TaskCenter";
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn().mockResolvedValue(()=>{})}));
vi.mock("../lib/api",()=>({api:{getLibraryTask:vi.fn(),listLibraryTasks:vi.fn(),queueModelThumbnailTasks:vi.fn().mockResolvedValue(0),cancelLibraryTask:vi.fn(),retryLibraryTask:vi.fn()}}));
vi.mock("./ModelThumbnailTask",()=>({ModelThumbnailTask:()=>null}));
const task=(patch:Partial<LibraryTask>={}):LibraryTask=>({id:"task",libraryId:"lib",label:"媒体导入",status:"completed",phase:"",current:201,total:201,currentName:"",createdAt:"",updatedAt:"",cancellable:true,message:"",result:{rows:Array.from({length:201},(_,index)=>({name:`文件-${index}`,status:"imported",message:"已保存"}))},payload:{kind:"mediaImport",request:{kind:"image",paths:[],folderId:null,tags:[],favorite:false}},...patch});
describe("TaskCenter",()=>{
 it("backs off failed polling and supports explicit retry",async()=>{
   vi.mocked(api.listLibraryTasks).mockRejectedValueOnce(new Error("unavailable"));render(<TaskCenter notify={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"任务中心"}));await screen.findByRole("button",{name:"重新读取"});window.dispatchEvent(new Event("library-task-started"));expect(api.listLibraryTasks).toHaveBeenCalledTimes(1);fireEvent.click(screen.getByRole("button",{name:"重新读取"}));await waitFor(()=>expect(api.listLibraryTasks).toHaveBeenCalledTimes(2));expect(await screen.findByRole("button",{name:"媒体导入"})).toBeInTheDocument();
 });
 beforeEach(()=>{vi.clearAllMocks();vi.mocked(api.listLibraryTasks).mockResolvedValue([task()]);vi.mocked(api.getLibraryTask).mockResolvedValue(task())});
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
});
