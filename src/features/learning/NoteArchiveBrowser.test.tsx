import {act,fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {StrictMode} from "react";
import NoteArchiveBrowser from "./NoteArchiveBrowser";
import {learningApi} from "./api";
import {save} from "@tauri-apps/plugin-dialog";
import type {NoteArchivePage,NoteArchiveView} from "./types";
vi.mock("./api",()=>({learningApi:{listNoteArchives:vi.fn(),readNoteArchive:vi.fn(),exportNoteArchive:vi.fn()}}));
vi.mock("@tauri-apps/plugin-dialog",()=>({save:vi.fn()}));
const page:NoteArchivePage={versions:{items:[{id:"raw",profileId:"profile",sourceKey:"lesson.md",versionKey:"history-1",sha256:"f".repeat(64),size:30,createdAt:"2026-10-25T00:00:00Z",conversionState:"partial"}],total:51,offset:0,limit:50},unconvertedCount:2,partialCount:1};
const view:NoteArchiveView={archiveId:"raw",sha256:"f".repeat(64),size:30,text:"<script>window.pwned=true</script>\r\n"};
beforeAll(()=>{
  Object.defineProperties(HTMLDialogElement.prototype,{showModal:{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}},close:{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}}});
});
beforeEach(()=>{vi.clearAllMocks();vi.mocked(learningApi.listNoteArchives).mockResolvedValue(structuredClone(page));vi.mocked(learningApi.readNoteArchive).mockResolvedValue(structuredClone(view));vi.mocked(learningApi.exportNoteArchive).mockResolvedValue();vi.mocked(save).mockResolvedValue(null);});
it("loads bounded version metadata in StrictMode and reads only the selected original as inert text",async()=>{
  render(<StrictMode><NoteArchiveBrowser libraryId="lib" onClose={vi.fn()}/></StrictMode>);
  expect(await screen.findByText(/2 个版本未完整转换/)).toBeInTheDocument();
  expect(learningApi.readNoteArchive).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button",{name:/lesson.md/}));
  // Native textarea normalizes line endings; export must still read original backend bytes.
  expect(await screen.findByRole("textbox",{name:"归档原文（只读）"})).toHaveValue("<script>window.pwned=true</script>\n");
  expect(document.querySelector("script")).toBeNull();
  expect(learningApi.readNoteArchive).toHaveBeenLastCalledWith("lib","raw");
  fireEvent.click(screen.getByRole("button",{name:"下一页"}));
  await waitFor(()=>expect(learningApi.listNoteArchives).toHaveBeenLastCalledWith("lib",50,50));
});
it("shows a list read failure and supports retry without creating a note",async()=>{
  vi.mocked(learningApi.listNoteArchives).mockRejectedValueOnce(new Error("cannot read"));
  render(<NoteArchiveBrowser libraryId="lib" onClose={vi.fn()}/>);
  expect(await screen.findByRole("alert")).toHaveTextContent("cannot read");
  fireEvent.click(screen.getByRole("button",{name:"重新读取"}));
  expect(await screen.findByRole("button",{name:/lesson.md/})).toBeEnabled();
  expect(learningApi.readNoteArchive).not.toHaveBeenCalled();
});
it("exports the registered original by ID and does nothing when the save dialog is cancelled",async()=>{
  render(<NoteArchiveBrowser libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/lesson.md/}));
  const button=await screen.findByRole("button",{name:"导出原字节"});
  await waitFor(()=>expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(()=>expect(save).toHaveBeenCalledTimes(1));
  expect(learningApi.exportNoteArchive).not.toHaveBeenCalled();
  vi.mocked(save).mockResolvedValueOnce("C:/exports/original.md");
  await waitFor(()=>expect(button).toBeEnabled());
  fireEvent.click(button);
  await waitFor(()=>expect(learningApi.exportNoteArchive).toHaveBeenCalledExactlyOnceWith("lib","raw","C:/exports/original.md"));
  expect(await screen.findByText("原字节已导出，归档保持不变。")).toBeInTheDocument();
});
it("keeps invalid encodings as bytes and retries a failed original read",async()=>{
  vi.mocked(learningApi.readNoteArchive).mockRejectedValueOnce(new Error("changed")).mockResolvedValueOnce({...view,text:null});
  render(<NoteArchiveBrowser libraryId="lib" onClose={vi.fn()}/>);
  fireEvent.click(await screen.findByRole("button",{name:/lesson.md/}));
  expect(await screen.findByRole("alert")).toHaveTextContent("changed");
  fireEvent.click(screen.getByRole("button",{name:"重试读取原件"}));
  expect(await screen.findByText(/不是有效 UTF-8/)).toBeInTheDocument();
  expect(screen.queryByRole("textbox",{name:"归档原文（只读）"})).toBeNull();
});
it("ignores late content from the old library and closes with native cancel or an outside click",async()=>{
  let resolve!:(value:NoteArchiveView)=>void;
  vi.mocked(learningApi.readNoteArchive).mockReturnValueOnce(new Promise(done=>{resolve=done;}));
  const close=vi.fn();
  const {rerender}=render(<NoteArchiveBrowser libraryId="old" onClose={close}/>);
  fireEvent.click(await screen.findByRole("button",{name:/lesson.md/}));
  rerender(<NoteArchiveBrowser libraryId="new" onClose={close}/>);
  await screen.findByRole("button",{name:/lesson.md/});
  await act(async()=>resolve({...view,text:"OLD PRIVATE CONTENT"}));
  expect(screen.queryByDisplayValue("OLD PRIVATE CONTENT")).toBeNull();
  const dialog=screen.getByRole("dialog",{name:"原文归档"});
  fireEvent(dialog,new Event("cancel",{cancelable:true}));
  expect(close).toHaveBeenCalledTimes(1);
  fireEvent.pointerDown(dialog,{clientX:-1,clientY:-1});
  expect(close).toHaveBeenCalledTimes(2);
});
