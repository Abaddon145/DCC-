import {act,fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {ProjectLearningNotes} from "./ProjectLearningNotes";
import {learningApi} from "../features/learning/api";
const knowledge=vi.hoisted(()=>({search:vi.fn()}));
vi.mock("../features/learning/api",()=>({learningApi:{listProjectNotes:vi.fn(),linkProjectNote:vi.fn(),unlinkProjectNote:vi.fn()}}));
vi.mock("../lib/knowledge",()=>({knowledgeApi:()=>knowledge}));
const linked={id:"note-1",title:"灯光学习",updatedAt:"2026-01-01",courseCount:2};
const props={libraryId:"library",projectId:"project",archived:false,onOpenNote:vi.fn(),notify:vi.fn()};
beforeEach(()=>{vi.clearAllMocks();knowledge.search.mockResolvedValue({items:[{id:"note-2",title:"新的场景笔记",plainText:"",tags:[],favorite:false,updatedAt:""}],total:1});vi.mocked(learningApi.listProjectNotes).mockResolvedValue({items:[linked],total:1,offset:0,limit:50});vi.mocked(learningApi.linkProjectNote).mockResolvedValue();vi.mocked(learningApi.unlinkProjectNote).mockResolvedValue();});
it("opens the real note and unlinks only its project association",async()=>{
  render(<ProjectLearningNotes {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"打开笔记：灯光学习"}));expect(props.onOpenNote).toHaveBeenCalledWith("note-1");
  fireEvent.click(screen.getByRole("button",{name:"解除关联：灯光学习"}));await waitFor(()=>expect(learningApi.unlinkProjectNote).toHaveBeenCalledWith("library","project","note-1"));expect(learningApi.linkProjectNote).not.toHaveBeenCalled();
});
it("searches existing notes and explicitly associates the selected identity",async()=>{
  render(<ProjectLearningNotes {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"关联现有笔记"}));
  fireEvent.change(screen.getByRole("searchbox",{name:"搜索笔记"}),{target:{value:"场景"}});
  fireEvent.click(await screen.findByRole("button",{name:"关联笔记：新的场景笔记"}));await waitFor(()=>expect(learningApi.linkProjectNote).toHaveBeenCalledWith("library","project","note-2"));
});
it("never shows a delayed result from another project",async()=>{
  let old!:(value:Awaited<ReturnType<typeof learningApi.listProjectNotes>>)=>void;
  vi.mocked(learningApi.listProjectNotes).mockImplementationOnce(()=>new Promise(resolve=>{old=resolve;}));
  const {rerender}=render(<ProjectLearningNotes {...props}/>);await waitFor(()=>expect(old).toBeDefined());
  vi.mocked(learningApi.listProjectNotes).mockResolvedValue({items:[{...linked,id:"new",title:"新项目资料"}],total:1,offset:0,limit:50});
  rerender(<ProjectLearningNotes {...props} projectId="other"/>);expect(await screen.findByText("新项目资料")).toBeVisible();await act(async()=>old({items:[linked],total:1,offset:0,limit:50}));expect(screen.queryByText("灯光学习")).toBeNull();
});
it("keeps archived project notes readable while preventing new links",async()=>{
  render(<ProjectLearningNotes {...props} archived/>);expect(await screen.findByRole("button",{name:"打开笔记：灯光学习"})).toBeEnabled();expect(screen.getByRole("button",{name:"关联现有笔记"})).toBeDisabled();
});
it("preserves visible notes when a reload fails and offers retry",async()=>{
  render(<ProjectLearningNotes {...props}/>);const region=await screen.findByRole("region",{name:"项目学习资料"});expect(within(region).getByText("灯光学习")).toBeVisible();
  vi.mocked(learningApi.listProjectNotes).mockRejectedValueOnce(new Error("读取失败"));fireEvent.click(within(region).getByRole("button",{name:"刷新学习资料"}));expect(await within(region).findByRole("alert")).toHaveTextContent("读取失败");expect(within(region).getByText("灯光学习")).toBeVisible();
});
it("returns to an existing page after unlinking the last note on a page",async()=>{
  const earlier=Array.from({length:50},(_,i)=>({...linked,id:`note-${i}`,title:`资料 ${i}`}));
  vi.mocked(learningApi.listProjectNotes).mockImplementation(async(_lib,_project,offset=0)=>offset===0?{items:earlier,total:51,offset:0,limit:50}:{items:[linked],total:51,offset:50,limit:50});
  render(<ProjectLearningNotes {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"下一页"}));await screen.findByRole("button",{name:"解除关联：灯光学习"});
  vi.mocked(learningApi.listProjectNotes).mockImplementation(async(_lib,_project,offset=0)=>offset===0?{items:earlier,total:50,offset:0,limit:50}:{items:[],total:50,offset:50,limit:50});
  fireEvent.click(screen.getByRole("button",{name:"解除关联：灯光学习"}));expect(await screen.findByText("资料 0")).toBeVisible();
});
