import { describe, expect, it, vi } from "vitest";
import { knowledgeApi, NoteSaveQueue, templateDocument, noteTemplates, type KnowledgeNote, type NoteInput } from "./knowledge";
import { registerNoteSaveGuard, withKnowledgeBarrier, knowledgeChangesPaused } from "./noteSaveGuard";

const note: KnowledgeNote = { id: "note", notebookId: null, title: "学习", document: templateDocument("blank"), plainText: "", tags: [], favorite: false, revision: 1, journalDate: null, deletedAt: null, createdAt: "", updatedAt: "" };
const input = (title: string): NoteInput => ({ ...note, title, expectedRevision: 1 });
const mockedInvoke = vi.hoisted(()=>vi.fn().mockResolvedValue(null));
vi.mock("@tauri-apps/api/core",()=>({invoke:mockedInvoke}));
describe("knowledge workspace", () => {
  it("projects full runtime note snapshots into the strict save and draft export contract",async()=>{
    mockedInvoke.mockClear();const client=knowledgeApi("library");
    await client.save(input("编辑"));await client.exportDraft(input("未保存草稿"),"draft.dccnotes");
    for(const [command,args] of mockedInvoke.mock.calls){
      expect(["save_note","export_note_draft"]).toContain(command);
      expect(args.libraryId).toBe("library");
      expect(Object.keys(args.input).sort()).toEqual(["id","expectedRevision","notebookId","title","document","tags","favorite","journalDate"].sort());
      expect(args.input.expectedRevision).toBe(1);
    }
  });
  it("blocks maintenance when the draft cannot be saved and releases its editing barrier", async()=>{
    const remove=registerNoteSaveGuard(async()=>{throw new Error("保存失败");});const mutation=vi.fn();
    try {await expect(withKnowledgeBarrier(mutation)).rejects.toThrow("保存失败");expect(mutation).not.toHaveBeenCalled();expect(knowledgeChangesPaused()).toBe(false);}finally{remove();}
  });
  it("provides learning and CG templates plus a blank document", () => {
    expect(Object.keys(noteTemplates)).toHaveLength(7);
    expect(templateDocument("tutorial").content?.[0].type).toBe("heading");
    expect(templateDocument("blank").content).toEqual([{ type: "paragraph" }]);
  });
  it("serializes saves and carries the latest revision into later edits", async () => {
    let release!: (value: KnowledgeNote) => void;
    const persist = vi.fn<(value: NoteInput) => Promise<KnowledgeNote>>()
      .mockImplementationOnce(() => new Promise(resolve => { release = resolve; }))
      .mockResolvedValueOnce({ ...note, title: "最新", revision: 3 });
    const accepted = vi.fn();
    const queue = new NoteSaveQueue(note, persist, accepted);
    queue.enqueue(input("第一笔")); const saving = queue.flush();
    queue.enqueue(input("中间")); queue.enqueue(input("最新"));
    expect(queue.flush()).toBe(saving);
    release({ ...note, title: "第一笔", revision: 2 }); await saving;
    expect(persist).toHaveBeenCalledTimes(2);
    expect(persist.mock.calls[1][0]).toMatchObject({ title: "最新", expectedRevision: 2 });
    expect(queue.isDirty()).toBe(false);
  });
  it("retains a failed draft for explicit retry", async () => {
    const persist = vi.fn().mockRejectedValueOnce(new Error("磁盘已满")).mockResolvedValueOnce({ ...note, revision: 2 });
    const queue = new NoteSaveQueue(note, persist, vi.fn()); queue.enqueue(input("不可丢失"));
    await expect(queue.flush()).rejects.toThrow("磁盘已满"); expect(queue.isDirty()).toBe(true);
    await queue.flush(); expect(persist.mock.calls[1][0].title).toBe("不可丢失"); expect(queue.isDirty()).toBe(false);
  });
  it("resumes edits from a freshly loaded revision after preserving a conflicting draft",async()=>{
    const persist=vi.fn().mockRejectedValueOnce(new Error("版本冲突")).mockResolvedValueOnce({...note,revision:5});
    const queue=new NoteSaveQueue(note,persist,vi.fn());queue.enqueue(input("草稿"));await expect(queue.flush()).rejects.toThrow("版本冲突");
    queue.reset({...note,title:"另一窗口的修改",revision:4});expect(queue.isDirty()).toBe(false);
    queue.enqueue(input("合并后的内容"));await queue.flush();expect(persist.mock.calls[1][0].expectedRevision).toBe(4);
  });
});
