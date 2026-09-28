import { StrictMode, useEffect } from "react";
import { render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { ModelThumbnailTask } from "./ModelThumbnailTask";

vi.mock("../lib/api",()=>({mediaLibraryBundleUrl:(id:string,path:string)=>`${id}/${path}`,api:{getModelThumbnailTask:vi.fn(),finishModelThumbnailTask:vi.fn()}}));
vi.mock("./ModelViewer",()=>({ModelViewer:({onReady}:{onReady:(capture:()=>Promise<Uint8Array>)=>void})=>{useEffect(()=>{onReady(async()=>new Uint8Array([1,2,3]))},[]);return <div/>}}));
describe("background model thumbnail",()=>{
  beforeEach(()=>{vi.clearAllMocks();vi.mocked(api.getModelThumbnailTask).mockResolvedValue({id:"model",logicalPath:"model.glb",originalName:"model.glb"});vi.mocked(api.finishModelThumbnailTask).mockResolvedValue(undefined)});
  it("captures once in StrictMode without needing the media library page",async()=>{
    render(<StrictMode><ModelThumbnailTask id="task"/></StrictMode>);
    await waitFor(()=>expect(api.finishModelThumbnailTask).toHaveBeenCalledWith("task",[1,2,3],null));
    expect(api.finishModelThumbnailTask).toHaveBeenCalledTimes(1);
  });
  it("reports unavailable original as a failed task",async()=>{
    vi.mocked(api.getModelThumbnailTask).mockRejectedValue(new Error("原件缺失"));render(<ModelThumbnailTask id="task"/>);
    await waitFor(()=>expect(api.finishModelThumbnailTask).toHaveBeenCalledWith("task",null,"Error: 原件缺失"));
  });
});
