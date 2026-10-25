import {beforeEach,describe,expect,it,vi} from "vitest";
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock("@tauri-apps/api/core",()=>mocks);
import {learningApi} from "./api";
import type {NotePreviewRequest} from "./types";
const library="596d257d-3e5c-46c4-a464-187cf180c6bd";
const previewId="5804245d-e1d3-4297-b08c-053a123706bd";
describe("library-scoped note preview API",()=>{
  beforeEach(()=>{mocks.invoke.mockReset();mocks.invoke.mockResolvedValue(undefined);});
  it("reads preserved bytes by library, immutable preview and placeholder IDs",async()=>{
    mocks.invoke.mockResolvedValue("<section>\r\nraw\r\n</section>\r\n");
    expect(await learningApi.readNotePreviewBlock(library,previewId,previewId)).toBe("<section>\r\nraw\r\n</section>\r\n");
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("read_note_conversion_block",{expectedLibraryId:library,previewId,placeholderId:previewId});
  });
  it("prepares from registered UUIDs without filesystem paths or frontend documents",async()=>{
    const request:NotePreviewRequest={archiveId:previewId,destination:{kind:"new",title:"学习",notebookId:null},attachments:[{sourceKey:"source.png",attachmentId:previewId}],courses:{}};
    await learningApi.prepareNotePreview(library,request);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("prepare_note_conversion_preview",{expectedLibraryId:library,request});
  });
  it("reads the specified immutable preview in its library",async()=>{
    await learningApi.getNotePreview(library,previewId);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_note_conversion_preview",{expectedLibraryId:library,previewId});
  });
  it("commits by preview ID without sending replacement content",async()=>{
    await learningApi.commitNotePreview(library,previewId);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("commit_note_preview",{expectedLibraryId:library,previewId});
  });
  it("cancels only the specified preview",async()=>{
    await learningApi.cancelNotePreview(library,previewId);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("cancel_note_conversion_preview",{expectedLibraryId:library,previewId});
  });
});
