import {fireEvent,render,screen,waitFor,within} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {ExcerptComposer} from "./ExcerptComposer";
import {learningApi} from "./api";
import {api} from "../../lib/api";
import type {ExcerptOperation,StoredExcerpt} from "./types";
import type {LibraryTask} from "../../types";
import type {KnowledgeNote} from "../../lib/knowledge";
const knowledge=vi.hoisted(()=>({notebooks:vi.fn(),search:vi.fn(),get:vi.fn()}));
vi.mock("./api",()=>({learningApi:{startExcerptTask:vi.fn(),getExcerpt:vi.fn(),getExcerptOperation:vi.fn(),listExcerptOperations:vi.fn(),excerptTargets:vi.fn(),cancelExcerptPreview:vi.fn(),cancelExcerptOperation:vi.fn()}}));
vi.mock("../../lib/api",()=>({api:{getLibraryTask:vi.fn(),listLibraryTasks:vi.fn(),cancelLibraryTask:vi.fn()}}));
vi.mock("../../lib/knowledge",()=>({knowledgeApi:()=>knowledge}));
const selection={subtitleId:"subtitle",cueIds:["cue"],ranges:[],comment:"学习备注",frameTimeMs:null};
const props={libraryId:"lib",courseId:"course",selection,onClose:vi.fn(),onOpen:vi.fn()};
const preview:StoredExcerpt={id:"preview",status:"ready",prepared:{libraryId:"lib",rootHash:"root",sourceFileId:"file",sourceSha256:"sha",durationMs:10000,text:{courseId:"course",mediaId:"media",title:"课程标题",subtitleId:"subtitle",subtitleSha256:"sha",ranges:[{startMs:1000,endMs:2000},{startMs:5000,endMs:6000}],cues:[{id:"cue",subtitleId:"subtitle",startMs:1000,endMs:2000,text:"完整字幕 <script>不作为HTML执行</script>"}],comment:"学习备注",frameTimeMs:null},document:{type:"doc",content:[]},frame:null,warnings:["未请求截图"]}};
function task(payload:LibraryTask["payload"],status:LibraryTask["status"]="completed"):LibraryTask{return {id:"task",libraryId:"lib",label:"学习摘录",status,phase:"完成",current:1,total:1,currentName:"",createdAt:"",updatedAt:"",cancellable:true,message:"",result:payload.kind==="courseExcerpt"&&payload.request.action==="prepare"?{previewId:"preview"}:null,payload};}
const operation:ExcerptOperation={operationId:"operation",excerptId:"preview",status:"committed",request:{operationId:"operation",excerptId:"preview",includeFrame:false,destination:{kind:"new",title:"摘录",notebookId:null}},receipt:{operationId:"operation",excerptId:"preview",noteId:"note",revision:1,warnings:[]},createdAt:"",previewReadable:false,warning:"来源预览不可读取，历史回执仍保留"};
beforeEach(()=>{vi.clearAllMocks();knowledge.notebooks.mockResolvedValue([]);knowledge.search.mockResolvedValue({items:[],total:0});vi.mocked(learningApi.listExcerptOperations).mockResolvedValue({items:[],total:0,offset:0,limit:20});vi.mocked(learningApi.excerptTargets).mockResolvedValue({projects:[],boards:[]});vi.mocked(api.listLibraryTasks).mockResolvedValue([]);vi.mocked(learningApi.getExcerpt).mockResolvedValue(preview);vi.mocked(learningApi.getExcerptOperation).mockImplementation(async(_lib,id)=>{const call=vi.mocked(learningApi.startExcerptTask).mock.calls.at(-1)?.[1];const request=call?.action==="commit"?call.request:operation.request;return {...operation,operationId:id,excerptId:request.excerptId,request,receipt:{...operation.receipt!,operationId:id,excerptId:request.excerptId}};});vi.mocked(learningApi.startExcerptTask).mockImplementation(async(_lib,request)=>task({kind:"courseExcerpt",request}));});
async function prepare(){fireEvent.click(await screen.findByRole("button",{name:"准备摘录预览"}));await screen.findByText("完整字幕 <script>不作为HTML执行</script>");}
it("prepares only on request and confirms a full inert preview before saving",async()=>{
  render(<ExcerptComposer {...props}/>);
  await screen.findByRole("button",{name:"准备摘录预览"});expect(learningApi.startExcerptTask).not.toHaveBeenCalled();
  await prepare();expect(learningApi.startExcerptTask).toHaveBeenCalledWith("lib",{action:"prepare",courseId:"course",selection});
  expect(screen.getByRole("region",{name:"摘录预览"}).querySelector("script")).toBeNull();expect(screen.getAllByText(/00:01/).length).toBeGreaterThan(0);
  expect(learningApi.startExcerptTask).toHaveBeenCalledTimes(1);expect(screen.getByRole("option",{name:"参考板"})).toBeDisabled();
  fireEvent.click(screen.getByRole("button",{name:"确认保存摘录"}));await screen.findByText("摘录已保存");
  const request=vi.mocked(learningApi.startExcerptTask).mock.calls[1][1];expect(request).toMatchObject({action:"commit",request:{excerptId:"preview",includeFrame:false,destination:{kind:"new",title:"课程标题 · 学习摘录",notebookId:null}}});
});
it("freezes the exact operation across uncertain failure and manual retry",async()=>{
  render(<ExcerptComposer {...props}/>);await prepare();vi.mocked(learningApi.startExcerptTask).mockRejectedValueOnce(new Error("回执连接中断"));
  fireEvent.click(screen.getByRole("button",{name:"确认保存摘录"}));await screen.findByText(/回执连接中断/);
  const first=vi.mocked(learningApi.startExcerptTask).mock.calls[1][1];expect(screen.queryByRole("combobox",{name:"保存到"})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"重试原确认"}));await screen.findByText("摘录已保存");expect(vi.mocked(learningApi.startExcerptTask).mock.calls[2][1]).toEqual(first);
});
it("opens a historical receipt even when its preview cannot be read",async()=>{
  vi.mocked(learningApi.listExcerptOperations).mockResolvedValue({items:[{operationId:"operation",excerptId:"preview",status:"committed",title:"历史摘录",receipt:operation.receipt,createdAt:"",error:null}],total:1,offset:0,limit:20});
  render(<ExcerptComposer {...props}/>);fireEvent.click(screen.getByText("摘录记录与恢复"));fireEvent.click(await screen.findByRole("button",{name:"读取记录：历史摘录"}));await screen.findByText("摘录已保存");
  expect(screen.getByRole("status")).toHaveTextContent("历史回执仍保留");expect(learningApi.getExcerpt).not.toHaveBeenCalled();fireEvent.click(screen.getByRole("button",{name:"打开保存位置"}));expect(props.onOpen).toHaveBeenCalledWith(operation.receipt);
});
it("rejects a foreign preview and restores the invoking focus on dismissal",async()=>{
  const trigger=document.createElement("button");document.body.append(trigger);trigger.focus();vi.mocked(learningApi.getExcerpt).mockResolvedValue({...preview,prepared:{...preview.prepared,libraryId:"other"}});
  const mounted=render(<ExcerptComposer {...props}/>);fireEvent.click(await screen.findByRole("button",{name:"准备摘录预览"}));await screen.findByText(/预览不属于当前课程或素材库/);expect(screen.queryByRole("button",{name:"确认保存摘录"})).toBeNull();
  fireEvent.keyDown(screen.getByRole("dialog"),{key:"Escape"});expect(props.onClose).toHaveBeenCalled();mounted.unmount();expect(trigger).toHaveFocus();trigger.remove();
});
it("appends only at the chosen block and expected revision without sending note content",async()=>{
  const note:KnowledgeNote={id:"note",notebookId:null,title:"已有笔记",document:{type:"doc",content:[{type:"paragraph"},{type:"paragraph"}]},plainText:"原文",tags:[],favorite:false,revision:7,journalDate:null,deletedAt:null,createdAt:"",updatedAt:""};
  knowledge.search.mockResolvedValue({items:[note],total:1});knowledge.get.mockResolvedValue(note);
  render(<ExcerptComposer {...props}/>);await prepare();fireEvent.change(screen.getByRole("combobox",{name:"保存到"}),{target:{value:"append"}});
  await waitFor(()=>expect(within(screen.getByRole("combobox",{name:"目标笔记"})).getByRole("option",{name:"已有笔记"})).toBeInTheDocument());fireEvent.change(screen.getByRole("combobox",{name:"目标笔记"}),{target:{value:"note"}});
  const position=await screen.findByRole("spinbutton");fireEvent.change(position,{target:{value:"1"}});fireEvent.click(screen.getByRole("button",{name:"确认保存摘录"}));await screen.findByText("摘录已保存");
  expect(vi.mocked(learningApi.startExcerptTask).mock.calls[1][1]).toMatchObject({action:"commit",request:{destination:{kind:"append",noteId:"note",expectedRevision:7,insertIndex:1}}});
});
it("keeps a recovered project confirmation readable with its real project name",async()=>{
  vi.mocked(learningApi.excerptTargets).mockResolvedValue({projects:[{id:"project-id",name:"沙漠场景",projectType:"scene",status:"active",description:"",targetTools:[],versions:[],resolutionWidth:1920,resolutionHeight:1080,frameRate:24,assetCount:0,unavailableAssetCount:0,taskCount:0,completedTaskCount:0,reviewTaskCount:0,progress:0,updatedAt:"",createdAt:"",lastOpenedAt:null,archivedAt:null,coverAssetId:null,coverMediaEntryId:null,coverImageId:null,coverMediaFileId:null,mainBoardId:null}],boards:[]});
  vi.mocked(learningApi.getExcerptOperation).mockResolvedValue({...operation,status:"pending",request:{...operation.request,destination:{kind:"project",projectId:"project-id",title:"场景学习",notebookId:null}},receipt:null,previewReadable:true,warning:null});
  vi.mocked(learningApi.listExcerptOperations).mockResolvedValue({items:[{operationId:"operation",excerptId:"preview",status:"pending",title:"场景学习",receipt:null,createdAt:"",error:null}],total:1,offset:0,limit:20});
  render(<ExcerptComposer {...props}/>);fireEvent.click(screen.getByText("摘录记录与恢复"));fireEvent.click(await screen.findByRole("button",{name:"读取记录：场景学习"}));
  expect(await screen.findByText("项目：沙漠场景 · 场景学习")).toBeInTheDocument();
  expect(screen.queryByRole("combobox",{name:"保存到"})).not.toBeInTheDocument();
});
it("shows exact millisecond boundaries for precise source verification",async()=>{
  vi.mocked(learningApi.getExcerpt).mockResolvedValue({...preview,prepared:{...preview.prepared,text:{...preview.prepared.text,ranges:[{startMs:1125,endMs:2875}]}}});
  render(<ExcerptComposer {...props}/>);await prepare();expect(screen.getByText("00:01.125—00:02.875")).toBeInTheDocument();
});
it("keeps IME composition open and cycles dialog focus with Tab",async()=>{
  render(<ExcerptComposer {...props}/>);const modal=screen.getByRole("dialog");fireEvent.keyDown(modal,{key:"Tab",shiftKey:true});expect(screen.getByRole("button",{name:"收起"})).toHaveFocus();
  fireEvent.keyDown(screen.getByRole("button",{name:"收起"}),{key:"Tab"});expect(screen.getByRole("button",{name:"关闭摘录窗口"})).toHaveFocus();
  fireEvent.compositionStart(modal);fireEvent.keyDown(modal,{key:"Escape"});expect(props.onClose).not.toHaveBeenCalled();fireEvent.compositionEnd(modal);fireEvent.keyDown(modal,{key:"Escape"});expect(props.onClose).toHaveBeenCalledTimes(1);
});
