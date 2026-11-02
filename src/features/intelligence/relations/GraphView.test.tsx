import {render,screen,fireEvent,cleanup,waitFor,within} from "@testing-library/react";
import {afterEach,beforeEach,expect,it,vi} from "vitest";
import {GraphView} from "./GraphView";
import {intelligenceApi} from "../api";
import {coreApi} from "../../assets/api";
import type {Asset} from "../../assets/types";
import type {AssetGraph} from "../types";
vi.mock("../api",()=>({intelligenceApi:{graph:vi.fn(),createRelation:vi.fn(),deleteRelation:vi.fn()}}));
vi.mock("../../assets/api",()=>({coreApi:{search:vi.fn()}}));
const center:Asset={id:"center",name:"生成结果",assetType:"image",description:"",thumbnail:null,tags:[],origin:{kind:"media",id:"source"},deleted:false,createdAt:"now",updatedAt:"now"};
const input:Asset={...center,id:"input",name:"参考图"};
const edge={id:"edge",sourceAssetId:"center",targetAssetId:"input",relationType:"generated_from" as const,createdAt:"now"};
const graph:AssetGraph={center,nodes:[center,input],relations:[edge],total:81,offset:0,limit:80};
const props={libraryId:"lib",assetId:"center",locale:"zh-CN",onOpenAsset:vi.fn(),onChanged:vi.fn()};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(intelligenceApi.graph).mockResolvedValue(graph);vi.mocked(coreApi.search).mockResolvedValue({items:[center,input],total:2,offset:0,limit:40});});
afterEach(cleanup);

it("图和列表保留生成结果→输入方向，可用键盘打开关联资产",async()=>{
 render(<GraphView {...props}/>);
 const list=await screen.findByRole("list",{name:"资产关系列表"});
 expect(within(list).getByText("生成自 →")).toBeTruthy();
 expect(within(list).getByText("生成结果")).toBeTruthy();
 const svg=screen.getByRole("group",{name:"局部资产关系图"});
 const line=svg.querySelector("path[data-relation-id='edge']")!;
 expect(line.getAttribute("data-source")).toBe("center");
 expect(line.getAttribute("data-target")).toBe("input");
 const node=within(svg).getByRole("button",{name:"在关系图打开 参考图"});
 node.focus();fireEvent.keyDown(node,{key:"Enter"});expect(props.onOpenAsset).toHaveBeenCalledWith("input");
 fireEvent.click(screen.getByRole("button",{name:"关系下一页"}));
 await waitFor(()=>expect(intelligenceApi.graph).toHaveBeenLastCalledWith("lib","center","zh-CN",80));
});

it("搜索候选、添加方向和删除只作用于当前库，失败保留关系",async()=>{
 render(<GraphView {...props}/>);await screen.findByRole("list",{name:"资产关系列表"});
 fireEvent.change(screen.getByLabelText("搜索关联资产"),{target:{value:"参考"}});
 fireEvent.click(await screen.findByRole("button",{name:"选择关联资产 参考图"}));
 expect(screen.queryByRole("button",{name:"选择关联资产 生成结果"})).toBeNull();
 fireEvent.change(screen.getByLabelText("关系类型"),{target:{value:"generated_from"}});
 fireEvent.click(screen.getByRole("button",{name:"添加关系"}));
 await waitFor(()=>expect(intelligenceApi.createRelation).toHaveBeenCalledWith("lib",{sourceAssetId:"center",targetAssetId:"input",relationType:"generated_from"}));
 await waitFor(()=>expect(screen.getByRole("button",{name:"移除关系 生成自 参考图"}).hasAttribute("disabled")).toBe(false));
 vi.mocked(intelligenceApi.deleteRelation).mockRejectedValueOnce(new Error("不能删除"));
 fireEvent.click(screen.getByRole("button",{name:"移除关系 生成自 参考图"}));
 await screen.findByText("Error: 不能删除");
 expect(screen.getByRole("button",{name:"打开关联资产 参考图"})).toBeTruthy();
 expect(intelligenceApi.deleteRelation).toHaveBeenCalledWith("lib","center","edge");
});

it("读取错误可以重试，回收站节点不可编辑但关系仍可见",async()=>{
 vi.mocked(intelligenceApi.graph).mockRejectedValueOnce(new Error("读取中断"));
 render(<GraphView {...props}/>);
 fireEvent.click(await screen.findByRole("button",{name:"重新读取"}));
 await screen.findByRole("button",{name:"打开关联资产 参考图"});
 vi.mocked(intelligenceApi.graph).mockResolvedValue({...graph,nodes:[center,{...input,deleted:true}]});
 fireEvent.click(screen.getByRole("button",{name:"刷新关系"}));
 await screen.findByText("暂不可用 · 已在回收站");
 expect(screen.getByRole("button",{name:"打开关联资产 参考图"}).hasAttribute("disabled")).toBe(true);
});

it("切库后拒绝迟到读取和写入反馈",async()=>{
 let resolveGraph:(value:AssetGraph)=>void=()=>{};
 vi.mocked(intelligenceApi.graph).mockReturnValueOnce(new Promise(r=>{resolveGraph=r;}));
 const {rerender}=render(<GraphView {...props}/>);
 vi.mocked(intelligenceApi.graph).mockResolvedValue({...graph,center:{...center,name:"新库中心"},nodes:[{...center,name:"新库中心"}],relations:[],total:0});
 rerender(<GraphView {...props} libraryId="second"/>);
 await screen.findByText("暂无资产关系");
 resolveGraph(graph);
 await waitFor(()=>expect(screen.queryByRole("button",{name:"打开关联资产 参考图"})).toBeNull());
 expect(props.onChanged).not.toHaveBeenCalled();
});
