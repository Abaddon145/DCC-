import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {afterEach,beforeAll,beforeEach,expect,it,vi} from "vitest";
import {StrictMode} from "react";
import LearningImportPanel from "./LearningImportPanel";
import {learningApi} from "./api";
import {api} from "../../lib/api";

vi.mock("./api",()=>({learningApi:{
  bindImportSource:vi.fn(),
  pickImportNote:vi.fn(),
  pickImportNotes:vi.fn(),
  previewImportNote:vi.fn(),
  queueImportPrepare:vi.fn(),
  queueImportPrepareBatch:vi.fn(),
  listImportBatches:vi.fn(),
  listImportItems:vi.fn(),
}}));
vi.mock("../../lib/api",()=>({api:{startLibraryTask:vi.fn(),getLibraryTask:vi.fn()}}));

const binding={
  profileId:"11111111-1111-4111-8111-111111111111",
  epoch:"22222222-2222-4222-8222-222222222222",
  vault:"vault-ui",
  root:"C:/synthetic/vault",
  libraryId:"lib",
  boundary:"仅绑定你明确选择的本地目录身份；不会自动读取真实映流清单路径，也不会覆盖已有笔记正文。",
};
const previewOk={
  profileId:binding.profileId,
  epoch:binding.epoch,
  vault:binding.vault,
  relativePath:"notes/hello.md",
  sourceSha256:"a".repeat(64),
  selectionId:"33333333-3333-4333-8333-333333333333",
  targetToken:"b".repeat(64),
  conflict:"unmapped",
  targetId:null,
  title:"迁入预检",
  pendingConversion:false,
  warnings:[],
  canPrepare:true,
  willOverwrite:false,
  message:"预检通过：将新建笔记，不会改动已有正文。",
};
const previewConflict={
  ...previewOk,
  conflict:"target_changed",
  targetId:"note-1",
  message:"存在冲突：可以暂存并提交冲突回执，但绝不会静默覆盖已有笔记正文。",
};
const batch={
  id:"33333333-3333-4333-8333-333333333333",
  status:"staged",
  requestSha256:"b".repeat(64),
  sourceProfileId:binding.profileId,
  sourceEpoch:binding.epoch,
  createdAt:"2026-10-28T00:00:00Z",
  finishedAt:null,
  receipt:null,
  currentSourceEpoch:binding.epoch,
  revertReport:null,
};

beforeAll(()=>{
  Object.defineProperties(HTMLDialogElement.prototype,{
    showModal:{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}},
    close:{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}},
  });
});

beforeEach(()=>{
  vi.clearAllMocks();
  vi.mocked(learningApi.listImportBatches).mockReset().mockResolvedValue([]);
  vi.mocked(learningApi.listImportItems).mockResolvedValue([]);
  vi.mocked(learningApi.bindImportSource).mockResolvedValue(structuredClone(binding));
  vi.mocked(learningApi.pickImportNote).mockResolvedValue(null);
  vi.mocked(learningApi.pickImportNotes).mockResolvedValue(null);
  vi.mocked(learningApi.queueImportPrepareBatch).mockResolvedValue({
    id:"task-batch",libraryId:"lib",label:"学习资料迁入准备",status:"queued",
    phase:"",current:0,total:0,currentName:"",createdAt:"",updatedAt:"",
    cancellable:true,message:"",result:null,payload:{kind:"learningImport",request:{action:"prepare"}},
  } as never);
  vi.mocked(learningApi.previewImportNote).mockResolvedValue(structuredClone(previewOk));
  vi.mocked(learningApi.queueImportPrepare).mockResolvedValue({
    id:"task-1",libraryId:"lib",label:"学习资料迁入准备",status:"queued",
    phase:"",current:0,total:0,currentName:"",createdAt:"",updatedAt:"",
    cancellable:true,message:"",result:null,payload:{kind:"learningImport",request:{action:"prepare"}},
  } as never);
});
afterEach(()=>vi.restoreAllMocks());

it("shows authorization boundary and does not bind without a native directory choice",async()=>{
  vi.mocked(learningApi.bindImportSource).mockResolvedValueOnce(null as never);
  render(<StrictMode><LearningImportPanel libraryId="lib" onClose={vi.fn()}/></StrictMode>);
  expect(await screen.findByText(/授权边界/)).toBeInTheDocument();
  expect(screen.getByText(/清单提示路径不构成授权|不会根据清单提示路径/)).toBeInTheDocument();
  expect(screen.getByText(/费用边界：.*不调用付费 AI/)).toBeInTheDocument();
  expect(screen.getByText(/尚未绑定来源/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await waitFor(()=>expect(learningApi.bindImportSource).toHaveBeenCalledWith("lib","vault-ui"));
  expect(await screen.findByText(/已取消目录选择/)).toBeInTheDocument();
});

it("binds a user-selected folder, previews conflict without overwrite, and queues prepare",async()=>{
  vi.mocked(learningApi.previewImportNote).mockResolvedValueOnce(structuredClone(previewConflict));
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  expect(await screen.findByText(/C:\/synthetic\/vault/)).toBeInTheDocument();
  expect(learningApi.bindImportSource).toHaveBeenCalledWith("lib","vault-ui");
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"notes/hello.md"}});
  fireEvent.click(screen.getByRole("button",{name:"预检相对路径"}));
  expect(await screen.findByText(/不会静默覆盖/)).toBeInTheDocument();
  expect(screen.getByText(/会覆盖正文：否/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"排队准备（不覆盖）"}));
  await waitFor(()=>expect(learningApi.queueImportPrepare).toHaveBeenCalledWith(
    "lib",binding.profileId,binding.epoch,binding.vault,binding.root,"notes/hello.md",previewOk.sourceSha256,previewOk.selectionId,previewOk.targetToken,
  ));
});

it("refuses prepare when conversion is pending and surfaces recovery for batches",async()=>{
  vi.mocked(learningApi.previewImportNote).mockResolvedValueOnce({
    ...previewOk,pendingConversion:true,canPrepare:false,message:"转换仍有未解析内容或缺少正文，已失败闭合，不能进入暂存。",
  });
  vi.mocked(learningApi.listImportBatches).mockResolvedValue([structuredClone(batch)]);
  vi.mocked(learningApi.listImportItems).mockResolvedValue([{
    id:"item-1",batchId:batch.id,status:"staged",conflictCode:null,targetId:null,
    sourceSha256:"a".repeat(64),stageRelative:"stage/item-1.md",
  }]);
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task-commit",status:"queued"} as never);
  vi.mocked(api.getLibraryTask).mockResolvedValue({id:"task-commit",status:"completed",message:"",result:{batchId:batch.id,status:"committed"}} as never);
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"notes/hello.md"}});
  fireEvent.click(screen.getByRole("button",{name:"预检相对路径"}));
  expect(await screen.findByText(/失败闭合/)).toBeInTheDocument();
  expect(screen.getByText(/不可排队，请按预检结果处理后重试/)).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"排队准备（不覆盖）"})).toBeDisabled();
  fireEvent.click(await screen.findByRole("button",{name:/staged/}));
  expect(await screen.findByText(/暂存 stage\/item-1.md/)).toBeInTheDocument();
  expect(screen.getByText(/可再次「提交批次」续跑/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"提交批次"}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith(
    {kind:"learningImport",request:{action:"commit",batchId:batch.id}},
    "lib",
  ));
});

it("refreshes batches on every click after a prepare task changes state",async()=>{
  vi.mocked(learningApi.listImportBatches)
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([{...batch,status:"pending"}])
    .mockResolvedValueOnce([{...batch,status:"committed"}]);
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  await waitFor(()=>expect(learningApi.listImportBatches).toHaveBeenCalledTimes(1));
  fireEvent.click(screen.getByRole("button",{name:"刷新批次"}));
  await screen.findByRole("button",{name:/pending/});
  fireEvent.click(screen.getByRole("button",{name:"刷新批次"}));
  expect(await screen.findByRole("button",{name:/committed/})).toBeInTheDocument();
  expect(learningApi.listImportBatches).toHaveBeenCalledTimes(3);
});

it("confirms a revert, queues it with the current epoch, and shows its durable report after reopening",async()=>{
  const report={batchId:batch.id,reverted:["note-created"],kept:[{itemId:"item-kept",outcome:"kept_modified",noteId:"note-edited"}],archivesKept:true};
  let savedReport:typeof report|null=null;
  vi.mocked(learningApi.listImportBatches).mockImplementation(async()=>[{...batch,status:"committed",revertReport:savedReport}]);
  vi.mocked(learningApi.listImportItems).mockResolvedValue([{id:"item",batchId:batch.id,status:"committed",conflictCode:null,targetId:"note-created",sourceSha256:"a".repeat(64),stageRelative:null}]);
  vi.mocked(api.startLibraryTask).mockImplementation(async()=>{savedReport=report;return {id:"revert-task",status:"queued"} as never;});
  vi.mocked(api.getLibraryTask).mockResolvedValue({id:"revert-task",status:"completed",result:{batchId:batch.id,status:"reverted",report}} as never);
  const confirm=vi.spyOn(window,"confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
  const view=render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/committed/}));
  await screen.findByText(/目标 note-cre/);
  fireEvent.click(screen.getByRole("button",{name:"撤回本批迁入"}));
  expect(api.startLibraryTask).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:"撤回本批迁入"}));
  await waitFor(()=>expect(api.startLibraryTask).toHaveBeenCalledWith({kind:"learningImport",request:{action:"revert",batchId:batch.id,expectedEpoch:binding.epoch}},"lib"));
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(await screen.findByText(/已移入回收站：1/)).toBeInTheDocument();
  expect(screen.getByText(/已修改或被使用/)).toBeInTheDocument();
  view.unmount();
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/committed/}));
  expect(await screen.findByText(/已移入回收站：1/)).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"撤回本批迁入"})).toBeDisabled();
});

it("does not enable revert using another batch's stale items while the selection loads",async()=>{
  const other={...batch,id:"44444444-4444-4444-8444-444444444444",status:"cancelled"};
  vi.mocked(learningApi.listImportBatches).mockResolvedValue([{...batch,status:"committed"},other]);
  let resolveOther:(items:Awaited<ReturnType<typeof learningApi.listImportItems>>)=>void=()=>{};
  vi.mocked(learningApi.listImportItems).mockImplementation(async(_library,id)=>id===batch.id
    ?[{id:"old",batchId:batch.id,status:"committed",conflictCode:null,targetId:"note",sourceSha256:"a".repeat(64),stageRelative:null}]
    :new Promise(resolve=>{resolveOther=resolve;}));
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/committed/}));
  await waitFor(()=>expect(screen.getByRole("button",{name:"撤回本批迁入"})).toBeEnabled());
  fireEvent.click(screen.getByRole("button",{name:/cancelled/}));
  expect(screen.getByRole("button",{name:"撤回本批迁入"})).toBeDisabled();
  await act(async()=>resolveOther([]));
  expect(screen.getByRole("button",{name:"撤回本批迁入"})).toBeDisabled();
});

it("allows reverting items already committed in a cancelled batch",async()=>{
  vi.mocked(learningApi.listImportBatches).mockResolvedValue([{...batch,status:"cancelled"}]);
  vi.mocked(learningApi.listImportItems).mockResolvedValue([{id:"item",batchId:batch.id,status:"committed",conflictCode:null,targetId:"note",sourceSha256:"a".repeat(64),stageRelative:null}]);
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/cancelled/}));
  await waitFor(()=>expect(screen.getByRole("button",{name:"撤回本批迁入"})).toBeEnabled());
});

it.each([
  ["提交批次", "cancelled", /批次已取消，未提交/],
  ["取消未完成项", "committed", /批次已提交，无法取消/],
])("reports the durable batch state when %s races with another action",async(button,status,message)=>{
  vi.mocked(learningApi.listImportBatches).mockResolvedValue([structuredClone(batch)]);
  vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task",status:"queued"} as never);
  vi.mocked(api.getLibraryTask).mockResolvedValue({id:"task",status:"completed",result:{batchId:batch.id,status}} as never);
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/staged/}));
  fireEvent.click(screen.getByRole("button",{name:button}));
  expect(await screen.findByText(message)).toBeInTheDocument();
});

it.each([null,{batchId:"other-batch",status:"committed"},{batchId:batch.id,status:"pending"}])(
  "does not claim success without a matching terminal batch result %#",async(result)=>{
    vi.mocked(learningApi.listImportBatches).mockResolvedValue([structuredClone(batch)]);
    vi.mocked(api.startLibraryTask).mockResolvedValue({id:"task",status:"queued"} as never);
    vi.mocked(api.getLibraryTask).mockResolvedValue({id:"task",status:"completed",result} as never);
    render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
    fireEvent.click(await screen.findByRole("button",{name:/staged/}));
    fireEvent.click(screen.getByRole("button",{name:"提交批次"}));
    expect(await screen.findByRole("alert")).toHaveTextContent(/无法确认批次结果/);
  },
);

it("shows bind failure and allows retry without opening a real vault path from the list",async()=>{
  vi.mocked(learningApi.bindImportSource).mockRejectedValueOnce(new Error("素材库已切换，请在当前库重新选择来源"));
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  expect(await screen.findByRole("alert")).toHaveTextContent(/素材库已切换/);
  fireEvent.click(screen.getByRole("button",{name:"清除并重试"}));
  expect(screen.queryByRole("alert")).toBeNull();
});

it("closes from native cancel",async()=>{
  const close=vi.fn();
  render(<LearningImportPanel libraryId="lib" onClose={close}/>);
  await screen.findByRole("dialog",{name:"学习资料迁入"});
  fireEvent(screen.getByRole("dialog",{name:"学习资料迁入"}),new Event("cancel",{cancelable:true}));
  expect(close).toHaveBeenCalledTimes(1);
});


it("clears the previous preview when the relative path changes",async()=>{
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"notes/hello.md"}});
  fireEvent.click(screen.getByRole("button",{name:"预检相对路径"}));
  await screen.findByText(/预检通过：将新建笔记/);
  expect(screen.getByRole("button",{name:"排队准备（不覆盖）"})).toBeEnabled();
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"different.md"}});
  expect(screen.getByRole("button",{name:"排队准备（不覆盖）"})).toBeDisabled();
});


it("selects a Markdown file through the backend and previews its result",async()=>{
  vi.mocked(learningApi.pickImportNote).mockResolvedValueOnce(structuredClone(previewOk));
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.click(screen.getByRole("button",{name:"选择 Markdown…"}));
  expect(await screen.findByText(/预检通过：将新建笔记/)).toBeInTheDocument();
  expect(learningApi.pickImportNote).toHaveBeenCalledWith("lib",binding.profileId,binding.epoch,binding.vault,binding.root);
  expect(screen.getByLabelText("迁入相对路径")).toHaveValue("notes/hello.md");
});


it("adds several authorized notes into one prepare batch and keeps a blocked note out",async()=>{
  const batchConflict={...previewConflict,relativePath:"notes/conflict.md",sourceSha256:"e".repeat(64),targetToken:"f".repeat(64)};
  const second={...previewOk,relativePath:"notes/second.md",title:"第二篇",sourceSha256:"c".repeat(64),targetToken:"d".repeat(64)};
  const blocked={...previewOk,relativePath:"notes/blocked.md",canPrepare:false,pendingConversion:true,message:"转换仍有未解析内容或缺少正文，已失败闭合，不能进入暂存。"};
  vi.mocked(learningApi.pickImportNotes).mockResolvedValueOnce([structuredClone(previewOk),structuredClone(batchConflict)]);
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.click(screen.getByRole("button",{name:"选择多篇 Markdown…"}));
  expect(await screen.findByRole("region",{name:"本批预检"})).toBeInTheDocument();
  expect(screen.getByText("notes/hello.md")).toBeInTheDocument();
  expect(screen.getByText("notes/conflict.md")).toBeInTheDocument();
  expect(screen.getAllByText(/覆盖：否/).length).toBeGreaterThan(0);
  expect(screen.getByText(/目标已改/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"排队准备（不覆盖）"}));
  await waitFor(()=>expect(learningApi.queueImportPrepareBatch).toHaveBeenCalledWith(
    "lib",binding.profileId,binding.epoch,binding.vault,binding.root,
    [
      {relativePath:"notes/conflict.md",sourceSha256:batchConflict.sourceSha256,targetToken:batchConflict.targetToken},
      {relativePath:"notes/hello.md",sourceSha256:previewOk.sourceSha256,targetToken:previewOk.targetToken},
    ],
    previewOk.selectionId,
  ));
  expect(learningApi.queueImportPrepare).not.toHaveBeenCalled();
  expect(await screen.findByText(/本批 2 篇/)).toBeInTheDocument();

  vi.mocked(learningApi.previewImportNote)
    .mockResolvedValueOnce(structuredClone(second))
    .mockResolvedValueOnce(structuredClone(blocked));
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"notes/second.md"}});
  fireEvent.click(screen.getByRole("button",{name:"预检相对路径"}));
  await screen.findByText(/预检通过：将新建笔记/);
  fireEvent.click(screen.getByRole("button",{name:"加入本批"}));
  fireEvent.change(screen.getByLabelText("迁入相对路径"),{target:{value:"notes/blocked.md"}});
  fireEvent.click(screen.getByRole("button",{name:"预检相对路径"}));
  expect(await screen.findByText(/失败闭合/)).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"加入本批"})).toBeDisabled();
  expect(screen.queryByText("notes/blocked.md")).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"移出 notes/second.md"}));
  expect(screen.queryByText("notes/second.md")).toBeNull();
});

it("shows a rejected multi-note pick without inventing paths",async()=>{
  vi.mocked(learningApi.pickImportNotes).mockRejectedValueOnce(new Error("请选择已授权来源目录中的笔记"));
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.click(screen.getByRole("button",{name:"选择多篇 Markdown…"}));
  expect(await screen.findByRole("alert")).toHaveTextContent(/已授权来源目录/);
  expect(screen.queryByRole("region",{name:"本批预检"})).toBeNull();
  expect(learningApi.queueImportPrepareBatch).not.toHaveBeenCalled();
});

it("keeps the selected vault but clears the preview when file picking is cancelled",async()=>{
  render(<LearningImportPanel libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(screen.getByRole("button",{name:"选择来源目录…"}));
  await screen.findByText(/C:\/synthetic\/vault/);
  fireEvent.click(screen.getByRole("button",{name:"选择 Markdown…"}));
  expect(await screen.findByText(/已取消笔记选择；来源目录仍保留/)).toBeInTheDocument();
  expect(screen.getByRole("button",{name:"排队准备（不覆盖）"})).toBeDisabled();
  expect(screen.getByRole("button",{name:"选择 Markdown…"})).toBeEnabled();
});
