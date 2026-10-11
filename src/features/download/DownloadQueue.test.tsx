import {render,screen,fireEvent,cleanup} from "@testing-library/react";
import {afterEach,describe,it,expect,vi} from "vitest";
import {DownloadQueue} from "./DownloadQueue";
import type {LibraryTask} from "../../types";
afterEach(cleanup);
const task:LibraryTask={id:"task",libraryId:"lib",label:"下载",status:"paused",phase:"paused",current:0,total:0,currentName:"标题",createdAt:"",updatedAt:"",cancellable:true,message:"",result:null,payload:{kind:"download",request:{url:"https://example.com/video",format:"best",quality:"best",audioOnly:false,subtitles:false,outputMode:"library"}}};
describe("下载队列",()=>{
 it("暂停的任务可恢复、取消和打开输出",()=>{const action=vi.fn();render(<DownloadQueue tasks={[task]} pending={null} onAction={action} onOpenLibrary={vi.fn()}/>);for(const [label,kind]of [["恢复","resume"],["取消任务","cancel"],["打开输出目录","open"]]){fireEvent.click(screen.getByRole("button",{name:label}));expect(action).toHaveBeenCalledWith(task,kind);}});
 it("未知总量显示不确定进度，不冒充已完成",()=>{render(<DownloadQueue tasks={[{...task,status:"running",phase:"downloading",result:{progress:{percent:null,total:null,downloaded:10,speed:null,eta:null}}}]} pending={null} onAction={vi.fn()} onOpenLibrary={vi.fn()}/>);expect(screen.getByRole("progressbar").getAttribute("value")).toBeNull();expect(screen.queryByRole("button",{name:"查看媒体库"})).toBeNull();});
 it("入库后可直接查看对应媒体模块",()=>{const open=vi.fn();render(<DownloadQueue tasks={[{...task,status:"completed"}]} pending={null} onAction={vi.fn()} onOpenLibrary={open}/>);fireEvent.click(screen.getByRole("button",{name:"查看媒体库"}));expect(open).toHaveBeenCalledWith(false);});
});
