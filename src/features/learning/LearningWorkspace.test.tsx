import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,it,expect,vi} from "vitest";
import {StrictMode,useState} from "react";
import {LearningWorkspace} from "./LearningWorkspace";
import {learningApi} from "./api";
import {api} from "../../lib/api";
import type {PlaybackSnapshot} from "./types";
import {flushWorkspaceEdits,registerNoteSaveGuard} from "../../lib/noteSaveGuard";
import {NativeLearningSurface} from "./NativeLearningSurface";
import type {CourseAnalysisDetail} from "./types";
import type {ExcerptComposerProps} from "./ExcerptComposer";

const events=vi.hoisted(()=>new Map<string,(event:{payload:any})=>void>());
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn(async(name:string,handler:(event:{payload:any})=>void)=>{events.set(name,handler);return ()=>events.delete(name);})}));
vi.mock("./api",()=>({learningApi:{getCourse:vi.fn(),listCourses:vi.fn(),listSeries:vi.fn(),startPlayback:vi.fn(),getPlayback:vi.fn(),controlPlayback:vi.fn(),setViewport:vi.fn().mockResolvedValue(undefined),stopPlayback:vi.fn(),listSubtitles:vi.fn(),transcriptPage:vi.fn(),listCourseAnalyses:vi.fn(),getCourseAnalysis:vi.fn(),transcriptionComponents:vi.fn(),listCourseAnalysisTemplates:vi.fn().mockResolvedValue([])}}));
vi.mock("./ExcerptComposer",()=>({ExcerptComposer:({selection,onClose,onOpen}:ExcerptComposerProps)=>{const [error,setError]=useState("");return <section role="dialog" aria-modal="true" aria-label="学习摘录"><output data-testid="excerpt-selection">{JSON.stringify(selection)}</output>{error&&<p role="alert">{error}</p>}<button onClick={onClose}>关闭测试窗口</button><button onClick={()=>void Promise.resolve(onOpen({operationId:"operation",excerptId:"preview",noteId:"note",revision:1,warnings:[]})).catch(reason=>setError(String(reason)))}>打开已保存笔记</button></section>;}}));
vi.mock("../../lib/api",()=>({api:{listLibraryTasks:vi.fn().mockResolvedValue([]),getWorkspaceContext:vi.fn(),saveWorkspaceContext:vi.fn()}}));
const initial:PlaybackSnapshot={sessionId:"session",libraryId:"lib",courseId:"course",ready:true,positionMs:1500,durationMs:60000,paused:true,rate:1,volume:60,muted:false,ended:false,tracks:[{id:1,kind:"audio",label:"中文"},{id:2,kind:"sub",label:"中文字幕"}],audioId:1,subtitleId:null,fullscreen:false,error:null,saveError:null};
const props={libraryId:"lib",courseId:"course",onBack:vi.fn(),notify:vi.fn()};
beforeEach(()=>{
  vi.clearAllMocks();events.clear();
  vi.mocked(api.getWorkspaceContext).mockResolvedValue({libraryId:"lib",enabled:false,value:null});
  vi.mocked(api.saveWorkspaceContext).mockResolvedValue(true);
  vi.stubGlobal("ResizeObserver",class{observe(){}disconnect(){}});
  vi.mocked(learningApi.getCourse).mockResolvedValue({id:"course",mediaId:"media",title:"学习视频",positionMs:1500,durationMs:60000,rate:1,completed:false,progressRevision:1});
  vi.mocked(learningApi.startPlayback).mockResolvedValue(initial);
  vi.mocked(learningApi.getPlayback).mockResolvedValue(initial);
  vi.mocked(learningApi.stopPlayback).mockResolvedValue(undefined);
  vi.mocked(learningApi.controlPlayback).mockImplementation(async(_lib,_sid,action)=>({...initial,...(action.type==="fullscreen"?{fullscreen:action.enabled}:action.type==="pause"?{paused:action.paused}:action.type==="volume"?{volume:action.volume}:{})}));
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([]);
  vi.mocked(learningApi.transcriptionComponents).mockResolvedValue({version:"1.9.1",modelRevision:"fixed",toolReady:false,toolMessage:"工具未安装",models:[]});
  vi.mocked(learningApi.listCourseAnalyses).mockResolvedValue({items:[{id:"analysis",stage:"segments",status:"completed",completedStages:1,totalStages:1,error:"",createdAt:1000}],total:1,offset:0,limit:50});
  const saved:CourseAnalysisDetail={id:"analysis",courseId:"course",previewId:"preview",sourceFingerprint:"a".repeat(64),model:"deepseek-chat",parentAnalysisId:null,status:"completed",error:"",createdAt:1000,updatedAt:1000,staleReasons:[],stages:{items:[],total:0,offset:0,limit:50},selectedStageKey:null,result:{contentType:"tutorial",oneSentenceSummary:"课程要点",overview:"保存的课程说明",chapters:[],keyPoints:[{timeMs:11500,title:"材质参数",content:"学习参数",importance:3,sourceSegmentIds:["cue"]}],tutorialSteps:[],terms:[],actionItems:[],tags:[],warnings:[],sections:[],sectionValidation:[]}};
  vi.mocked(learningApi.getCourseAnalysis).mockResolvedValue(saved);
});
it("retains panel input across tabs and focus changes without restarting playback",async()=>{
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"课程分析"}));
  const sampling=await screen.findByRole("textbox",{name:"采样时间（秒）"});
  fireEvent.change(sampling,{target:{value:"12.5, 30"}});
  fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));
  await screen.findByRole("region",{name:"课程字幕"});
  fireEvent.click(screen.getByRole("button",{name:"课程分析"}));
  expect(await screen.findByRole("textbox",{name:"采样时间（秒）"})).toHaveValue("12.5, 30");
  fireEvent.click(screen.getByRole("button",{name:"固定学习资料"}));
  fireEvent.click(screen.getByRole("button",{name:"专注学习"}));
  expect(screen.queryByRole("region",{name:"课程分析"})).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"恢复面板布局"}));
  expect(screen.getByRole("textbox",{name:"采样时间（秒）"})).toHaveValue("12.5, 30");
  expect(learningApi.startPlayback).toHaveBeenCalledTimes(1);
  expect(learningApi.stopPlayback).not.toHaveBeenCalled();
});
it("persists only stable learning panel settings without overwriting video browsing context",async()=>{
  vi.mocked(api.getWorkspaceContext).mockResolvedValue({libraryId:"lib",enabled:true,value:{otherField:"keep"}});
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));
  await screen.findByRole("region",{name:"课程字幕"});
  await waitFor(()=>expect(api.saveWorkspaceContext).toHaveBeenCalledWith("lib","learning",expect.objectContaining({otherField:"keep",panels:expect.objectContaining({right:expect.objectContaining({mode:"closed",tab:"transcript"})})})));
  fireEvent.click(screen.getByRole("button",{name:"固定学习资料"}));
  await waitFor(()=>expect(api.saveWorkspaceContext).toHaveBeenLastCalledWith("lib","learning",expect.objectContaining({panels:expect.objectContaining({right:expect.objectContaining({mode:"pinned",width:340})})})));
  fireEvent.click(screen.getByRole("button",{name:"专注学习"}));
  await waitFor(()=>expect(api.saveWorkspaceContext).toHaveBeenLastCalledWith("lib","learning",expect.objectContaining({panels:expect.objectContaining({right:expect.objectContaining({mode:"pinned",width:340})})})));
  expect(vi.mocked(api.saveWorkspaceContext).mock.calls.every(call=>call[1]==="learning")).toBe(true);
});
it("does not replace an early panel action with a late restored layout",async()=>{
  let restore!:(value:Awaited<ReturnType<typeof api.getWorkspaceContext>>)=>void;
  vi.mocked(api.getWorkspaceContext).mockReturnValueOnce(new Promise(resolve=>{restore=resolve;}));
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));
  await screen.findByRole("region",{name:"课程字幕"});
  await act(async()=>restore({libraryId:"lib",enabled:true,value:{panels:{right:{mode:"pinned",width:500,tab:"analysis"}}}}));
  expect(screen.getByRole("region",{name:"课程字幕"})).toBeVisible();
  expect(screen.queryByRole("region",{name:"课程分析"})).toBeNull();
  await waitFor(()=>expect(api.saveWorkspaceContext).toHaveBeenCalledWith("lib","learning",expect.objectContaining({panels:expect.objectContaining({right:expect.objectContaining({mode:"closed",tab:"transcript",width:340})})})));
});
it.each(["read failure","other library"])("keeps playback available after a layout %s and retries without writing stale settings",async reason=>{
  if(reason==="read failure")vi.mocked(api.getWorkspaceContext).mockRejectedValueOnce(new Error("layout unavailable"));
  else vi.mocked(api.getWorkspaceContext).mockResolvedValueOnce({libraryId:"other",enabled:true,value:{panels:{right:{mode:"pinned",width:500,tab:"analysis"}}}});
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});
  expect(await screen.findByRole("alert")).toHaveTextContent("布局读取失败");
  expect(screen.queryByRole("region",{name:"课程分析"})).toBeNull();
  expect(api.saveWorkspaceContext).not.toHaveBeenCalled();
  vi.mocked(api.getWorkspaceContext).mockResolvedValueOnce({libraryId:"lib",enabled:true,value:{panels:{right:{mode:"pinned",width:420,tab:"analysis"}}}});
  fireEvent.click(screen.getByRole("button",{name:"重新读取布局"}));
  expect(await screen.findByRole("region",{name:"课程分析"})).toBeVisible();
  expect(screen.getByRole("separator",{name:"学习资料宽度"})).toHaveAttribute("aria-valuenow","420");
  expect(learningApi.startPlayback).toHaveBeenCalledTimes(1);
  expect(learningApi.stopPlayback).not.toHaveBeenCalled();
});
it("waits for readiness, pauses and seeks a note target once without resetting later manual seeks",async()=>{
  let current={...initial,ready:false};
  vi.mocked(learningApi.startPlayback).mockResolvedValue(current);
  vi.mocked(learningApi.getPlayback).mockImplementation(async()=>current);
  vi.mocked(learningApi.controlPlayback).mockImplementation(async(_lib,_session,action)=>{if(action.type==="seek")current={...current,positionMs:action.positionMs};if(action.type==="pause")current={...current,paused:action.paused};return current;});
  render(<LearningWorkspace {...props} initialPositionMs={5000} backLabel="返回笔记"/>);
  await screen.findByRole("button",{name:"返回笔记"});
  expect(vi.mocked(learningApi.controlPlayback).mock.calls.filter(call=>call[2].type==="seek")).toHaveLength(0);
  await act(async()=>{current={...current,ready:true,paused:false};events.get("learning-playback-state")?.({payload:current});});
  await waitFor(()=>expect(screen.getByRole("slider",{name:"播放进度"})).toHaveValue("5000"));
  expect(screen.getByRole("button",{name:"播放"})).toBeInTheDocument();
  fireEvent.change(screen.getByRole("slider",{name:"播放进度"}),{target:{value:"12000"}});
  await waitFor(()=>expect(screen.getByRole("slider",{name:"播放进度"})).toHaveValue("12000"));
  expect(vi.mocked(learningApi.controlPlayback).mock.calls.filter(call=>call[2].type==="seek"&&call[2].positionMs===5000)).toHaveLength(1);
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
  // Temporary overlays defer playback shortcuts; pin the series before testing its order.
  await waitFor(()=>{
    const action=vi.mocked(learningApi.controlPlayback).mock.calls.filter(call=>call[2].type==="shortcuts").at(-1)?.[2];
    expect(action).toMatchObject({type:"shortcuts",bindings:[]});
  });
  fireEvent.click(screen.getByRole("button",{name:"固定课程导航"}));
  await waitFor(()=>{
    const action=vi.mocked(learningApi.controlPlayback).mock.calls.filter(call=>call[2].type==="shortcuts").at(-1)?.[2];
    expect(action?.type==="shortcuts"&&action.bindings.length>0).toBe(true);
  });
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
it("captures selected subtitle IDs and current frame without sending displayed text",async()=>{
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([{id:"subtitle",courseId:"course",name:"字幕",format:"srt",sha256:"sha",cueCount:1}]);
  vi.mocked(learningApi.transcriptPage).mockResolvedValue({items:[{id:"cue",subtitleId:"subtitle",startMs:1000,endMs:2000,text:"页面上的字幕"}],total:1,offset:0,limit:50});
  render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));
  fireEvent.click(await screen.findByRole("checkbox",{name:"选择第 1 段字幕"}));fireEvent.click(screen.getByRole("button",{name:"创建学习摘录"}));
  expect(screen.getByTestId("excerpt-selection")).toHaveTextContent(JSON.stringify({subtitleId:"subtitle",cueIds:["cue"],ranges:[],comment:"",frameTimeMs:1500}));
  expect(screen.getByRole("dialog",{name:"学习摘录"})).toHaveAttribute("aria-modal","true");
});
it("does not navigate to a receipt until the playback leave guard succeeds",async()=>{
  const onOpenReceipt=vi.fn();vi.mocked(learningApi.stopPlayback).mockRejectedValueOnce(new Error("停止播放器失败"));
  render(<LearningWorkspace {...props} onOpenReceipt={onOpenReceipt}/>);await screen.findByRole("button",{name:"播放"});fireEvent.click(screen.getByRole("button",{name:"创建学习摘录"}));
  expect(screen.getByTestId("excerpt-selection")).toHaveTextContent('"ranges":[{"startMs":1500,"endMs":1501}]');
  fireEvent.click(screen.getByRole("button",{name:"打开已保存笔记"}));expect(await screen.findByRole("alert")).toHaveTextContent("停止播放器失败");expect(onOpenReceipt).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"打开已保存笔记"}));await waitFor(()=>expect(onOpenReceipt).toHaveBeenCalledWith(expect.objectContaining({noteId:"note"})));
});
it("never reuses another course's selected captions",async()=>{
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([{id:"subtitle",courseId:"course",name:"字幕",format:"srt",sha256:"sha",cueCount:1}]);
  vi.mocked(learningApi.transcriptPage).mockResolvedValue({items:[{id:"cue",subtitleId:"subtitle",startMs:1000,endMs:2000,text:"旧课程字幕"}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.startPlayback).mockImplementation(async(_lib,id)=>({...initial,courseId:id}));
  const {rerender}=render(<LearningWorkspace {...props}/>);await screen.findByRole("button",{name:"播放"});fireEvent.click(screen.getByRole("button",{name:"课程字幕"}));fireEvent.click(await screen.findByRole("checkbox",{name:"选择第 1 段字幕"}));
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([]);rerender(<LearningWorkspace {...props} courseId="another-course"/>);await waitFor(()=>expect(learningApi.startPlayback).toHaveBeenCalledWith("lib","another-course"));
  fireEvent.click(screen.getByRole("button",{name:"创建学习摘录"}));expect(screen.getByTestId("excerpt-selection")).toHaveTextContent('"cueIds":[]');expect(screen.getByTestId("excerpt-selection")).toHaveTextContent('"subtitleId":null');
});
it("waits for an in-flight player start before leaving and stops that session",async()=>{
  let finish!:(snapshot:PlaybackSnapshot)=>void;vi.mocked(learningApi.startPlayback).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const onBack=vi.fn();render(<LearningWorkspace {...props} onBack={onBack}/>);await waitFor(()=>expect(finish).toBeDefined());fireEvent.click(screen.getByRole("button",{name:"返回视频详情"}));
  expect(onBack).not.toHaveBeenCalled();expect(learningApi.stopPlayback).not.toHaveBeenCalled();await act(async()=>finish(initial));
  await waitFor(()=>expect(onBack).toHaveBeenCalledTimes(1));expect(learningApi.stopPlayback).toHaveBeenCalledWith("lib","session");
});
