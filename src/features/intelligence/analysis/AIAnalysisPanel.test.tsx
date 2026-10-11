import {render,screen,fireEvent,waitFor,cleanup} from "@testing-library/react";
import {it,expect,vi,beforeEach,afterEach} from "vitest";
import {AIAnalysisPanel} from "./AIAnalysisPanel";
import {intelligenceApi} from "../api";
import {api} from "../../../lib/api";
vi.mock("../api",()=>({intelligenceApi:{prepareAnalysis:vi.fn(),getAnalysis:vi.fn(),applyAnalysisTags:vi.fn()}}));
vi.mock("../../../lib/api",()=>({api:{startLibraryTask:vi.fn(),getLibraryTask:vi.fn(),cancelLibraryTask:vi.fn()}}));
const calls=intelligenceApi as unknown as {prepareAnalysis:ReturnType<typeof vi.fn>;getAnalysis:ReturnType<typeof vi.fn>;applyAnalysisTags:ReturnType<typeof vi.fn>};
const preview={id:"preview",assetId:"asset",model:"deepseek-flash",mode:"visual",summary:'{"name":"城市图"}',fingerprint:"hash",images:[{dataUrl:"data:image/jpeg;base64,abc",label:"图片预览",timestamp:null}],estimatedTokens:6700,warnings:["基于缩小预览分析"],cachedAnalysisId:null};
const saved={id:"analysis",assetId:"asset",description:"雨夜城市",keywords:["雨夜","电影感"],scene:"城市",style:"写实",camera:"广角",lighting:"霓虹",mood:"安静",model:"deepseek-flash",mode:"visual",inputFingerprint:"hash",formatVersion:1,actualTokens:64,createdAt:"now"};
const task={id:"task",libraryId:"lib",status:"running",payload:{kind:"aiAnalyze",request:{assetId:"asset",previewId:"preview",force:false}},message:"",result:null};
const props={libraryId:"lib",assetId:"asset",locale:"zh-CN",onChanged:vi.fn()};
beforeEach(()=>{vi.clearAllMocks();calls.prepareAnalysis.mockResolvedValue(preview);calls.getAnalysis.mockResolvedValue(null);vi.mocked(api.startLibraryTask).mockResolvedValue(task as never);vi.mocked(api.getLibraryTask).mockResolvedValue(task as never)});
afterEach(cleanup);
it("预览和明确确认后才提交，重复点击只提交一次",async()=>{
 render(<AIAnalysisPanel {...props}/>);expect(api.startLibraryTask).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"预览分析资料"}));expect(await screen.findByAltText("图片预览")).toBeTruthy();expect(screen.getByText(/6700/)).toBeTruthy();expect(api.startLibraryTask).not.toHaveBeenCalled();
 let resolve:(v:unknown)=>void=()=>{};vi.mocked(api.startLibraryTask).mockReturnValueOnce(new Promise(r=>{resolve=r}) as never);
 fireEvent.click(screen.getByRole("button",{name:"确认并开始分析"}));fireEvent.click(screen.getByRole("button",{name:"确认并开始分析"}));
 await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledTimes(1));expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"aiAnalyze",request:{assetId:"asset",previewId:"preview",force:false}},"lib");resolve(task);
});
it("本地缓存仍需确认，标签只追加勾选项",async()=>{
 calls.prepareAnalysis.mockResolvedValue({...preview,cachedAnalysisId:"analysis"});calls.getAnalysis.mockResolvedValue(saved);
 render(<AIAnalysisPanel {...props}/>);expect(await screen.findByText("雨夜城市")).toBeTruthy();
 fireEvent.click(screen.getByRole("checkbox",{name:"电影感"}));fireEvent.click(screen.getByRole("button",{name:"追加所选标签"}));await waitFor(()=>expect(calls.applyAnalysisTags).toHaveBeenCalledWith("lib","asset","analysis","zh-CN",["电影感"]));
 fireEvent.click(screen.getByRole("button",{name:"预览分析资料"}));await screen.findByText(/本地缓存/);expect(api.startLibraryTask).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"确认并复用缓存"}));await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledTimes(1));
});
it("运行可以取消，失败明确显示且可重新预览",async()=>{
 render(<AIAnalysisPanel {...props}/>);fireEvent.click(screen.getByRole("button",{name:"预览分析资料"}));await screen.findByRole("button",{name:"确认并开始分析"});fireEvent.click(screen.getByRole("button",{name:"确认并开始分析"}));
 fireEvent.click(await screen.findByRole("button",{name:"停止分析"}));await waitFor(()=>expect(api.cancelLibraryTask).toHaveBeenCalledWith("task"));
 vi.mocked(api.getLibraryTask).mockResolvedValue({...task,status:"failed",message:"额度不足"} as never);
 window.dispatchEvent(new CustomEvent("library-task-completed",{detail:{...task,status:"failed"}}));expect(await screen.findByText("额度不足")).toBeTruthy();expect(screen.getByRole("button",{name:"预览分析资料"})).toBeTruthy();
});
it("切库后丢弃迟到的预览和标签反馈",async()=>{
 let resolve:(v:unknown)=>void=()=>{};calls.prepareAnalysis.mockReturnValueOnce(new Promise(r=>{resolve=r}));
 const {rerender}=render(<AIAnalysisPanel {...props}/>);fireEvent.click(screen.getByRole("button",{name:"预览分析资料"}));rerender(<AIAnalysisPanel {...props} libraryId="other"/>);resolve(preview);
 await waitFor(()=>expect(screen.queryByAltText("图片预览")).toBeNull());expect(api.startLibraryTask).not.toHaveBeenCalled();
});
it("强制重新分析需再次确认额度，完成后刷新结果",async()=>{
 calls.getAnalysis.mockResolvedValue(saved);render(<AIAnalysisPanel {...props}/>);await screen.findByText("雨夜城市");
 fireEvent.click(screen.getByRole("button",{name:"预览分析资料"}));await screen.findByAltText("图片预览");fireEvent.click(screen.getByRole("checkbox",{name:/强制重新分析/}));
 vi.spyOn(window,"confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);fireEvent.click(screen.getByRole("button",{name:"确认并开始分析"}));expect(api.startLibraryTask).not.toHaveBeenCalled();await waitFor(()=>expect(screen.getByRole("button",{name:"确认并开始分析"})).not.toBeDisabled());fireEvent.click(screen.getByRole("button",{name:"确认并开始分析"}));await screen.findByRole("button",{name:"停止分析"});
 calls.getAnalysis.mockImplementation(()=>new Promise(resolve=>window.setTimeout(()=>resolve({...saved,description:"新的分析结果"}),30)));vi.mocked(api.getLibraryTask).mockResolvedValue({...task,status:"completed",result:{analysisId:"analysis"}} as never);window.dispatchEvent(new CustomEvent("library-task-completed",{detail:{...task,status:"completed"}}));expect(await screen.findByText("新的分析结果")).toBeTruthy();expect(screen.queryByRole("button",{name:"停止分析"})).toBeNull();
});
