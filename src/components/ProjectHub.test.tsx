import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetDetail, CreativeProject, MediaEntry, ProjectSummary, ProjectUnit } from "../types";
import { api } from "../lib/api";
import { ProjectHub } from "./ProjectHub";
import {intelligenceApi} from "../features/intelligence/api";
import type {ShotAsset} from "../features/intelligence/types";

vi.mock("../lib/api", () => ({ mediaLibraryBundleUrl:(id:string,path:string)=>`http://dcc-media.localhost/library/bundle/${id}/${path}`,mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`, api: {
  listProjectAssets: vi.fn(), listProjectTasks: vi.fn(), updateProjectAssets: vi.fn(), getProjectUnitImpact: vi.fn(), listProjects: vi.fn(), getProject: vi.fn(), moveProjectTask: vi.fn(), listReferenceBoards: vi.fn(),
  unlinkProjectMedia: vi.fn().mockResolvedValue(1), searchMediaEntries: vi.fn(), saveProject: vi.fn(), setProjectShotVideo: vi.fn(), getAsset: vi.fn(), getMediaEntry:vi.fn(),
} }));
vi.mock("./DetailPanel", () => ({ DetailPanel: ({ asset }: { asset: AssetDetail }) => <aside className="detail-panel" aria-label="项目素材详情">{asset.name}</aside> }));
vi.mock("../features/intelligence/api",()=>({intelligenceApi:{listShots:vi.fn().mockResolvedValue({items:[],total:0,offset:0,limit:200})}}));
vi.mock("../features/intelligence/shot/ShotPanel",()=>({ShotPanel:({unitId,onSaved}:{unitId?:string;onSaved:(id:string)=>void})=><div>正在登记 {unitId}<button onClick={()=>onSaved("registered")}>完成登记</button></div>}));
vi.mock("../features/assets/AssetDetail",()=>({AssetDetail:({onClose}:{onClose:()=>void})=><aside><button onClick={onClose}>关闭 Shot 详情</button></aside>}));
beforeEach(()=>vi.mocked(intelligenceApi.listShots).mockReset().mockResolvedValue({items:[],total:0,offset:0,limit:200}));

const summary: ProjectSummary = {
  id: "project-1", name: "遗迹短片", description: "UE 镜头练习", projectType: "animation", status: "active",
  targetTools: ["Unreal Engine"], versions: ["5.5"], resolutionWidth: 1920, resolutionHeight: 1080, frameRate: 24,
  coverAssetId: null, coverImageId: null, coverMediaEntryId: null, coverMediaFileId: null, assetCount: 0, unavailableAssetCount: 0, taskCount: 1,
  completedTaskCount: 0, reviewTaskCount: 0, progress: 0, mainBoardId: null,
  createdAt: "2026-01-01", updatedAt: "2026-01-02", lastOpenedAt: null, archivedAt: null,
};
const detail: CreativeProject = {
  ...summary, units: [], assets: [], media: [], boards: [], paths: [], tasks: [{
    id: "task-1", projectId: summary.id, unitId: null, title: "完成灯光", description: "", status: "todo",
    priority: "high", dueDate: null, sortOrder: 0, assetIds: [], createdAt: "2026-01-01", updatedAt: "2026-01-01",
  }],
};
const image: MediaEntry = {
  id: "image-1", kind: "image", folderId: null, name: "概念封面", description: "", author: "", sourceUrl: "", license: "",
  favorite: false, processingStatus: "ready", processingMessage: "", format: "png", fileSize: 512, tags: [],
  thumbnailFileId: "thumb-1", primaryFileId: "file-1", createdAt: "2026-01-01", updatedAt: "2026-01-01",
};

async function pointerDrop(source:HTMLElement,target:HTMLElement){
  vi.stubGlobal("PointerEvent",MouseEvent);
  Object.defineProperty(document,"elementFromPoint",{configurable:true,value:()=>target});
  fireEvent.pointerDown(source,{button:0,clientX:0,clientY:0});
  fireEvent.pointerMove(window,{clientX:20,clientY:1});
  fireEvent.pointerUp(window,{clientX:20,clientY:1});
  await act(async()=>{await new Promise(resolve=>setTimeout(resolve,110));});
}

describe("ProjectHub", () => {
  it("reopens the exact project video and originating tab even without remembered workspace",async()=>{
    const video={...image,id:"video",kind:"video" as const,name:"课程",format:"mp4",projectIds:[summary.id],assetIds:[],files:[{id:"file-1",entryId:"video",role:"main" as const,logicalPath:"originals/video.mp4",originalName:"video.mp4",mimeType:"video/mp4",fileSize:512,checksum:"sha",width:320,height:180,durationMs:60000}]};
    vi.mocked(api.getProject).mockResolvedValue({...detail,media:[video]});vi.mocked(api.getMediaEntry).mockResolvedValue(video);
    const onLearn=vi.fn();
    render(<ProjectHub libraryId="lib" initialProjectId={summary.id} initialMediaId="video" initialContext={{tab:"media",scroll:123}} onLearn={onLearn} contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);
    fireEvent.click(await screen.findByRole("button",{name:"学习此视频"}));
    expect(screen.getByRole("button",{name:"关联媒体 1"})).toHaveClass("active");
    expect(onLearn).toHaveBeenCalledWith("video",summary.id,{tab:"media",scroll:123});
  });
  beforeEach(() => {
    vi.mocked(api.listProjects).mockResolvedValue([summary]);
    vi.mocked(api.getProject).mockResolvedValue(detail);
    vi.mocked(api.listProjectAssets).mockImplementation(async(id,language,query,status,offset=0,limit=60)=>{const project=await api.getProject(id,language,false);const rows=project.assets.filter(row=>(!status||row.status===status)&&(!query||[row.name,row.categoryName,row.note,row.purpose].some(value=>value?.includes(query))));return {items:rows.slice(offset,offset+limit),total:rows.length,offset,limit}});
    vi.mocked(api.listProjectTasks).mockImplementation(async(id,status,offset=0,limit=40)=>{const project=await api.getProject(id,"zh-CN",false);const rows=project.tasks.filter(row=>!status||row.status===status);return {items:rows.slice(offset,offset+limit),total:rows.length,offset,limit}});
    vi.mocked(api.updateProjectAssets).mockResolvedValue(1);
    vi.mocked(api.moveProjectTask).mockResolvedValue();
    vi.mocked(api.listReferenceBoards).mockResolvedValue([]);
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [image], total: 1, offset: 0, limit: 100 });
    vi.mocked(api.saveProject).mockResolvedValue(detail);
    vi.mocked(api.setProjectShotVideo).mockResolvedValue();
  });

  it("removes only a project association, not the media original",async()=>{
    vi.mocked(api.getProject).mockResolvedValue({...detail,media:[image]});vi.spyOn(window,"confirm").mockReturnValue(true);
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);fireEvent.click(await screen.findByRole("button",{name:/打开工作区/}));fireEvent.click(await screen.findByRole("button",{name:/关联媒体 1/}));fireEvent.click(await screen.findByRole("button",{name:"移出项目"}));await waitFor(()=>expect(api.unlinkProjectMedia).toHaveBeenCalledWith(["image-1"],["project-1"]));
  });

  it("opens a project and moves a task across the fixed kanban columns", async () => {
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /任务 1/ }));
    const task = await screen.findByText("完成灯光");
    const doneColumn = screen.getByText("完成", { selector: "strong" }).closest("section")!;
    await pointerDrop(task.closest("article")!,doneColumn);
    await waitFor(() => expect(api.moveProjectTask).toHaveBeenCalledWith("task-1", "done", 0));
  });

  it("allows an existing image-library entry to become the project cover", async () => {
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: "项目设置" }));
    fireEvent.click(await screen.findByRole("button", { name: "概念封面" }));
    fireEvent.click(screen.getByRole("button", { name: "保存项目" }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledWith(expect.objectContaining({ coverMediaEntryId: "image-1", coverAssetId: null })));
  });

  it("blocks duplicate project saves and preserves the editor after a failed save", async () => {
    const notify = vi.fn();
    let reject!: (error: Error) => void;
    vi.mocked(api.saveProject).mockImplementationOnce(() => new Promise((_, failure) => { reject = failure; }));
    render(<ProjectHub contentLanguage="zh-CN" notify={notify} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: "项目设置" }));
    fireEvent.change(screen.getByLabelText("项目名称"), { target: { value: "保留我的草稿" } });
    fireEvent.click(screen.getByRole("button", { name: "保存项目" }));
    fireEvent.click(screen.getByRole("button", { name: "保存中…" }));
    expect(api.saveProject).toHaveBeenCalledTimes(1);
    await act(async () => reject(new Error("磁盘写入失败")));
    expect(screen.getByLabelText("项目名称")).toHaveValue("保留我的草稿");
    expect(notify).toHaveBeenCalledWith(expect.stringContaining("磁盘写入失败"), true);
    fireEvent.click(screen.getByRole("button", { name: "保存项目" }));
    await waitFor(() => expect(api.saveProject).toHaveBeenCalledTimes(2));
  });

  it("shows a retry on failed kanban reads instead of treating them as empty columns", async () => {
    vi.mocked(api.listProjectTasks).mockRejectedValueOnce(new Error("读取失败"));
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /任务 1/ }));
    fireEvent.click(await screen.findByRole("button", { name: "重新读取" }));
    expect(await screen.findByText("完成灯光")).toBeInTheDocument();
  });

  it("assigns a replaceable video-library entry to a shot", async () => {
    const scene: ProjectUnit = { id: "scene-1", projectId: summary.id, parentId: null, kind: "scene", name: "场景一", description: "", startFrame: null, endFrame: null, resolutionWidth: null, resolutionHeight: null, frameRate: null, sortOrder: 0, videoMediaEntryId: null, videoStreamFileId: null, videoName: null, createdAt: "now", updatedAt: "now" };
    const shot: ProjectUnit = { ...scene, id: "shot-1", parentId: scene.id, kind: "shot", name: "SH010", startFrame: 1, endFrame: 100 };
    vi.mocked(api.getProject).mockResolvedValue({ ...detail, units: [scene, shot] });
    vi.mocked(api.searchMediaEntries).mockResolvedValue({ items: [{ ...image, id: "video-1", kind: "video", name: "预演视频", format: "mp4" }], total: 1, offset: 0, limit: 100 });
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: "场景 / 镜头" }));
    fireEvent.click(screen.getByRole("button", { name: "从视频库选择" }));
    fireEvent.click(await screen.findByRole("button", { name: /预演视频/ }));
    await waitFor(() => expect(api.setProjectShotVideo).toHaveBeenCalledWith("shot-1", "video-1"));
  });

  it("closes project asset details on outside click or Escape and switches cards directly", async () => {
    vi.mocked(api.getProject).mockResolvedValue({ ...detail, assetCount: 2, assets: [
      { assetId: "asset-a", name: "沙漠", coverImageId: null, categoryName: "环境", status: "candidate", purpose: "", note: "", unitIds: [], unavailable: false, updatedAt: "now" },
      { assetId: "asset-b", name: "古城", coverImageId: null, categoryName: "环境", status: "candidate", purpose: "", note: "", unitIds: [], unavailable: false, updatedAt: "now" },
    ] });
    vi.mocked(api.getAsset).mockImplementation(async id => ({ id, name: id === "asset-a" ? "沙漠" : "古城" } as AssetDetail));
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /项目素材 2/ }));
    fireEvent.click(await screen.findByText("沙漠", { selector: ".project-asset-grid strong" }));
    expect(await screen.findByLabelText("项目素材详情")).toHaveTextContent("沙漠");
    fireEvent.click(await screen.findByText("古城", { selector: ".project-asset-grid strong" }));
    expect(await screen.findByLabelText("项目素材详情")).toHaveTextContent("古城");
    fireEvent.pointerDown(screen.getByPlaceholderText("搜索项目素材、分类、用途或备注…"));
    await waitFor(() => expect(screen.queryByLabelText("项目素材详情")).not.toBeInTheDocument());
    fireEvent.click(await screen.findByText("沙漠", { selector: ".project-asset-grid strong" }));
    expect(await screen.findByLabelText("项目素材详情")).toHaveTextContent("沙漠");
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByLabelText("项目素材详情")).not.toBeInTheDocument());
  });

  it("ignores older detail responses and does not reopen a dismissed detail", async () => {
    vi.mocked(api.getProject).mockResolvedValue({ ...detail, assetCount: 2, assets: [
      { assetId: "asset-a", name: "沙漠", coverImageId: null, categoryName: "环境", status: "candidate", purpose: "", note: "", unitIds: [], unavailable: false, updatedAt: "now" },
      { assetId: "asset-b", name: "古城", coverImageId: null, categoryName: "环境", status: "candidate", purpose: "", note: "", unitIds: [], unavailable: false, updatedAt: "now" },
    ] });
    let finish!: (asset: AssetDetail) => void;
    vi.mocked(api.getAsset).mockImplementation(id => id === "asset-a" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve({ id, name: "古城" } as AssetDetail));
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /打开工作区/ }));
    fireEvent.click(await screen.findByRole("button", { name: /项目素材 2/ }));
    fireEvent.click(await screen.findByText("沙漠", { selector: ".project-asset-grid strong" }));
    fireEvent.click(await screen.findByText("古城", { selector: ".project-asset-grid strong" }));
    expect(await screen.findByLabelText("项目素材详情")).toHaveTextContent("古城");
    await act(async () => { finish({ id: "asset-a", name: "沙漠" } as AssetDetail); });
    await waitFor(() => expect(screen.getByLabelText("项目素材详情")).toHaveTextContent("古城"));
    fireEvent.click(await screen.findByText("沙漠", { selector: ".project-asset-grid strong" }));
    fireEvent.pointerDown(screen.getByPlaceholderText("搜索项目素材、分类、用途或备注…"));
    await act(async () => { finish({ id: "asset-a", name: "沙漠" } as AssetDetail); });
    await waitFor(() => expect(screen.queryByLabelText("项目素材详情")).not.toBeInTheDocument());
  });

  it("keeps cross-page selections and preserves long notes after a failed save", async () => {
    const assets = Array.from({length:61},(_,index)=>({assetId:`asset-${index}`,name:`素材 ${index}`,coverImageId:null,categoryName:null,status:"candidate" as const,purpose:"用途",note:"原备注",unitIds:[],unavailable:false,updatedAt:"now"}));
    vi.mocked(api.getProject).mockResolvedValue({...detail,assets,assetCount:61});
    vi.mocked(api.updateProjectAssets).mockRejectedValueOnce(new Error("磁盘写入失败")).mockResolvedValueOnce(2);
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);
    fireEvent.click(await screen.findByRole("button",{name:/打开工作区/}));
    fireEvent.click(await screen.findByRole("button",{name:/项目素材 61/}));
    await screen.findByText("素材 0",{selector:".project-asset-grid strong"});
    expect(screen.queryByText("素材 60",{selector:".project-asset-grid strong"})).toBeNull();
    fireEvent.click(screen.getAllByRole("button",{name:"选择项目素材"})[0]);
    fireEvent.click(screen.getByRole("button",{name:"下一页"}));
    await screen.findByText("素材 60",{selector:".project-asset-grid strong"});
    fireEvent.click(screen.getByRole("button",{name:"选择项目素材"}));
    fireEvent.click(screen.getByRole("button",{name:"用途与备注"}));
    const text="用于沙漠镜头\n需要调整材质与灯光";
    fireEvent.click(screen.getByRole("checkbox",{name:"修改备注"}));
    fireEvent.change(screen.getByRole("textbox",{name:"修改备注"}),{target:{value:text}});
    fireEvent.click(screen.getByRole("button",{name:"保存"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("磁盘写入失败");
    expect(screen.getByRole("textbox",{name:"修改备注"})).toHaveValue(text);
    fireEvent.click(screen.getByRole("button",{name:"保存"}));
    await waitFor(()=>expect(screen.queryByRole("dialog",{name:"编辑项目用途与备注"})).toBeNull());
    expect(api.updateProjectAssets).toHaveBeenLastCalledWith({projectId:"project-1",assetIds:["asset-0","asset-60"],note:text});
  });

  it("uses whole-column positions when dragging a task on a later page",async()=>{
    vi.mocked(api.getProject).mockResolvedValue({...detail,taskCount:42,tasks:Array.from({length:42},(_,index)=>({...detail.tasks[0],id:`task-${index}`,title:`任务 ${index}`,sortOrder:index}))});
    render(<ProjectHub contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);
    fireEvent.click(await screen.findByRole("button",{name:/打开工作区/}));
    fireEvent.click(await screen.findByRole("button",{name:/任务 42/}));
    await screen.findByText("任务 0");
    fireEvent.click(screen.getAllByRole("button",{name:"下一页"})[0]);
    const task=await screen.findByText("任务 40");
    await pointerDrop(task.closest("article")!,screen.getByText("任务 41").closest("article")!);
    await waitFor(()=>expect(api.moveProjectTask).toHaveBeenCalledWith("task-40","todo",41));
  });
});

function shotProject():CreativeProject {
 const unit={id:"unit-shot",projectId:summary.id,parentId:"scene",kind:"shot",name:"航拍",description:"",startFrame:1,endFrame:100,sortOrder:0} as ProjectUnit;
 return {...detail,units:[{...unit,id:"scene",kind:"scene",parentId:null,name:"沙漠"},unit]};
}
function registeredShot(unitId:string):ShotAsset {return {asset:{id:`core-${unitId}`,name:"航拍",assetType:"shot",description:"",thumbnail:null,tags:[],origin:{kind:"entity",id:`entity-${unitId}`},deleted:false,createdAt:"now",updatedAt:"now"},revision:1,projectId:summary.id,projectUnitId:unitId,act:"ACT1",shotNumber:"001",status:"todo",unit:null,createdAt:"now",updatedAt:"now"};}
async function openShotUnits(){
 vi.mocked(api.listProjects).mockResolvedValue([summary]);
 vi.mocked(api.getProject).mockImplementation(async()=>shotProject());
 render(<ProjectHub libraryId="lib" contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);
 fireEvent.click(await screen.findByRole("button",{name:/打开工作区/}));
 fireEvent.click(await screen.findByRole("button",{name:"场景 / 镜头"}));
}
it("登记后即使项目更新时间不变也刷新 Shot 身份",async()=>{
 let registered=false;vi.mocked(intelligenceApi.listShots).mockImplementation(async()=>({items:registered?[registeredShot("unit-shot")]:[],total:registered?1:0,offset:0,limit:200}));
 await openShotUnits();const button=await screen.findByRole("button",{name:"登记为 Shot Asset"});await waitFor(()=>expect(button).not.toBeDisabled());fireEvent.click(button);
 registered=true;fireEvent.click(await screen.findByRole("button",{name:"完成登记"}));fireEvent.click(await screen.findByRole("button",{name:"关闭 Shot 详情"}));
 expect(await screen.findByRole("button",{name:"打开 Shot 资产"})).toBeEnabled();
});
it("完整分页读取第201项 Shot，不将未加载记录当作未登记",async()=>{
 const rows=[...Array.from({length:200},(_,i)=>registeredShot(`other-${i}`)),registeredShot("unit-shot")];
 vi.mocked(intelligenceApi.listShots).mockImplementation(async(_lib,_id,offset=0)=>({items:rows.slice(offset,offset+200),total:201,offset,limit:200}));
 await openShotUnits();expect(await screen.findByRole("button",{name:"打开 Shot 资产"})).toBeEnabled();expect(intelligenceApi.listShots).toHaveBeenCalledWith("lib","project-1",200);
});
it("登记身份仍在读取或读取失败时不允许误登记",async()=>{
 let reject!:(reason:Error)=>void;vi.mocked(intelligenceApi.listShots).mockReturnValueOnce(new Promise((_,failure)=>{reject=failure}));
 await openShotUnits();const button=await screen.findByRole("button",{name:"登记为 Shot Asset"});expect(button).toBeDisabled();
 await act(async()=>reject(new Error("读取失败")));expect(button).toBeDisabled();
});

it("项目内可以为既有镜头登记 Shot Asset，不替代旧编辑入口",async()=>{
 const shot={id:"unit-shot",projectId:summary.id,parentId:"scene",kind:"shot",name:"航拍",description:"",startFrame:1,endFrame:100,sortOrder:0} as ProjectUnit;
 vi.mocked(api.listProjects).mockResolvedValue([summary]);vi.mocked(api.getProject).mockResolvedValue({...detail,units:[{...shot,id:"scene",kind:"scene",parentId:null,name:"沙漠"},shot]});
 render(<ProjectHub libraryId="lib" contentLanguage="zh-CN" notify={vi.fn()} onOpenReference={vi.fn()} onEditAsset={vi.fn()}/>);fireEvent.click(await screen.findByRole("button",{name:/打开工作区/}));fireEvent.click(await screen.findByRole("button",{name:"场景 / 镜头"}));const register=await screen.findByRole("button",{name:"登记为 Shot Asset"});await waitFor(()=>expect(register).toBeEnabled());fireEvent.click(register);expect(await screen.findByText("正在登记 unit-shot")).toBeTruthy();expect(screen.getByText("1–100 帧")).toBeTruthy();
});
