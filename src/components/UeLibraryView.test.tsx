import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { UeAsset } from "../types";
import { UeLibraryView } from "./UeLibraryView";

vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn()}));
vi.mock("../lib/api",()=>({ueThumbnailUrl:(id:string)=>`http://dcc-media.localhost/ue/${id}`,api:{
  listUeEngines:vi.fn(),listUeSources:vi.fn(),listUeFolders:vi.fn(),searchUeAssets:vi.fn(),
  getWorkspaceContext:vi.fn(),saveWorkspaceContext:vi.fn(),runUeJob:vi.fn(),getUeAsset:vi.fn(),getUeLinks:vi.fn(),
  listUeCaptures:vi.fn(),selectUeAssetIds:vi.fn(),
}}));
const asset:UeAsset={id:"mesh",sourceId:"source",packageName:"/Game/Mesh",objectPath:"/Game/Mesh.Mesh",name:"Mesh",alias:"岩石",className:"StaticMesh",relativePath:"Mesh.uasset",status:"ready",message:"",favorite:false,folderId:null,thumbnail:"native.png",dependencies:[],tags:[]};
const props={notify:vi.fn(),paneWidth:248,paneCollapsed:false,onPaneWidthChange:vi.fn(),onPaneCollapsedChange:vi.fn()};
beforeEach(()=>{
  vi.clearAllMocks();
  vi.mocked(api.listUeEngines).mockResolvedValue([{id:"engine",root:"UE_5.6",version:"5.6.1",buildId:"build",toolReady:true}]);
  vi.mocked(api.listUeSources).mockResolvedValue([{id:"source",name:"工程",kind:"project",originalPath:"project.uproject",contentRoot:"Content",engineVersion:"5.6",storageMode:"reference",lastIndexed:null,assetCount:1,warnings:[]}]);
  vi.mocked(api.listUeFolders).mockResolvedValue([]);
  vi.mocked(api.searchUeAssets).mockResolvedValue({items:[asset],total:1,offset:0,limit:80});
  vi.mocked(api.getWorkspaceContext).mockResolvedValue({libraryId:"library",enabled:true,value:null});
  vi.mocked(api.saveWorkspaceContext).mockResolvedValue(true);
  vi.mocked(api.listUeCaptures).mockResolvedValue([]);
  vi.mocked(api.selectUeAssetIds).mockResolvedValue([asset.id]);
  vi.mocked(api.getUeAsset).mockResolvedValue(asset);vi.mocked(api.getUeLinks).mockResolvedValue([]);
});
it("reads metadata and native thumbnails without launching UE on entry",async()=>{
  render(<UeLibraryView {...props}/>);await screen.findByRole("button",{name:"查看 岩石"});
  expect(screen.getByAltText("岩石 · UE 原生缩略图")).toHaveAttribute("src","http://dcc-media.localhost/ue/mesh");
  expect(api.runUeJob).not.toHaveBeenCalled();
});
it("requires confirmation before an engine job",async()=>{
  vi.spyOn(window,"confirm").mockReturnValue(false);
  render(<UeLibraryView {...props}/>);await screen.findByRole("button",{name:"查看 岩石"});
  fireEvent.change(screen.getByLabelText("UE 来源"),{target:{value:"source"}});
  fireEvent.click(screen.getByRole("button",{name:"读取原生索引"}));
  expect(window.confirm).toHaveBeenCalled();expect(api.runUeJob).not.toHaveBeenCalled();
});
it("opens details and closes them outside",async()=>{
  render(<UeLibraryView {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"查看 岩石"}));
  await screen.findByRole("complementary",{name:"UE 资产详情"});
  fireEvent.pointerDown(screen.getByRole("heading",{name:"UE 资产库"}));
  await waitFor(()=>expect(screen.queryByRole("complementary",{name:"UE 资产详情"})).not.toBeInTheDocument());
});
it("selects every matching ID, not just mounted cards",async()=>{
  vi.mocked(api.selectUeAssetIds).mockResolvedValue(Array.from({length:150},(_,i)=>`asset-${i}`));
  render(<UeLibraryView {...props}/>);await screen.findByRole("button",{name:"查看 岩石"});
  fireEvent.click(screen.getByRole("button",{name:"多选"}));fireEvent.click(screen.getByRole("button",{name:"全选结果"}));
  await screen.findByText(/已选择 150 项/);expect(api.selectUeAssetIds).toHaveBeenCalledWith("library",expect.objectContaining({query:"",sourceId:null}));
  expect(api.runUeJob).not.toHaveBeenCalled();
});
it("does not offer native preview for unparsed packages",async()=>{
  vi.mocked(api.getUeAsset).mockResolvedValue({...asset,status:"unresolved",className:"Unknown",message:"缺失依赖"});
  render(<UeLibraryView {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"查看 岩石"}));
  await screen.findByText("缺失依赖");expect(screen.getByRole("button",{name:"在 UE 中原生预览"})).toBeDisabled();
});
it("keeps navigation counts stable when results are filtered",async()=>{
  vi.mocked(api.searchUeAssets).mockResolvedValue({items:[],total:0,offset:0,limit:80});
  render(<UeLibraryView {...props}/>);
  await screen.findByRole("button",{name:/全部 UE 资产\s*1/});
  expect(screen.getByRole("button",{name:/全部 UE 资产\s*1/})).toBeInTheDocument();
});
it("uses keyboard selection without opening details in multi-select mode",async()=>{
  render(<UeLibraryView {...props}/>);const card=await screen.findByRole("button",{name:"查看 岩石"});
  fireEvent.click(screen.getByRole("button",{name:"多选"}));fireEvent.keyDown(card,{key:"Enter"});
  await screen.findByText(/已选择 1 项/);
  expect(api.getUeAsset).not.toHaveBeenCalled();
});
