import {render,screen,fireEvent,cleanup,waitFor} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {AssetBrowser,AssetScope} from "./AssetBrowser";
import {coreApi} from "./api";
import {api} from "../../lib/api";
import {open} from "@tauri-apps/plugin-dialog";
import type {Asset,CoreAssetDetail} from "./types";
import {intelligenceApi} from "../intelligence/api";
vi.mock("../intelligence/api",()=>({intelligenceApi:{graph:vi.fn().mockResolvedValue({center:{id:"one",name:"森林图",deleted:false},nodes:[],relations:[],total:0,offset:0,limit:80}),createRelation:vi.fn(),deleteRelation:vi.fn()}}));
vi.mock("./api",()=>({coreApi:{search:vi.fn(),get:vi.fn(),tags:vi.fn(),color:vi.fn(),project:vi.fn(),attach:vi.fn(),createVersion:vi.fn(),deleteVersion:vi.fn(),addSource:vi.fn(),removeSource:vi.fn(),scanPage:vi.fn()}}));
vi.mock("../../lib/api",()=>({api:{listProjects:vi.fn().mockResolvedValue([]),startLibraryTask:vi.fn(),getLibraryTask:vi.fn(),cancelLibraryTask:vi.fn(),imageData:vi.fn()},mediaLibraryFileUrl:(id:string)=>`/media/${id}`,assetMediaUrl:(id:string)=>`/asset/${id}`,mediaLibraryBundleUrl:(id:string)=>`/model/${id}`}));
vi.mock("../../components/ModelViewer",()=>({ModelViewer:()=> <div>模型视口</div>}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn().mockResolvedValue(null)}));
const image:Asset={id:"one",name:"森林图",assetType:"image",description:"",thumbnail:null,tags:["森林"],origin:{kind:"media",id:"old-one"},deleted:false,createdAt:"now",updatedAt:"now"};
const video:Asset={...image,id:"two",name:"演示视频",assetType:"video",origin:{kind:"media",id:"old-two"}};
const detail:CoreAssetDetail={asset:image,files:[],metadata:[],sources:[],versions:[],projects:[],knowledgeLinks:[{id:"note",name:"学习笔记"}],relatedAssets:[],tagDetails:[{id:"tag",name:"森林",color:"",createdAt:"now"}],processingHistory:[]};
const props={libraryId:"lib",locale:"zh-CN",notify:vi.fn(),onOpenOrigin:vi.fn(),onOpenNote:vi.fn()};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(coreApi.search).mockResolvedValue({items:[image,video],total:82,offset:0,limit:80});vi.mocked(coreApi.get).mockResolvedValue(detail);});afterEach(cleanup);
it("关系按需打开，不影响原详情与外部关闭",async()=>{
 render(<AssetBrowser {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"打开 森林图"}));
 await screen.findByRole("button",{name:"打开原详情"});expect(intelligenceApi.graph).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole("button",{name:"查看资产关系"}));
 await screen.findByText("暂无资产关系");expect(intelligenceApi.graph).toHaveBeenCalledWith("lib","one","zh-CN",0);
 fireEvent.click(screen.getByRole("button",{name:"关闭详情背景"}));expect(screen.queryByRole("dialog")).toBeNull();
});
it("默认范围保留普通素材，全部资产是独立范围",()=>{const change=vi.fn();const {rerender}=render(<AssetScope scope="normal" onChange={change} browser={<p>统一浏览</p>}><p>Fab 原入口</p></AssetScope>);expect(screen.getByText("Fab 原入口")).toBeTruthy();expect(screen.queryByText("统一浏览")).toBeNull();fireEvent.click(screen.getByRole("button",{name:"全部资产"}));expect(change).toHaveBeenCalledWith("all");rerender(<AssetScope scope="all" onChange={change} browser={<p>统一浏览</p>}><p>Fab 原入口</p></AssetScope>);expect(screen.queryByText("Fab 原入口")).toBeNull();});
it("混合浏览、分页和旧详情跳转不出现 Fab 批量按钮",async()=>{render(<AssetBrowser {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"打开 森林图"}));await screen.findByRole("dialog",{name:"资产详情"});fireEvent.click(screen.getByRole("button",{name:"打开原详情"}));expect(props.onOpenOrigin).toHaveBeenCalledWith(image.origin,"image");fireEvent.click(screen.getByRole("button",{name:"关闭资产详情"}));fireEvent.click(screen.getByRole("button",{name:"下一页"}));await waitFor(()=>expect(coreApi.search).toHaveBeenLastCalledWith("lib",expect.objectContaining({offset:80})));expect(screen.queryByText("补齐双语")).toBeNull();});
it("搜索和类型筛选发送分页查询，失败可重试",async()=>{vi.mocked(coreApi.search).mockRejectedValueOnce(new Error("读取失败"));render(<AssetBrowser {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"重新读取"}));await screen.findByText("演示视频");fireEvent.change(screen.getByLabelText("搜索全部资产"),{target:{value:"森林"}});fireEvent.change(screen.getByLabelText("资产类型"),{target:{value:"image"}});await waitFor(()=>expect(coreApi.search).toHaveBeenLastCalledWith("lib",expect.objectContaining({query:"森林",assetType:"image",offset:0})));});

it("标签移除、来源与版本创建走类型化接口，失败不伪装成功",async()=>{
 const file={id:"file",assetId:image.id,sourceRef:{kind:"mediaFile" as const,id:"original",role:"original"},role:"original",filename:"森林.png",extension:"png",size:20,hash:"sha",createdAt:"now",available:true};
 vi.mocked(coreApi.get).mockResolvedValue({...detail,files:[file]});
 render(<AssetBrowser {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"打开 森林图"}));
 fireEvent.click(await screen.findByRole("button",{name:"移除标签 森林"}));await waitFor(()=>expect(coreApi.tags).toHaveBeenCalledWith("lib","one","zh-CN",[]));
 await waitFor(()=>expect(screen.getByRole("dialog",{name:"资产详情"}).getAttribute("aria-busy")).toBe("false"));
 fireEvent.change(screen.getByLabelText("来源链接"),{target:{value:"https://example.com/参考"}});fireEvent.change(screen.getByLabelText("来源作者"),{target:{value:"作者"}});fireEvent.click(screen.getByRole("button",{name:"保存来源"}));
 await waitFor(()=>expect(coreApi.addSource).toHaveBeenCalledWith("lib",{assetId:"one",sourceType:"web",url:"https://example.com/参考",author:"作者"}));
 await waitFor(()=>expect(screen.getByRole("dialog",{name:"资产详情"}).getAttribute("aria-busy")).toBe("false"));
 vi.mocked(coreApi.createVersion).mockRejectedValueOnce(new Error("版本标签已存在"));fireEvent.click(screen.getByRole("button",{name:"创建版本"}));await screen.findByText("Error: 版本标签已存在");expect(coreApi.createVersion).toHaveBeenCalledWith("lib",{assetId:"one",fileId:"file",version:"v001",note:""});expect(screen.queryByRole("button",{name:"预览 v001"})).toBeNull();
});

it("扫描先预览再确认，重复项只有显式选择项目才能引用",async()=>{
 const task={id:"scan",libraryId:"lib",label:"扫描",status:"completed" as const,phase:"scanning",current:2,total:2,currentName:"",createdAt:"now",updatedAt:"now",cancellable:false,message:"",payload:{kind:"assetScan" as const,request:{root:"C:/源"}},result:{total:2,importable:1,duplicate:1,unsupported:0,failed:0,cancelled:false}};
 vi.mocked(open).mockResolvedValueOnce("C:/源");vi.mocked(api.startLibraryTask).mockResolvedValue(task);vi.mocked(api.getLibraryTask).mockResolvedValue(task);
 vi.mocked(coreApi.scanPage).mockResolvedValue({total:2,offset:0,limit:100,items:[{id:"new",path:"C:/源/new.png",relativePath:"new.png",kind:"image",size:20,hash:"new",status:"ready",message:"",duplicateAssetId:null},{id:"dup",path:"C:/源/old.png",relativePath:"old.png",kind:"image",size:20,hash:"old",status:"duplicate",message:"已有文件",duplicateAssetId:"one"}]});
 render(<AssetBrowser {...props}/>);fireEvent.click(screen.getByRole("button",{name:"扫描 / 导入文件"}));fireEvent.click(screen.getByRole("button",{name:"选择目录扫描"}));await screen.findByText("可导入 1 · 重复 1 · 不支持 0 · 失败 0");await screen.findByRole("button",{name:"打开现有资产"});
 expect(api.startLibraryTask).toHaveBeenCalledTimes(1);expect(coreApi.project).not.toHaveBeenCalled();expect(screen.getAllByRole("checkbox")[1].hasAttribute("disabled")).toBe(true);
 fireEvent.click(screen.getByRole("button",{name:"选择当前页可导入项"}));fireEvent.click(screen.getByRole("button",{name:"确认导入 1 项"}));await waitFor(()=>expect(api.startLibraryTask).toHaveBeenLastCalledWith({kind:"assetImport",request:{paths:[],scanTaskId:"scan",scanRowIds:["new"],tags:[],projectId:null}},"lib"));
});
it("切库和快速切换详情丢弃过期响应，Esc 和外部点击关闭",async()=>{let resolveOld:(value:CoreAssetDetail)=>void=()=>{};vi.mocked(coreApi.get).mockReturnValueOnce(new Promise(resolve=>{resolveOld=resolve;}));const {rerender}=render(<AssetBrowser {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"打开 森林图"}));fireEvent.click(screen.getByRole("button",{name:"打开 演示视频"}));await screen.findByRole("dialog",{name:"资产详情"});resolveOld({...detail,asset:{...image,name:"过期详情"}});await waitFor(()=>expect(screen.queryByText("过期详情")).toBeNull());fireEvent.keyDown(window,{key:"Escape"});expect(screen.queryByRole("dialog")).toBeNull();fireEvent.click(screen.getByRole("button",{name:"打开 森林图"}));await screen.findByRole("dialog");fireEvent.click(screen.getByRole("button",{name:"关闭详情背景"}));expect(screen.queryByRole("dialog")).toBeNull();let resolveSearch:(value:{items:Asset[];total:number;offset:number;limit:number})=>void=()=>{};vi.mocked(coreApi.search).mockReturnValueOnce(new Promise(r=>{resolveSearch=r;}));rerender(<AssetBrowser {...props} libraryId="second"/>);rerender(<AssetBrowser {...props} libraryId="third"/>);resolveSearch({items:[{...image,name:"错库素材"}],total:1,offset:0,limit:80});await waitFor(()=>expect(screen.queryByText("错库素材")).toBeNull());});
