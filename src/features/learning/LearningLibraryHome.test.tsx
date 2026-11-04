import {act,fireEvent,render,screen} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {LearningLibraryHome} from "./LearningLibraryHome";
import {learningApi} from "./api";

vi.mock("./api",()=>({learningApi:{listCourses:vi.fn(),listUnassignedVideos:vi.fn(),listSeries:vi.fn(),searchCourses:vi.fn()}}));
vi.mock("./LearningVideoImport",()=>({LearningVideoImport:()=> <div>教程导入</div>}));
vi.mock("./CourseAnalysisPanel",()=>({CourseAnalysisPanel:({courseId,autoPrepare}:{courseId:string;autoPrepare:boolean})=><div>分析 {courseId} {autoPrepare?"自动预览":""}</div>}));
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listUnassignedVideos).mockResolvedValue({items:[],total:0,offset:0,limit:50});vi.mocked(learningApi.listSeries).mockResolvedValue([]);});

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
it("browses a series in its stored course order",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c2","c1"]}]);
  vi.mocked(learningApi.searchCourses).mockResolvedValue({items:["c2","c1"].map(id=>({id,mediaId:`m-${id}`,title:`课程 ${id}`,positionMs:0,durationMs:null,rate:1,completed:false,progressRevision:1})),total:2,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"材质系列 · 2 课"}));
  const cards=await screen.findAllByRole("article");
  expect(cards.map(card=>card.querySelector("h3")?.textContent)).toEqual(["课程 c2","课程 c1"]);
});
it("reports unavailable series courses instead of inventing progress",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["missing"]}]);
  vi.mocked(learningApi.searchCourses).mockRejectedValue(new Error("课程不可用"));
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"材质系列 · 1 课"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("课程不可用");
  expect(screen.queryByText(/已学习 00:00/)).toBeNull();
});
it("searches the selected series in the current library",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c1"]}]);
  vi.mocked(learningApi.searchCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"材质系列 · 1 课"}));
  fireEvent.change(screen.getByRole("searchbox",{name:"搜索学习课程"}),{target:{value:"材质"}});
  expect(learningApi.searchCourses).toHaveBeenLastCalledWith("lib","材质","s1",0,50);
});
it("does not show a stale search result after the query changes",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  let finishFirst!:(value:Awaited<ReturnType<typeof learningApi.searchCourses>>)=>void;
  vi.mocked(learningApi.searchCourses).mockImplementation((_lib,query)=>query==="a"?new Promise(resolve=>{finishFirst=resolve;}):Promise.resolve({items:[],total:0,offset:0,limit:50}));
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  const input=screen.getByRole("searchbox",{name:"搜索学习课程"});
  fireEvent.change(input,{target:{value:"a"}});fireEvent.change(input,{target:{value:"ab"}});
  await screen.findByText("没有找到匹配的课程。");
  await act(async()=>finishFirst({items:[{id:"old",mediaId:"old",title:"旧结果",positionMs:0,durationMs:null,rate:1,completed:false,progressRevision:1}],total:1,offset:0,limit:50}));
  expect(screen.queryByText("旧结果")).toBeNull();
});
