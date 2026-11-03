import {fireEvent,render,screen} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {LearningLibraryHome} from "./LearningLibraryHome";
import {learningApi} from "./api";

vi.mock("./api",()=>({learningApi:{listCourses:vi.fn(),listUnassignedVideos:vi.fn()}}));
vi.mock("./LearningVideoImport",()=>({LearningVideoImport:()=> <div>教程导入</div>}));
vi.mock("./CourseAnalysisPanel",()=>({CourseAnalysisPanel:({courseId,autoPrepare}:{courseId:string;autoPrepare:boolean})=><div>分析 {courseId} {autoPrepare?"自动预览":""}</div>}));
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listUnassignedVideos).mockResolvedValue({items:[],total:0,offset:0,limit:50});});

it("shows courses from the learning library and opens the chosen course",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[{id:"c1",mediaId:"m1",title:"教程一",positionMs:30000,durationMs:60000,rate:1,completed:false,progressRevision:1}],total:1,offset:0,limit:50});
  const onOpen=vi.fn();render(<LearningLibraryHome libraryId="lib" onOpen={onOpen} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"继续学习 教程一"}));
  expect(onOpen).toHaveBeenCalledWith("c1","m1");
});

it("shows unassigned learning videos with unknown progress without creating courses",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listUnassignedVideos).mockResolvedValue({items:[{mediaId:"m2",name:"未建课视频"}],total:1,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  expect(await screen.findByText("未建课视频")).toBeInTheDocument();
  expect(screen.getByText("尚未建课 · 进度未知")).toBeInTheDocument();
});

it("opens quick analysis for a selected course without starting playback",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[{id:"c1",mediaId:"m1",title:"教程一",positionMs:0,durationMs:60000,rate:1,completed:false,progressRevision:1}],total:1,offset:0,limit:50});
  const onOpen=vi.fn();render(<LearningLibraryHome libraryId="lib" onOpen={onOpen} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"分析视频 教程一"}));
  expect(screen.getByText("分析 c1 自动预览")).toBeInTheDocument();
  expect(onOpen).not.toHaveBeenCalled();
});
