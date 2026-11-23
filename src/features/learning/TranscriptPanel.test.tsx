import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {TranscriptPanel} from "./TranscriptPanel";
import {learningApi} from "./api";
import type {TranscriptCue} from "./types";
vi.mock("./api",()=>({learningApi:{listSubtitles:vi.fn(),transcriptPage:vi.fn(),searchTranscriptCues:vi.fn(),importSubtitle:vi.fn(),loadSubtitle:vi.fn(),transcriptionComponents:vi.fn()}}));
vi.mock("../../lib/api",()=>({api:{listLibraryTasks:vi.fn().mockResolvedValue([])}}));
vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn().mockResolvedValue(null)}));
const subtitle={id:"subtitle",courseId:"course",name:"中文字幕.srt",format:"srt" as const,sha256:"a".repeat(64),cueCount:55};
const cues:TranscriptCue[]=Array.from({length:55},(_,n)=>({id:`cue-${n}`,subtitleId:"subtitle",startMs:n*1000+125,endMs:n*1000+875,text:`完整中文🧑🏽‍🎨 ${n}\n第二行`}));
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listSubtitles).mockResolvedValue([subtitle]);vi.mocked(learningApi.transcriptPage).mockImplementation(async(_lib,_course,_id,offset=0)=>({items:cues.slice(offset,offset+50),total:55,offset,limit:50}));});
it("exposes optional local transcription without automatically installing components",async()=>{
  vi.mocked(learningApi.transcriptionComponents).mockResolvedValue({version:"1.9.1",modelRevision:"fixed",toolReady:false,toolMessage:"工具未安装",models:[]});
  render(<TranscriptPanel libraryId="lib" courseId="course" onSeek={vi.fn()}/>);
  await screen.findByText(/完整中文.* 0/);
  expect(learningApi.transcriptionComponents).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"本地转写"}));
  expect(await screen.findByRole("region",{name:"本地语音转写"})).toBeInTheDocument();
  expect(await screen.findByText("工具未安装")).toBeInTheDocument();
});
it("pages through all cues, seeks real time and retains complete selections across pages",async()=>{
  const onSeek=vi.fn(),onSelection=vi.fn();render(<TranscriptPanel libraryId="lib" courseId="course" sessionId="session" onSeek={onSeek} onSelection={onSelection}/>);
  expect(await screen.findByText(/完整中文.* 0/)).toHaveTextContent("第二行");
  fireEvent.click(screen.getByRole("checkbox",{name:"选择第 1 段字幕"}));
  fireEvent.click(screen.getByRole("button",{name:"下一页字幕"}));
  expect(await screen.findByText(/完整中文.* 54/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox",{name:"选择第 55 段字幕"}));
  expect(onSelection).toHaveBeenLastCalledWith([cues[0],cues[54]]);
  fireEvent.click(screen.getByRole("button",{name:/跳转到 00:54/}));expect(onSeek).toHaveBeenLastCalledWith(54125);
});
it("does not expose old-library cues after a delayed response and reports missing files",async()=>{
  let finish!:(value:Awaited<ReturnType<typeof learningApi.transcriptPage>>)=>void;
  vi.mocked(learningApi.transcriptPage).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  const {rerender}=render(<TranscriptPanel libraryId="lib" courseId="course" onSeek={vi.fn()}/>);
  await waitFor(()=>expect(finish).toBeDefined());
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([]);
  rerender(<TranscriptPanel libraryId="new-library" courseId="new-course" onSeek={vi.fn()}/>);
  await screen.findByText("尚未导入字幕");
  await act(async()=>finish({items:[cues[0]],offset:0,limit:50,total:1}));
  expect(screen.queryByText(/完整中文/)).toBeNull();
  vi.mocked(learningApi.listSubtitles).mockResolvedValue([subtitle]);vi.mocked(learningApi.transcriptPage).mockRejectedValue(new Error("学习资料缺失"));
  rerender(<TranscriptPanel libraryId="lib" courseId="course" onSeek={vi.fn()}/>);
  expect(await screen.findByRole("alert")).toHaveTextContent("学习资料缺失");
});

it("highlights the active cue by playback position and searches transcript cues",async()=>{
  vi.mocked(learningApi.searchTranscriptCues).mockResolvedValue({items:[cues[5]],total:1,offset:0,limit:50});
  render(<TranscriptPanel libraryId="lib" courseId="course" positionMs={1500} onSeek={vi.fn()}/>);
  const activeBtn=await screen.findByRole("button",{name:"跳转到 00:01 · 第 2 段"});
  expect(activeBtn.closest("li")).toHaveClass("active-cue");
  fireEvent.change(screen.getByRole("searchbox",{name:"搜索字幕内容"}),{target:{value:"5"}});
  await waitFor(()=>expect(learningApi.searchTranscriptCues).toHaveBeenCalledWith("lib","course","subtitle","5",0,50));
  expect(await screen.findByText(/完整中文.* 5 第二行/)).toBeInTheDocument();
});
