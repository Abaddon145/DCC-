import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {QuickCourseAnalysis} from "./QuickCourseAnalysis";
import {learningApi} from "./api";
import {api} from "../../lib/api";

vi.mock("./api",()=>({learningApi:{listCourseAnalysisTemplates:vi.fn(),getCourseAnalysisDefault:vi.fn(),listSubtitles:vi.fn(),prepareCourseAnalysis:vi.fn(),listCourseAnalyses:vi.fn(),getCourseAnalysis:vi.fn()}}));
vi.mock("../../lib/api",()=>({api:{listLibraryTasks:vi.fn(),startLibraryTask:vi.fn(),getLibraryTask:vi.fn()}}));
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listCourseAnalysisTemplates).mockResolvedValue([{id:"custom",name:"材质",focus:"节点",version:2}]);vi.mocked(learningApi.getCourseAnalysisDefault).mockResolvedValue("general");vi.mocked(learningApi.listSubtitles).mockResolvedValue([{id:"sub",courseId:"course",name:"字幕",format:"srt",sha256:"a",cueCount:3}]);vi.mocked(learningApi.listCourseAnalyses).mockResolvedValue({items:[],total:0,offset:0,limit:50});vi.mocked(api.listLibraryTasks).mockResolvedValue([]);});
it("prepares and queues the selected version from one explicit click",async()=>{
  vi.mocked(learningApi.prepareCourseAnalysis).mockResolvedValue({previewId:"p",courseId:"course",expiresAt:Math.floor(Date.now()/1000)+60} as never);
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task",libraryId:"lib",status:"queued",payload:{kind:"courseAnalysis",request:{courseId:"course"}}} as never);
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" durationMs={60000} onSeek={()=>{}}/>);
  fireEvent.change(await screen.findByLabelText("分析模板"),{target:{value:"custom"}});
  expect(api.startLibraryTask).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"开始分析"}));
  await waitFor(()=>expect(learningApi.prepareCourseAnalysis).toHaveBeenCalledWith("lib","course",{subtitleId:"sub",frameTimesMs:[15000,30000,45000],templateId:"custom",templateVersion:2}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"courseAnalysis",request:{courseId:"course",previewId:"p",stage:"segments",autoSummary:true}},"lib"));
});
it("does not queue a task when local preparation fails",async()=>{
  vi.mocked(learningApi.prepareCourseAnalysis).mockRejectedValue(new Error("未配置密钥"));
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" onSeek={()=>{}}/>);
  fireEvent.click(await screen.findByRole("button",{name:"开始分析"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("未配置密钥");
  expect(api.startLibraryTask).not.toHaveBeenCalled();
});
it("refreshes available templates after settings change without starting analysis",async()=>{
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" onSeek={()=>{}}/>);
  await screen.findByRole("option",{name:"材质 · 自定义"});
  vi.mocked(learningApi.listCourseAnalysisTemplates).mockResolvedValue([{id:"new",name:"新模板",focus:"步骤",version:1}]);
  window.dispatchEvent(new CustomEvent("learning-analysis-templates-changed",{detail:{libraryId:"lib"}}));
  expect(await screen.findByRole("option",{name:"新模板 · 自定义"})).toBeInTheDocument();
  expect(api.startLibraryTask).not.toHaveBeenCalled();
});
it("loads the finished task's result instead of a different recent analysis",async()=>{
  const current={id:"task",libraryId:"lib",status:"running",payload:{kind:"courseAnalysis",request:{courseId:"course"}}};
  vi.mocked(api.listLibraryTasks).mockResolvedValue([current] as never);
  vi.mocked(api.getLibraryTask).mockResolvedValue({...current,status:"completed",phase:"completed",result:{analysisId:"result-id",courseId:"course"}} as never);
  vi.mocked(learningApi.getCourseAnalysis).mockResolvedValue({id:"result-id",stages:{items:[],offset:0,total:0},staleReasons:[],result:null} as never);
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" onSeek={()=>{}}/>);
  await waitFor(()=>expect(learningApi.getCourseAnalysis).toHaveBeenCalledWith("lib","course","result-id",null,0,50),{timeout:4000});
  expect(await screen.findByText("已完成")).toBeInTheDocument();
});
it("shows completed segments when the subsequent summary failed",async()=>{
  vi.mocked(learningApi.listCourseAnalyses).mockResolvedValue({items:[{id:"failed-summary"}] as never,total:1,offset:0,limit:1});
  vi.mocked(learningApi.getCourseAnalysis).mockImplementation(async(_lib,_course,id,key)=>id==="failed-summary"?{id,parentAnalysisId:"segments",stages:{items:[],offset:0,total:0},staleReasons:[],result:null} as never:{id,parentAnalysisId:null,stages:{items:[{key:"s",status:"completed"}],offset:0,total:1},staleReasons:[],result:key?{oneSentenceSummary:"保留的分段",overview:"",chapters:[],keyPoints:[],tutorialSteps:[],terms:[],actionItems:[],sections:[],tags:[],warnings:[],sectionValidation:[]}:null} as never);
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" onSeek={()=>{}}/>);
  expect(await screen.findByText("保留的分段")).toBeInTheDocument();
});

it("shows material readiness and switches analysis result tabs",async()=>{
  vi.mocked(learningApi.listCourseAnalyses).mockResolvedValue({items:[{id:"done",stage:"summary",status:"completed",completedStages:1,totalStages:1,error:"",createdAt:1700000000}],total:1,offset:0,limit:10});
  vi.mocked(learningApi.getCourseAnalysis).mockResolvedValue({
    id:"done",courseId:"course",previewId:"p",sourceFingerprint:"fp",model:"deepseek",parentAnalysisId:null,status:"completed",error:"",createdAt:1,updatedAt:1,staleReasons:[],
    stages:{items:[],offset:0,total:0,limit:50},selectedStageKey:null,
    result:{
      contentType:"tutorial",oneSentenceSummary:"总结一句",overview:"总览正文",
      chapters:[{startMs:0,endMs:5000,title:"第一章",summary:"章节说明",keywords:[],keyframeIds:[]}],
      keyPoints:[{timeMs:10000,title:"重点A",content:"重点内容",importance:3,sourceSegmentIds:[]}],
      tutorialSteps:[{order:1,timeMs:20000,title:"步骤一",description:"先做这个",notes:[]}],
      terms:[],actionItems:[],sections:[],tags:["UE5"],warnings:[],sectionValidation:[],
    },
  } as never);
  render(<QuickCourseAnalysis libraryId="lib" courseId="course" courseTitle="UE材质" canSeek onSeek={vi.fn()}/>);
  expect(await screen.findByText(/材料已就绪/)).toBeInTheDocument();
  expect(await screen.findByText("总结一句")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab",{name:"操作步骤 (1)"}));
  expect(screen.getByText("先做这个")).toBeInTheDocument();
  expect(screen.queryByText("总览正文")).toBeNull();
});
