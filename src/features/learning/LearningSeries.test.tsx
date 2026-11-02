import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {LearningSeries} from "./LearningSeries";
import {learningApi} from "./api";
vi.mock("./api",()=>({learningApi:{listSeries:vi.fn(),getCourse:vi.fn(),upsertSeries:vi.fn(),setSeriesMembers:vi.fn(),deleteSeries:vi.fn()}}));
const first={id:"a",name:"UE入门",description:"",revision:3,orderedCourseIds:["one","two"]};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listSeries).mockResolvedValue([first,{...first,id:"b",name:"材质练习",orderedCourseIds:["two"]}]);vi.mocked(learningApi.getCourse).mockImplementation(async(_lib,id)=>({id,mediaId:`media-${id}`,title:`课程 ${id}`,positionMs:500,durationMs:1000,rate:1,completed:false,progressRevision:1}));vi.mocked(learningApi.setSeriesMembers).mockImplementation(async(_lib,_id,revision,ids)=>({...first,revision:revision+1,orderedCourseIds:ids}));});
it("uses the selected series order and revision without copying a shared course",async()=>{
  const onOpen=vi.fn().mockResolvedValue(undefined),onOrderChange=vi.fn();render(<LearningSeries libraryId="lib" courseId="one" onOpen={onOpen} onOrderChange={onOrderChange}/>);
  expect(await screen.findByText("课程 two")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"上移 课程 two"}));
  await waitFor(()=>expect(learningApi.setSeriesMembers).toHaveBeenCalledWith("lib","a",3,["two","one"]));
  await waitFor(()=>expect(onOrderChange).toHaveBeenLastCalledWith(["two","one"]));
  fireEvent.change(screen.getByLabelText("当前系列"),{target:{value:"b"}});
  fireEvent.click(await screen.findByRole("button",{name:"打开 课程 two"}));
  await waitFor(()=>expect(onOpen).toHaveBeenCalledWith("two"));
});
it("retains the existing order and reports a revision conflict",async()=>{
  vi.mocked(learningApi.setSeriesMembers).mockRejectedValue(new Error("系列顺序已变化，请重新读取"));
  render(<LearningSeries libraryId="lib" courseId="one" onOpen={vi.fn()} onOrderChange={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:"上移 课程 two"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("系列顺序已变化");
  expect(screen.getByRole("button",{name:"上移 课程 one"})).toBeDisabled();
});
