import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {ReferenceBoardWorkspace} from "./ReferenceBoardWorkspace";
import {flushWorkspaceEdits} from "../lib/noteSaveGuard";
import {api} from "../lib/api";
import type {ReferenceBoardDetail,ReferenceBoardItem} from "../types";
const fitSelection=vi.hoisted(()=>vi.fn());
vi.mock("../lib/api",()=>({api:{saveReferenceBoardChanges:vi.fn()}}));
vi.mock("./ReferenceCanvas",async()=>{const {forwardRef,useImperativeHandle}=await import("react");return {ReferenceCanvas:forwardRef((_props:{items:ReferenceBoardItem[];selectedIds:Set<string>;onSelectionChange:(ids:Set<string>)=>void;onItemsCommit:(before:ReferenceBoardItem[],after:ReferenceBoardItem[])=>void},ref)=>{useImperativeHandle(ref,()=>({fitSelection,fitAll:vi.fn(),resetView:vi.fn(),centerPoint:()=>({x:0,y:0}),pointAt:()=>({x:0,y:0})}));return <div><output data-testid="selected-item">{[..._props.selectedIds].join(",")}</output><button onClick={()=>_props.onSelectionChange(new Set(["item"]))}>选择图片</button><button onClick={()=>_props.onItemsCommit(_props.items,_props.items.map(item=>({...item,x:item.x+10})))}>移动图片</button></div>;})};});
const item:ReferenceBoardItem={id:"item",boardId:"board",originalName:"截图",pixelWidth:100,pixelHeight:100,x:0,y:0,width:100,height:100,rotation:0,zIndex:0,sourceAssetId:null,sourceImageId:null};
const board:ReferenceBoardDetail={id:"board",name:"参考板",background:"#112233",viewX:0,viewY:0,viewScale:1,createdAt:"",updatedAt:"",items:[item]};
const props={initialBoard:board,onLearnSource:vi.fn(),notify:vi.fn()};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(api.saveReferenceBoardChanges).mockResolvedValue();});
it("saves pending changes before requesting the selected image source",async()=>{
  let finish!:()=>void;vi.mocked(api.saveReferenceBoardChanges).mockReturnValueOnce(new Promise<void>(resolve=>{finish=resolve;}));
  render(<ReferenceBoardWorkspace {...props}/>);fireEvent.click(screen.getByRole("button",{name:"选择图片"}));fireEvent.click(screen.getByRole("button",{name:"移动图片"}));fireEvent.click(screen.getByRole("button",{name:"查看图片的视频来源"}));
  await waitFor(()=>expect(finish).toBeDefined());expect(props.onLearnSource).not.toHaveBeenCalled();await act(async()=>finish());await waitFor(()=>expect(props.onLearnSource).toHaveBeenCalledWith("board","item"));
  expect(api.saveReferenceBoardChanges).toHaveBeenCalledWith(expect.objectContaining({boardId:"board",items:[expect.objectContaining({id:"item",x:10})]}));
});
it("does not leave or call the source when saving fails",async()=>{
  vi.mocked(api.saveReferenceBoardChanges).mockRejectedValueOnce(new Error("磁盘不可写"));render(<ReferenceBoardWorkspace {...props}/>);fireEvent.click(screen.getByRole("button",{name:"选择图片"}));fireEvent.click(screen.getByRole("button",{name:"移动图片"}));fireEvent.click(screen.getByRole("button",{name:"查看图片的视频来源"}));
  await waitFor(()=>expect(props.notify).toHaveBeenCalledWith(expect.stringContaining("磁盘不可写"),true));expect(props.onLearnSource).not.toHaveBeenCalled();
});
it("saves board edits when the app navigates elsewhere",async()=>{
  render(<ReferenceBoardWorkspace {...props}/>);fireEvent.click(screen.getByRole("button",{name:"移动图片"}));await act(async()=>flushWorkspaceEdits());expect(api.saveReferenceBoardChanges).toHaveBeenCalledWith(expect.objectContaining({boardId:"board"}));
});
it("selects and fits the exact item when returning to its board",async()=>{
  render(<ReferenceBoardWorkspace {...props} initialItemId="item"/>);expect(await screen.findByTestId("selected-item")).toHaveTextContent("item");await waitFor(()=>expect(fitSelection).toHaveBeenCalled());
});
it("saves edits made while the first save is still running before opening the source",async()=>{
  let first!:()=>void,second!:()=>void;
  vi.mocked(api.saveReferenceBoardChanges).mockReturnValueOnce(new Promise<void>(resolve=>{first=resolve;})).mockReturnValueOnce(new Promise<void>(resolve=>{second=resolve;}));
  render(<ReferenceBoardWorkspace {...props}/>);fireEvent.click(screen.getByRole("button",{name:"选择图片"}));fireEvent.click(screen.getByRole("button",{name:"移动图片"}));fireEvent.click(screen.getByRole("button",{name:"查看图片的视频来源"}));
  await waitFor(()=>expect(first).toBeDefined());fireEvent.click(screen.getByRole("button",{name:"移动图片"}));await act(async()=>first());await waitFor(()=>expect(second).toBeDefined());
  expect(props.onLearnSource).not.toHaveBeenCalled();await act(async()=>second());await waitFor(()=>expect(props.onLearnSource).toHaveBeenCalledTimes(1));
  expect(vi.mocked(api.saveReferenceBoardChanges).mock.calls[1][0].items[0].x).toBe(20);
});
it("reports a missing source without opening another video and suppresses duplicate clicks",async()=>{
  let fail!:(error:Error)=>void;props.onLearnSource.mockReturnValueOnce(new Promise<void>((_resolve,reject)=>{fail=reject;}));
  render(<ReferenceBoardWorkspace {...props}/>);fireEvent.click(screen.getByRole("button",{name:"选择图片"}));const button=screen.getByRole("button",{name:"查看图片的视频来源"});fireEvent.click(button);fireEvent.click(button);
  await waitFor(()=>expect(fail).toBeDefined());expect(props.onLearnSource).toHaveBeenCalledTimes(1);await act(async()=>fail(new Error("该图片没有关联视频学习来源")));
  await waitFor(()=>expect(props.notify).toHaveBeenCalledWith(expect.stringContaining("没有关联视频学习来源"),true));
});
