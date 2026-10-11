import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { isAssetWorkspace, readContextPage, RequestGate, useWorkspaceState } from "./workspace";
vi.mock("./api",()=>({api:{getWorkspaceContext:vi.fn(),saveWorkspaceContext:vi.fn()}}));
describe("workspace safety and persistence",()=>{
 beforeEach(()=>{vi.clearAllMocks();vi.mocked(api.getWorkspaceContext).mockResolvedValue({libraryId:"lib-A",enabled:true,value:{query:"沙漠",scroll:720}});vi.mocked(api.saveWorkspaceContext).mockResolvedValue(true)});
 it("does not treat media, projects or reference boards as an asset-delete workspace",()=>{
   expect(isAssetWorkspace("library")).toBe(true);expect(isAssetWorkspace("health")).toBe(true);
   for(const view of ["modelLibrary","imageLibrary","projects","reference"] as const)expect(isAssetWorkspace(view)).toBe(false);
 });
 it("restores distant scroll positions by bounded pages and stops superseded requests",async()=>{
   const fetch=vi.fn(async({offset}:{offset:number;limit:number})=>({items:[offset,offset+1],total:8,offset,limit:2}));const result=await readContextPage(fetch,{offset:0,limit:2},6);expect(result.items).toEqual([0,1,2,3,4,5]);expect(fetch).toHaveBeenCalledTimes(3);fetch.mockClear();await readContextPage(fetch,{offset:0,limit:2},6,()=>false);expect(fetch).toHaveBeenCalledTimes(1);
 });
 it("invalidates late responses after a later request or dismissal",()=>{const gate=new RequestGate();const first=gate.begin();expect(gate.isCurrent(first)).toBe(true);gate.begin();expect(gate.isCurrent(first)).toBe(false);const last=gate.begin();gate.invalidate();expect(gate.isCurrent(last)).toBe(false)});
 it("restores only view context, flushes to its original library, and resets on library change",async()=>{
   const hook=renderHook(({revision})=>useWorkspaceState("image",{query:"",scroll:0},revision),{initialProps:{revision:0}});
   await waitFor(()=>expect(hook.result.current[2]).toBe(true));expect(hook.result.current[0]).toEqual({query:"沙漠",scroll:720});
   act(()=>hook.result.current[1]({query:"树林",scroll:920}));
   vi.mocked(api.getWorkspaceContext).mockResolvedValue({libraryId:"lib-B",enabled:true,value:{query:"云",scroll:10}});
   hook.rerender({revision:1});await waitFor(()=>expect(hook.result.current[0].query).toBe("云"));
   expect(api.saveWorkspaceContext).toHaveBeenCalledWith("lib-A","image",{query:"树林",scroll:920});hook.unmount();expect(api.saveWorkspaceContext).toHaveBeenCalledWith("lib-B","image",{query:"云",scroll:10});
 });
 it("does not overwrite typing when a saved context arrives late",async()=>{
   let finish!:(value:Awaited<ReturnType<typeof api.getWorkspaceContext>>)=>void;vi.mocked(api.getWorkspaceContext).mockReturnValue(new Promise(resolve=>{finish=resolve}));
   const hook=renderHook(()=>useWorkspaceState("image",{query:"",scroll:0}));act(()=>hook.result.current[1]({query:"刚输入",scroll:0}));
   await act(async()=>finish({libraryId:"lib-A",enabled:true,value:{query:"旧搜索"}}));expect(hook.result.current[0].query).toBe("刚输入");
 });
});
