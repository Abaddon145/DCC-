import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,it,expect,vi} from "vitest";
import {StrictMode} from "react";
import {LearningWorkspace} from "./LearningWorkspace";
import {learningApi} from "./api";
import type {PlaybackSnapshot} from "./types";
import {flushWorkspaceEdits,registerNoteSaveGuard} from "../../lib/noteSaveGuard";
import {NativeLearningSurface} from "./NativeLearningSurface";
import type {CourseAnalysisDetail} from "./types";

const events=vi.hoisted(()=>new Map<string,(event:{payload:any})=>void>());
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn(async(name:string,handler:(event:{payload:any})=>void)=>{events.set(name,handler);return ()=>events.delete(name);})}));
vi.mock("./api",()=>({learningApi:{getCourse:vi.fn(),listCourses:vi.fn(),listSeries:vi.fn(),startPlayback:vi.fn(),getPlayback:vi.fn(),controlPlayback:vi.fn(),setViewport:vi.fn().mockResolvedValue(undefined),stopPlayback:vi.fn(),listSubtitles:vi.fn(),listCourseAnalyses:vi.fn(),getCourseAnalysis:vi.fn()}}));
vi.mock("../../lib/api",()=>({api:{listLibraryTasks:vi.fn().mockResolvedValue([])}}));
const initial:PlaybackSnapshot={sessionId:"session",libraryId:"lib",courseId:"course",ready:true,positionMs:1500,durationMs:60000,paused:true,rate:1,volume:60,muted:false,ended:false,tracks:[{id:1,kind:"audio",label:"中文"},{id:2,kind:"sub",label:"中文字幕"}],audioId:1,subtitleId:null,fullscreen:false,error:null,saveError:null};
const props={libraryId:"lib",courseId:"course",onBack:vi.fn(),notify:vi.fn()};
beforeEach(()=>{
  vi.clearAllMocks();events.clear();
  vi.stubGlobal("ResizeObserver",class{observe(){}disconnect(){}});
  vi.mocked(learningApi.getCourse).mockResolvedValue({id:"course",mediaId:"media",title:"学习视频",positionMs:1500,durationMs:60000,rate:1,completed:false,progressRevision:1});
  vi.mocked(learningApi.startPlayback).mockResolvedValue(initial);
  vi.mocked(learningApi.getPlayback).mockResolvedValue(initial);
  vi.mocked(learningApi.stopPlayback).mockResolvedValue(undefined);
  vi.mocked(learningApi.controlPlayback).mockImplementation(async(_lib,_sid,action)=>({...initial,...(action.type==="fullscreen"?{fullscreen:action.enabled}:action.type==="pause"?{paused:action.paused}:action.type==="volume"?{volume:action.volume}:{})}));
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([]);
  vi.mocked(learningApi.listCourseAnalyses).mockResolvedValue({items:[{id:"analysis",stage:"segments",status:"completed",completedStages:1,totalStages:1,error:"",createdAt:1000}],total:1,offset:0,limit:50});
  const saved:CourseAnalysisDetail={id:"analysis",courseId:"course",previewId:"preview",sourceFingerprint:"a".repeat(64),model:"deepseek-chat",parentAnalysisId:null,status:"completed",error:"",createdAt:1000,updatedAt:1000,staleReasons:[],stages:{items:[],total:0,offset:0,limit:50},selectedStageKey:null,result:{contentType:"tutorial",oneSentenceSummary:"课程要点",overview:"保存的课程说明",chapters:[],keyPoints:[{timeMs:11500,title:"材质参数",content:"学习参数",importance:3,sourceSegmentIds:["cue"]}],tutorialSteps:[],terms:[],actionItems:[],tags:[],warnings:[],sections:[],sectionValidation:[]}};
  vi.mocked(learningApi.getCourseAnalysis).mockResolvedValue(saved);
});
it("opens analysis beside the native viewport and reuses protected playback seeking",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"课程分析"}));
  fireEvent.click(await screen.findByRole("button",{name:/00:11 · 材质参数/}));
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"seek",positionMs:11500}));
  fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));
  expect(screen.queryByRole("region",{name:"课程分析"})).toBeNull();
  expect(await screen.findByRole("region",{name:"课程字幕"})).toBeInTheDocument();
});
it("keeps analysis history available when the native playback tool is missing",async()=>{
  vi.mocked(learningApi.startPlayback).mockRejectedValueOnce(new Error("固定 mpv 工具缺失"));
  render(<LearningWorkspace {...props}/>);await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button",{name:"课程分析"}));
  expect(await screen.findByRole("button",{name:/00:11 · 材质参数/})).toBeDisabled();
});
it("shows missing-tool failure with retry, not an empty player",async()=>{
  vi.mocked(learningApi.startPlayback).mockRejectedValueOnce(new Error("固定 mpv 工具缺失"));
  render(<LearningWorkspace {...props}/>);
  expect(await screen.findByRole("alert")).toHaveTextContent("mpv");
  fireEvent.click(screen.getByRole("button",{name:"重试播放"}));
  await screen.findByRole("button",{name:"播放"});
});
it("rejects old library/session events and supports F11 before clicking controls",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  await act(async()=>events.get("learning-playback-state")?.({payload:{...initial,libraryId:"old",paused:false}}));
  expect(screen.getByRole("button",{name:"播放"})).toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("region",{name:"学习播放器"}),{key:"F11"});
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"fullscreen",enabled:true}));
});
it("save failure keeps the workspace and the original navigation target",async()=>{
  const unregister=registerNoteSaveGuard(async()=>{throw new Error("笔记保存失败");});
  render(<LearningWorkspace {...props} projectId="project"/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"返回项目"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("笔记保存失败");
  expect(props.onBack).not.toHaveBeenCalled();expect(learningApi.stopPlayback).not.toHaveBeenCalled();unregister();
  vi.mocked(learningApi.stopPlayback).mockRejectedValueOnce(new Error("进度保存失败"));
  await expect(flushWorkspaceEdits()).rejects.toThrow("进度保存失败");
  expect(props.onBack).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"返回项目"}));
  await waitFor(()=>expect(props.onBack).toHaveBeenCalledTimes(1));
});
it("hides the native viewport for overlays and clears bindings in inputs and IME",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  const input=screen.getByLabelText("精确跳转（秒）");fireEvent.focus(input);
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"shortcuts",bindings:[]}));
  fireEvent.keyDown(input,{key:"F11"});
  expect(learningApi.controlPlayback).not.toHaveBeenCalledWith("lib","session",{type:"fullscreen",enabled:true});
  const dialog=document.createElement("div");dialog.setAttribute("role","dialog");dialog.setAttribute("aria-modal","true");
  await act(async()=>{document.body.append(dialog);});
  await waitFor(()=>expect(learningApi.setViewport).toHaveBeenCalledWith("lib","session",expect.objectContaining({visible:false})));
  dialog.remove();
});
it("native double click cancels a pending single click, wheel changes volume by five",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  await act(async()=>{events.get("learning-playback-pointer")?.({payload:{...initial,kind:"click"}});events.get("learning-playback-pointer")?.({payload:{...initial,kind:"doubleClick"}});});
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"fullscreen",enabled:true}));
  await new Promise(resolve=>setTimeout(resolve,400));
  expect(learningApi.controlPlayback).not.toHaveBeenCalledWith("lib","session",{type:"pause",paused:false});
  await act(async()=>events.get("learning-playback-pointer")?.({payload:{...initial,kind:"wheelUp"}}));
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"volume",volume:65}));
});
it("serializes old viewport cleanup before showing a ready session",async()=>{
  let finish!:()=>void;
  vi.spyOn(HTMLElement.prototype,"getBoundingClientRect").mockReturnValue({left:10,top:10,width:400,height:200,right:410,bottom:210,x:10,y:10,toJSON:()=>({})});
  vi.mocked(learningApi.setViewport).mockImplementationOnce(()=>new Promise<void>(resolve=>{finish=resolve;}));
  const props={snapshot:{...initial,ready:false},hidden:false,onBlockedChange:vi.fn(),onPointer:vi.fn(),onError:vi.fn()};
  const {rerender}=render(<NativeLearningSurface {...props}/>);
  await waitFor(()=>expect(finish).toBeDefined());
  rerender(<NativeLearningSurface {...props} snapshot={initial}/>);
  await act(async()=>{finish();});
  await waitFor(()=>expect(vi.mocked(learningApi.setViewport).mock.calls.at(-1)?.[2].visible).toBe(true));
});
it("StrictMode reuses pending startup instead of spawning or stopping another session",async()=>{
  render(<StrictMode><LearningWorkspace {...props}/></StrictMode>);await screen.findByRole("button",{name:"播放"});
  expect(learningApi.startPlayback).toHaveBeenCalledTimes(1);expect(learningApi.stopPlayback).not.toHaveBeenCalled();
});
it("shortcut help is localized, Escape dismisses it and returns keyboard focus",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"快捷键"}));
  const dialog=await screen.findByRole("dialog",{name:"播放器快捷键"});
  expect(dialog).toHaveTextContent("播放 / 暂停");
  fireEvent.keyDown(dialog,{key:"Escape"});
  expect(screen.queryByRole("dialog",{name:"播放器快捷键"})).toBeNull();
  expect(screen.getByRole("button",{name:"快捷键"})).toHaveFocus();
});
it("rapid native wheel input applies five points for each event, not a stale volume",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  await act(async()=>{events.get("learning-playback-pointer")?.({payload:{...initial,kind:"wheelUp"}});events.get("learning-playback-pointer")?.({payload:{...initial,kind:"wheelUp"}});});
  await waitFor(()=>expect(learningApi.controlPlayback).toHaveBeenCalledWith("lib","session",{type:"volume",volume:70}));
});
it("confirms startup state so an early native error cannot be overwritten by the pending reply",async()=>{
  let finish!:(value:PlaybackSnapshot)=>void;
  vi.mocked(learningApi.startPlayback).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  vi.mocked(learningApi.getPlayback).mockResolvedValue({...initial,ready:false,error:"播放内核首帧失败"});
  render(<LearningWorkspace {...props}/>);
  await waitFor(()=>expect(finish).toBeDefined());
  await act(async()=>{events.get("learning-playback-state")?.({payload:{...initial,ready:false,error:"播放内核首帧失败"}});finish({...initial,ready:false});});
  expect(await screen.findByRole("alert")).toHaveTextContent("首帧失败");
});
it("ignores a delayed next-course lookup after the selected course changes",async()=>{
  let finish!:(value:Awaited<ReturnType<typeof learningApi.listCourses>>)=>void;
  vi.mocked(learningApi.listCourses).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const {rerender}=render(<LearningWorkspace {...props}/>);
  await screen.findByRole("button",{name:"播放"});
  fireEvent.keyDown(screen.getByRole("region",{name:"学习播放器"}),{key:"n"});
  await waitFor(()=>expect(finish).toBeDefined());
  rerender(<LearningWorkspace {...props} courseId="selected-other"/>);
  await waitFor(()=>expect(learningApi.startPlayback).toHaveBeenCalledWith("lib","selected-other"));
  const course=await learningApi.getCourse("lib","course");
  await act(async()=>finish({items:[course,{...course,id:"obsolete-next"}],total:2,offset:0,limit:80}));
  expect(learningApi.stopPlayback).not.toHaveBeenCalled();
  expect(learningApi.startPlayback).not.toHaveBeenCalledWith("lib","obsolete-next");
});
it("next-course shortcut follows the selected series rather than library update order",async()=>{
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"series",name:"UE制作",description:"",revision:0,orderedCourseIds:["course","series-next"]}]);
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"课程系列"}));
  await screen.findByRole("option",{name:"UE制作"});
  await waitFor(()=>expect(screen.getByLabelText("当前系列")).toHaveValue("series"));
  fireEvent.keyDown(screen.getByRole("region",{name:"学习播放器"}),{key:"n"});
  await waitFor(()=>expect(learningApi.startPlayback).toHaveBeenCalledWith("lib","series-next"));
  expect(learningApi.listCourses).not.toHaveBeenCalled();
});
it("only confirms a completed mark after backend success and preserves it on save failure",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"标记课程完成"}));
  const completed=await screen.findByRole("button",{name:"已完成 · 恢复学习"});
  expect(completed).toHaveAttribute("aria-pressed","true");
  vi.mocked(learningApi.controlPlayback).mockRejectedValueOnce(new Error("进度写入失败"));
  fireEvent.click(completed);
  expect(await screen.findByRole("alert")).toHaveTextContent("进度写入失败");
  expect(screen.getByRole("button",{name:"已完成 · 恢复学习"})).toHaveAttribute("aria-pressed","true");
});
