import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {LearningLibraryHome} from "./LearningLibraryHome";
import {learningApi} from "./api";

vi.mock("./api",()=>({learningApi:{listCourses:vi.fn(),listUnassignedVideos:vi.fn(),listSeries:vi.fn(),upsertSeries:vi.fn(),deleteSeries:vi.fn(),setSeriesMembers:vi.fn(),searchCourses:vi.fn(),listCategories:vi.fn(),createCategory:vi.fn(),renameCategory:vi.fn(),deleteCategory:vi.fn(),assignSeriesCategory:vi.fn(),ensureCourse:vi.fn()}}));
vi.mock("./LearningVideoImport",()=>({LearningVideoImport:()=> <div>教程导入</div>}));
vi.mock("./QuickCourseAnalysis",()=>({QuickCourseAnalysis:({courseId}:{courseId:string})=><div>分析 {courseId}</div>}));
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listUnassignedVideos).mockResolvedValue({items:[],total:0,offset:0,limit:50});vi.mocked(learningApi.listSeries).mockResolvedValue([]);vi.mocked(learningApi.listCategories).mockResolvedValue([]);});

it("dismisses the import and series popovers when clicking outside",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  const importTrigger=screen.getByText("＋ 导入教程"),seriesTrigger=screen.getByText("＋ 新建系列");
  fireEvent.click(importTrigger);expect(importTrigger.closest("details")!.open).toBe(true);
  fireEvent.pointerDown(seriesTrigger);fireEvent.click(seriesTrigger);
  expect(importTrigger.closest("details")!.open).toBe(false);expect(seriesTrigger.closest("details")!.open).toBe(true);
  fireEvent.pointerDown(screen.getByLabelText("新系列名称"));expect(seriesTrigger.closest("details")!.open).toBe(true);
  fireEvent.pointerDown(screen.getByRole("heading",{name:"学习库"}));expect(seriesTrigger.closest("details")!.open).toBe(false);
});

it("adds a course to an existing series without removing its other members",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[{id:"c1",mediaId:"m1",title:"教程一",positionMs:0,durationMs:null,rate:1,completed:false,progressRevision:0}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:3,orderedCourseIds:["c2"]}]);
  vi.mocked(learningApi.setSeriesMembers).mockResolvedValue({id:"s1",name:"材质系列",description:"",revision:4,orderedCourseIds:["c2","c1"]});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.change(await screen.findByLabelText("教程一加入系列"),{target:{value:"s1"}});
  await waitFor(()=>expect(learningApi.setSeriesMembers).toHaveBeenCalledWith("lib","s1",3,["c2","c1"]));
});

it("keeps category navigation separate from series browsing and opens a series detail",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"学习材质",revision:1,orderedCourseIds:["c1"]}]);
  vi.mocked(learningApi.searchCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  expect(screen.getByRole("navigation",{name:"学习库分类导航"})).toBeInTheDocument();
  fireEvent.click(await screen.findByRole("button",{name:"材质系列 · 1 课"}));
  expect(await screen.findByRole("heading",{name:"材质系列"})).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"返回系列列表"})).toBeInTheDocument();
});
it("shows a series title once with a concise course count",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c1"]}]);
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  const card=await screen.findByRole("button",{name:"材质系列 · 1 课"});
  expect(card.textContent?.match(/材质系列/g)).toHaveLength(1);
  expect(card).toHaveTextContent("1 课");
});
it("creates a series from the library header",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.upsertSeries).mockResolvedValue({id:"new",name:"新教程",description:"",revision:0,orderedCourseIds:[]});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.click(screen.getByText("＋ 新建系列"));
  fireEvent.change(screen.getByLabelText("新系列名称"),{target:{value:"新教程"}});
  fireEvent.click(screen.getByRole("button",{name:"创建系列"}));
  await waitFor(()=>expect(learningApi.upsertSeries).toHaveBeenCalledWith("lib",{name:"新教程",description:""}));
});
it("offers in-progress courses without labeling untouched courses as recent study",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[{id:"started",mediaId:"m1",title:"已学习课程",positionMs:1000,durationMs:60000,rate:1,completed:false,progressRevision:1},{id:"new",mediaId:"m2",title:"未开始课程",positionMs:0,durationMs:60000,rate:1,completed:false,progressRevision:0}],total:2,offset:0,limit:50});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  const section=await screen.findByRole("region",{name:"继续学习"});
  expect(section).toHaveTextContent("已学习课程");
  expect(section).not.toHaveTextContent("未开始课程");
});

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
  expect(screen.getByText("分析 c1")).toBeInTheDocument();
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
it("creates a learning category and assigns a series without touching courses",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c1"]}]);
  vi.mocked(learningApi.createCategory).mockImplementation(async()=>{vi.mocked(learningApi.listCategories).mockResolvedValue([{id:"cat1",name:"材质",revision:1}]);return {id:"cat1",name:"材质",revision:1};});
  vi.mocked(learningApi.assignSeriesCategory).mockImplementation(async()=>{const saved={id:"s1",name:"材质系列",description:"",revision:2,orderedCourseIds:["c1"],categoryId:"cat1"};vi.mocked(learningApi.listSeries).mockResolvedValue([saved]);return saved;});
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  fireEvent.click(await screen.findByText("管理分类"));
  fireEvent.change(screen.getByLabelText("新分类名称"),{target:{value:"材质"}});
  fireEvent.click(screen.getByRole("button",{name:"创建分类"}));
  await screen.findByRole("button",{name:"材质 · 0 系列"});
  await waitFor(()=>expect(screen.getByLabelText("材质系列的分类")).toBeEnabled());
  fireEvent.change(screen.getByLabelText("材质系列的分类"),{target:{value:"cat1"}});
  expect(await screen.findByRole("button",{name:"材质 · 1 系列"})).toBeInTheDocument();
  expect(learningApi.assignSeriesCategory).toHaveBeenCalledWith("lib","s1",1,"cat1");
});
it("reloads series after an assignment so an overlapping refresh cannot restore stale data",async()=>{
  const old={id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c1"],categoryId:null};
  const saved={...old,revision:2,categoryId:"cat1"};
  let finishRefresh!:(value:typeof old[])=>void;
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listCategories).mockResolvedValue([{id:"cat1",name:"材质",revision:1}]);
  vi.mocked(learningApi.listSeries).mockResolvedValueOnce([old]).mockImplementationOnce(()=>new Promise(resolve=>{finishRefresh=resolve;})).mockResolvedValue([saved]);
  vi.mocked(learningApi.assignSeriesCategory).mockResolvedValue(saved);
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  await screen.findByLabelText("材质系列的分类");
  act(()=>window.dispatchEvent(new CustomEvent("learning-memberships-changed",{detail:{libraryId:"lib"}})));
  await waitFor(()=>expect(finishRefresh).toBeDefined());
  fireEvent.change(screen.getByLabelText("材质系列的分类"),{target:{value:"cat1"}});
  await waitFor(()=>expect(learningApi.listSeries).toHaveBeenCalledTimes(3));
  await act(async()=>finishRefresh([old]));
  expect(await screen.findByRole("button",{name:"材质 · 1 系列"})).toBeInTheDocument();
});
it("requires an explicit in-app confirmation before deleting a category",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listCategories).mockResolvedValue([{id:"cat1",name:"材质",revision:1}]);
  vi.mocked(learningApi.deleteCategory).mockResolvedValue(undefined);
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  await screen.findByRole("button",{name:"材质 · 0 系列"});
  fireEvent.click(screen.getByText("管理分类"));
  fireEvent.click(screen.getByRole("button",{name:"删除分类"}));
  expect(learningApi.deleteCategory).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog",{name:"删除分类确认"})).toHaveTextContent("课程不会删除");
  fireEvent.click(screen.getByRole("button",{name:"取消删除"}));
  expect(learningApi.deleteCategory).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"删除分类"}));
  fireEvent.click(screen.getByRole("button",{name:"确认删除分类"}));
  await waitFor(()=>expect(learningApi.deleteCategory).toHaveBeenCalledWith("lib","cat1",1));
});

it("shows an available series thumbnail and falls back when it cannot load",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.listSeries).mockResolvedValue([{id:"s1",name:"材质系列",description:"",revision:1,orderedCourseIds:["c1"],coverFileId:"11111111-1111-4111-8111-111111111111"}]);
  render(<LearningLibraryHome libraryId="lib" onOpen={vi.fn()} onOrganize={vi.fn()}/>);
  const cover=await screen.findByAltText("材质系列封面");
  expect(cover).toHaveAttribute("src","http://dcc-media.localhost/library/file/11111111-1111-4111-8111-111111111111");
  fireEvent.error(cover);
  expect(screen.queryByAltText("材质系列封面")).not.toBeInTheDocument();
  expect(screen.getByRole("button",{name:"材质系列 · 1 课"})).toBeInTheDocument();
});

it("filters by learning status and allows one-click course creation for unassigned videos",async()=>{
  vi.mocked(learningApi.listCourses).mockResolvedValue({items:[],total:0,offset:0,limit:50});
  vi.mocked(learningApi.searchCourses).mockResolvedValue({items:[{id:"c-done",mediaId:"m-done",title:"已完成教程",positionMs:60000,durationMs:60000,rate:1,completed:true,progressRevision:1}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.listUnassignedVideos).mockResolvedValue({items:[{mediaId:"m-new",name:"待建课视频"}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.ensureCourse).mockResolvedValue({id:"c-new",mediaId:"m-new",title:"待建课视频",positionMs:0,durationMs:null,rate:1,completed:false,progressRevision:0});
  const onOpen=vi.fn();
  render(<LearningLibraryHome libraryId="lib" onOpen={onOpen} onOrganize={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"已完成"}));
  await waitFor(()=>expect(learningApi.searchCourses).toHaveBeenCalledWith("lib","",null,0,50,{categoryId:null,status:"completed",sort:"default"}));
  expect(await screen.findByText("已完成教程")).toBeInTheDocument();
  fireEvent.click(await screen.findByRole("button",{name:"建课并学习"}));
  await waitFor(()=>expect(learningApi.ensureCourse).toHaveBeenCalledWith("lib","m-new"));
  expect(onOpen).toHaveBeenCalledWith("c-new","m-new");
});
