import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";
import {LearningVideoImport} from "./LearningVideoImport";

vi.mock("@tauri-apps/plugin-dialog",()=>({open:vi.fn()}));
vi.mock("../../lib/api",()=>({api:{startLibraryTask:vi.fn()}}));
beforeEach(()=>vi.clearAllMocks());

it("queues explicitly selected videos for the current learning library",async()=>{
  vi.mocked(open).mockResolvedValue(["C:/synthetic/lesson.mp4"]);
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task-1"} as never);
  render(<LearningVideoImport libraryId="lib"/>);
  fireEvent.click(screen.getByRole("button",{name:"导入本地教程…"}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"learningMediaImport",request:{kind:"video",paths:["C:/synthetic/lesson.mp4"],folderId:null,tags:[],favorite:false}},"lib"));
  expect(await screen.findByRole("status")).toHaveTextContent("任务中心");
});

it("does not submit anything when file selection is cancelled",async()=>{
  vi.mocked(open).mockResolvedValue(null);
  render(<LearningVideoImport libraryId="lib"/>);
  fireEvent.click(screen.getByRole("button",{name:"导入本地教程…"}));
  await waitFor(()=>expect(open).toHaveBeenCalledOnce());
  expect(api.startLibraryTask).not.toHaveBeenCalled();
});

it("queues a selected tutorial folder for the learning library",async()=>{
  vi.mocked(open).mockResolvedValue("C:/synthetic/course");
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task-2"} as never);
  render(<LearningVideoImport libraryId="lib"/>);
  fireEvent.click(screen.getByRole("button",{name:"导入教程文件夹…"}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"learningMediaImport",request:{kind:"video",paths:["C:/synthetic/course"],folderId:null,tags:[],favorite:false}},"lib"));
  expect(open).toHaveBeenCalledWith(expect.objectContaining({directory:true}));
});

it("queues multiple videos into the chosen series",async()=>{
  vi.mocked(open).mockResolvedValue(["C:/synthetic/one.mp4","C:/synthetic/two.mp4"]);
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task-series"} as never);
  render(<LearningVideoImport libraryId="lib" series={[{id:"s1",name:"UE 教程",description:"",revision:0,orderedCourseIds:[]}]}/>);
  fireEvent.change(screen.getByLabelText("导入到系列"),{target:{value:"s1"}});
  fireEvent.click(screen.getByRole("button",{name:"导入本地教程…"}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"learningMediaImport",request:{kind:"video",paths:["C:/synthetic/one.mp4","C:/synthetic/two.mp4"],folderId:null,tags:[],favorite:false,seriesId:"s1"}},"lib"));
});
