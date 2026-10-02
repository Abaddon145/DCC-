import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { MediaAssociationPicker } from "./MediaAssociationPicker";
vi.mock("../lib/api",()=>({api:{listProjects:vi.fn(),listReferenceBoards:vi.fn(),linkProjectMedia:vi.fn(),addMediaImagesToBoard:vi.fn()}}));
describe("media association workflow",()=>{
 beforeEach(()=>{vi.clearAllMocks();vi.mocked(api.listProjects).mockResolvedValue([{id:"project",name:"CG作品"}] as Awaited<ReturnType<typeof api.listProjects>>);vi.mocked(api.listReferenceBoards).mockResolvedValue([{id:"board",name:"参考"}] as Awaited<ReturnType<typeof api.listReferenceBoards>>);vi.mocked(api.linkProjectMedia).mockResolvedValue(1);vi.mocked(api.addMediaImagesToBoard).mockResolvedValue({items:[],skipped:1,warnings:["原图缺失"]})});
 it("chunks every explicit selection including those outside the current filter",async()=>{
   const ids=Array.from({length:5001},(_,index)=>`media-${index}`);const close=vi.fn();render(<MediaAssociationPicker ids={ids} target="project" onClose={close} onAdded={async()=>{}} notify={vi.fn()}/>);
   await screen.findByRole("option",{name:"CG作品"});fireEvent.click(screen.getByRole("button",{name:"确认加入"}));await waitFor(()=>expect(close).toHaveBeenCalled());
   expect(api.linkProjectMedia).toHaveBeenNthCalledWith(1,ids.slice(0,5000),["project"]);expect(api.linkProjectMedia).toHaveBeenNthCalledWith(2,ids.slice(5000),["project"]);
 });
 it("copies images and reports skips instead of moving or deleting originals",async()=>{
   const notify=vi.fn();render(<MediaAssociationPicker ids={["image"]} target="board" onClose={vi.fn()} onAdded={async()=>{}} notify={notify}/>);await screen.findByRole("option",{name:"参考"});fireEvent.click(screen.getByRole("button",{name:"确认加入"}));await waitFor(()=>expect(api.addMediaImagesToBoard).toHaveBeenCalledWith("board",["image"],{x:0,y:0}));await waitFor(()=>expect(notify).toHaveBeenCalledWith(expect.stringContaining("跳过 1 项"),true));
 });
 it.each(["project","board"] as const)("passes the expected library UUID to every %s mutation",async target=>{
   const close=vi.fn();const libraryId="11111111-1111-4111-8111-111111111111";
   render(<MediaAssociationPicker ids={["image"]} target={target} expectedLibraryId={libraryId} onClose={close} onAdded={async()=>{}} notify={vi.fn()}/>);
   await screen.findByRole("option",{name:target==="project"?"CG作品":"参考"});fireEvent.click(screen.getByRole("button",{name:"确认加入"}));
   await waitFor(()=>expect(close).toHaveBeenCalled());
   if(target==="project")expect(api.linkProjectMedia).toHaveBeenCalledWith(["image"],["project"],libraryId);
   else expect(api.addMediaImagesToBoard).toHaveBeenCalledWith("board",["image"],{x:0,y:0},libraryId);
 });
 it("stops a picker whose target module has been disabled",async()=>{
   const props={ids:["image"],target:"project" as const,onClose:vi.fn(),onAdded:async()=>{},notify:vi.fn()};
   const {rerender}=render(<MediaAssociationPicker {...props}/>);await screen.findByRole("option",{name:"CG作品"});
   rerender(<MediaAssociationPicker {...props} canAddProject={false}/>);
   expect(screen.getByRole("button",{name:"确认加入"})).toBeDisabled();
   expect(screen.getByText("此模块已关闭，请先在设置中心启用。")).toBeInTheDocument();
   fireEvent.click(screen.getByRole("button",{name:"确认加入"}));expect(api.linkProjectMedia).not.toHaveBeenCalled();
 });
});
