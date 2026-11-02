import {act,fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {invoke} from "@tauri-apps/api/core";
import {learningApi} from "./api";
import CourseSqliteImport from "./CourseSqliteImport";

vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn()}));
const mediaId="11111111-1111-4111-8111-111111111111";
const source={selectionId:"selection",libraryId:"lib",profileId:"profile",epoch:"epoch",fileName:"synthetic.sqlite"};
const preview=()=>({previewId:"preview",selectionId:"selection",sourceFingerprint:"source-hash",targetFingerprint:"target-hash",expiresAt:Math.floor(Date.now()/1000)+60,series:[{id:"series",title:"系列标题",items:[{id:"course",title:"课程一",mediaId:null,sourceProgress:null,targetProgress:{positionMs:1200,durationMs:null,rate:1,completed:false},conflictCodes:["unresolved"]}]}]});
beforeEach(()=>{vi.clearAllMocks();vi.mocked(invoke).mockImplementation(async command=>command==="preview_learning_course_import"?preview():{id:"task",label:"课程迁入",status:"queued"} as never);});

it("strips runtime metadata from explicit bindings and queues only server preview identity",async()=>{
  await learningApi.previewCourseImport("lib","selection",[{sourceItemId:"course",mediaId,path:"private",sourceFingerprint:"forged"}] as never);
  expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[{sourceItemId:"course",mediaId}]});
  await learningApi.queueCourseImport("lib","selection","preview");
  expect(invoke).toHaveBeenLastCalledWith("queue_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",previewId:"preview"});
});

it("previews only explicitly selected complete series from a large source",async()=>{
  vi.mocked(invoke).mockImplementation(async command=>command==="list_learning_course_source_series"?[{id:"series",title:"可迁入系列",courseCount:1},{id:"large",title:"超限系列",courseCount:10000}]:preview() as never);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择系列分批迁入"}));
  const small=await screen.findByRole("checkbox",{name:/可迁入系列/});
  expect(screen.getByRole("checkbox",{name:/超限系列/})).toBeDisabled();
  expect(screen.getByRole("button",{name:"预检课程数据库"})).toBeDisabled();
  fireEvent.click(small);fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));
  await screen.findByText("课程一");
  expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[],seriesIds:["series"]});
  fireEvent.click(screen.getByRole("checkbox",{name:/可迁入系列/}));
  expect(screen.queryByText("课程一")).not.toBeInTheDocument();
});

it("allows a large complete series and enforces the combined 10000 item limit",async()=>{
  vi.mocked(invoke).mockImplementation(async command=>command==="list_learning_course_source_series"?[{id:"large",title:"大系列",courseCount:9999},{id:"extra",title:"额外系列",courseCount:0}]:preview() as never);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择系列分批迁入"}));
  fireEvent.click(await screen.findByRole("checkbox",{name:/大系列/}));
  expect(screen.getByRole("button",{name:"预检课程数据库"})).toBeEnabled();
  fireEvent.click(screen.getByRole("checkbox",{name:/额外系列/}));
  expect(screen.getByRole("button",{name:"预检课程数据库"})).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox",{name:/额外系列/}));
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));
  await screen.findByText("课程一");
  expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[],seriesIds:["large"]});
});

it("selects a registered video from search and requires a fresh preview",async()=>{
  vi.mocked(invoke).mockImplementation(async command=>command==="preview_learning_course_import"?preview():command==="search_media_entries"?{items:[{id:mediaId,name:"本地视频"}],total:1,offset:0,limit:50}:{id:"task"} as never);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await screen.findByText("课程一");
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button",{name:"为课程一选择本地视频"}));
  fireEvent.change(screen.getByRole("textbox",{name:"搜索本地视频"}),{target:{value:"本地"}});
  fireEvent.click(screen.getByRole("button",{name:"搜索视频"}));
  fireEvent.click(await screen.findByRole("button",{name:`选择 本地视频 ${mediaId}`}));
  expect(screen.getByRole("textbox",{name:"课程一 · 目标媒体 UUID"})).toHaveValue(mediaId);
  expect(screen.getByRole("button",{name:"排队迁入课程"})).toBeDisabled();
  fireEvent.click(screen.getByRole("button",{name:"重新预检课程数据库"}));
  await waitFor(()=>expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[{sourceItemId:"course",mediaId}]}));
});

it("ignores an old video search after the picker closes",async()=>{
  let finish!:(value:unknown)=>void;
  vi.mocked(invoke).mockImplementation(async command=>command==="preview_learning_course_import"?preview():new Promise(resolve=>{finish=resolve}) as never);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await screen.findByText("课程一");
  fireEvent.click(screen.getByRole("button",{name:"为课程一选择本地视频"}));fireEvent.click(screen.getByRole("button",{name:"搜索视频"}));
  fireEvent.click(screen.getByRole("button",{name:"取消选择"}));
  fireEvent.click(screen.getByRole("button",{name:"为课程一选择本地视频"}));
  await act(async()=>finish({items:[{id:mediaId,name:"过期视频"}],total:1,offset:0,limit:50}));
  expect(screen.queryByText(/过期视频/)).not.toBeInTheDocument();
});

it("prevents changing the media binding while a preview is in progress",async()=>{
  let finish!:(value:ReturnType<typeof preview>)=>void;
  vi.mocked(invoke).mockImplementation(command=>command==="preview_learning_course_import"?new Promise(resolve=>{finish=resolve}) as never:Promise.resolve({items:[{id:mediaId,name:"本地视频"}],total:1,offset:0,limit:50}) as never);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await act(async()=>finish(preview()));
  fireEvent.click(screen.getByRole("button",{name:"为课程一选择本地视频"}));fireEvent.click(screen.getByRole("button",{name:"搜索视频"}));
  const option=await screen.findByRole("button",{name:`选择 本地视频 ${mediaId}`});
  fireEvent.click(screen.getByRole("button",{name:"重新预检课程数据库"}));
  expect(option).toBeDisabled();fireEvent.click(option);
  await act(async()=>finish(preview()));
  expect(screen.getByRole("textbox",{name:"课程一 · 目标媒体 UUID"})).toHaveValue("");
});

it("allows confirming a preview containing an empty series",async()=>{
  const empty=preview();empty.series[0].items=[];vi.mocked(invoke).mockResolvedValue(empty);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await screen.findByText(/系列：系列标题/);
  fireEvent.click(screen.getByRole("checkbox"));expect(screen.getByRole("button",{name:"排队迁入课程"})).toBeEnabled();
});

it("requires explicit preview and review, shows unknown progress, and clears approval when binding changes",async()=>{
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));
  await screen.findByText("课程一");
  expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[]});
  expect(screen.getByText("系列标题")).toBeInTheDocument();
  expect(screen.getByText("来源进度：未知")).toBeInTheDocument();
  expect(screen.getByText(/目标进度：1.2 秒.*时长 未知/)).toBeInTheDocument();
  expect(screen.getByText(/未绑定目标媒体/)).toBeInTheDocument();
  const queue=screen.getByRole("button",{name:"排队迁入课程"});
  expect(queue).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox"));expect(queue).toBeEnabled();
  fireEvent.change(screen.getByRole("textbox",{name:"课程一 · 目标媒体 UUID"}),{target:{value:mediaId}});
  expect(screen.getByRole("checkbox")).not.toBeChecked();expect(queue).toBeDisabled();
  fireEvent.click(screen.getByRole("button",{name:"重新预检课程数据库"}));
  await waitFor(()=>expect(invoke).toHaveBeenLastCalledWith("preview_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",bindings:[{sourceItemId:"course",mediaId}]}));
  fireEvent.click(screen.getByRole("checkbox"));fireEvent.click(queue);
  expect(await screen.findByText(/已排队.*任务中心/)).toBeInTheDocument();
  expect(screen.getByRole("checkbox")).not.toBeChecked();expect(queue).toBeDisabled();
  expect(invoke).toHaveBeenLastCalledWith("queue_learning_course_import",{expectedLibraryId:"lib",selectionId:"selection",previewId:"preview"});
});

it("blocks invalid UUID bindings and displays backend preview failures",async()=>{
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await screen.findByText("课程一");
  fireEvent.change(screen.getByRole("textbox",{name:"课程一 · 目标媒体 UUID"}),{target:{value:"C:/source/video.mp4"}});
  fireEvent.click(screen.getByRole("button",{name:"重新预检课程数据库"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("UUID");expect(invoke).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByRole("textbox",{name:"课程一 · 目标媒体 UUID"}),{target:{value:mediaId}});
  vi.mocked(invoke).mockRejectedValueOnce(new Error("媒体未登记"));
  fireEvent.click(screen.getByRole("button",{name:"重新预检课程数据库"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("媒体未登记");expect(screen.getByRole("button",{name:"排队迁入课程"})).toBeDisabled();
});

it("shows source changes for both series and courses",async()=>{
  const changed=preview();
  vi.mocked(invoke).mockResolvedValue({...changed,series:changed.series.map(series=>({...series,conflictCodes:["source_changed"],items:series.items.map(item=>({...item,conflictCodes:["target_changed"]}))}))});
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("系列 系列标题：来源已变");
  expect(screen.getByText(/冲突：目标已改/)).toBeInTheDocument();
});

it("paginates course rows at 100 and refuses an expired preview",async()=>{
  const many=preview();many.expiresAt=Math.floor(Date.now()/1000)+1;many.series[0].items=Array.from({length:101},(_,index)=>({...many.series[0].items[0],id:`course-${index}`,title:`课程 ${index}`}));
  vi.mocked(invoke).mockResolvedValue(many);
  render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));await screen.findByText("课程 99");
  expect(within(screen.getByRole("table")).getAllByRole("row")).toHaveLength(101);expect(screen.queryByText("课程 100")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"下一页课程"}));expect(screen.getByText("课程 100")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("checkbox"));
  const now=vi.spyOn(Date,"now").mockReturnValue((many.expiresAt+1)*1000);
  try{fireEvent.click(screen.getByRole("button",{name:"排队迁入课程"}));await screen.findByRole("alert");expect(invoke).toHaveBeenCalledTimes(1);}finally{now.mockRestore();}
});

it("ignores a preview response after its source component unmounts",async()=>{
  let finish!:(value:ReturnType<typeof preview>)=>void;
  vi.mocked(invoke).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}) as never);
  const view=render(<CourseSqliteImport libraryId="lib" source={source}/>);
  fireEvent.click(screen.getByRole("button",{name:"预检课程数据库"}));view.unmount();
  await act(async()=>finish(preview()));expect(screen.queryByText("课程一")).not.toBeInTheDocument();
});
