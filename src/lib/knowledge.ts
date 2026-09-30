import { invoke } from "@tauri-apps/api/core";
import type { JSONContent } from "@tiptap/react";

export interface KnowledgeNote {
  id: string; notebookId: string | null; title: string; document: JSONContent; plainText: string;
  tags: string[]; favorite: boolean; revision: number; journalDate: string | null;
  deletedAt: string | null; createdAt: string; updatedAt: string;
}
export type NoteInput = Pick<KnowledgeNote, "notebookId" | "title" | "document" | "tags" | "favorite" | "journalDate"> & { id: string | null; expectedRevision: number | null };
export interface KnowledgeNotebook { id: string; parentId: string | null; name: string; sortOrder: number }
export interface NoteRevision { id: string; noteId: string; snapshot: KnowledgeNote; pinned: boolean; createdAt: string }
export type NoteRevisionSummary = Omit<NoteRevision,"snapshot"> & {title:string};
export interface NoteSearch { query: string; notebookId: string | null; trash: boolean; favorite: boolean; offset: number; limit: number }
export type NoteSummary = Pick<KnowledgeNote,"id"|"title"|"plainText"|"tags"|"favorite"|"updatedAt">;
export interface KnowledgeReference {kind:string;targetId:string;label:string;available:boolean}

/** TypeScript structural types do not remove extra runtime fields from note snapshots. */
function writableNoteInput(input: NoteInput): NoteInput {
  const { id, expectedRevision, notebookId, title, document, tags, favorite, journalDate } = input;
  return { id, expectedRevision, notebookId, title, document, tags, favorite, journalDate };
}

/** A client is permanently bound to one library, including delayed autosaves. */
export function knowledgeApi(libraryId: string) {
  return {
    search: (request: NoteSearch) => invoke<{ items: NoteSummary[]; total: number }>("search_notes", { libraryId, ...request }),
    get: (id: string) => invoke<KnowledgeNote>("get_note", { libraryId, id }),
    journal: (date: string) => invoke<KnowledgeNote | null>("get_journal_note", {libraryId,date}),
    purge: (note: KnowledgeNote) => invoke<void>("purge_note", {libraryId,id:note.id,expectedRevision:note.revision}),
    save: (input: NoteInput) => invoke<KnowledgeNote>("save_note", { libraryId, input: writableNoteInput(input) }),
    setDeleted: (note: KnowledgeNote, deleted: boolean) => invoke<KnowledgeNote>("set_note_deleted", { libraryId, id: note.id, expectedRevision: note.revision, deleted }),
    notebooks: () => invoke<KnowledgeNotebook[]>("list_notebooks", { libraryId }),
    deleteNotebook: (id:string) => invoke<void>("delete_empty_notebook",{libraryId,id}),
    upsertNotebook: (name: string, parentId: string | null = null, id: string | null = null, sortOrder = 0) => invoke<KnowledgeNotebook>("upsert_notebook", { libraryId, name, parentId, id, sortOrder }),
    revisions: (id: string, offset=0) => invoke<NoteRevisionSummary[]>("list_note_revisions", { libraryId, id, offset }),
    revision: (id:string,revisionId:string)=>invoke<NoteRevision>("get_note_revision",{libraryId,id,revisionId}),
    pin: (id: string, pinned: boolean) => invoke<void>("pin_note_revision", { libraryId, id, pinned }),
    restore: (note: KnowledgeNote, revisionId: string) => invoke<KnowledgeNote>("restore_note_revision", { libraryId, id: note.id, expectedRevision: note.revision, revisionId }),
    importAttachment: (id: string, path: string) => invoke<{id:string;filename:string;mime:string}>("import_note_attachment", {libraryId,id,path}),
    pasteImage: (id: string) => invoke<{id:string;filename:string;mime:string}>("paste_note_clipboard_image", {libraryId,id}),
    exportPackage: (id: string, path: string) => invoke<void>("export_note_package", {libraryId,id,path}),
    exportDraft: (input:NoteInput,path:string)=>invoke<void>("export_note_draft",{libraryId,input: writableNoteInput(input),path}),
    importPackage: (path: string) => invoke<{note:KnowledgeNote;warnings:string[]}>("import_note_package", {libraryId,path}),
    readMarkdown: (path: string) => invoke<{text:string;title:string;sha256:string}>("read_note_markdown", {path}),
    importMarkdown: (path:string,sha256:string,document:JSONContent,notebookId:string|null) => invoke<{note:KnowledgeNote;warnings:string[]}>("import_note_markdown", {libraryId,path,sha256,document,notebookId}),
    exportMarkdown: (note:KnowledgeNote,markdown:string,path:string) => invoke<void>("export_note_markdown",{libraryId,id:note.id,revision:note.revision,markdown,path}),
    openAttachment: (id: string) => invoke<void>("open_note_attachment", {libraryId,id}),
    referenceTargets: (kind: string, query: string) => invoke<KnowledgeReference[]>("search_note_reference_targets", {libraryId,kind,query}),
    backlinks: (id: string) => invoke<NoteSummary[]>("get_note_backlinks", {libraryId,id}),
    references: (id: string) => invoke<Array<{kind:string;targetId:string;label:string;available:boolean}>>("get_note_references", {libraryId,id}),
  };
}

export const noteTemplates = {
  blank: { name: "空白笔记", headings: [] },
  tutorial: { name: "教程学习", headings: ["学习目标", "教程来源", "关键步骤", "练习与结果", "疑问与下一步"] },
  concept: { name: "技术知识点", headings: ["概念与用途", "适用场景", "实现方法", "注意事项", "相关资料"] },
  troubleshooting: { name: "报错解决", headings: ["问题现象", "环境与版本", "复现步骤", "原因分析", "解决方案", "验证结果"] },
  retrospective: { name: "项目复盘", headings: ["目标与结果", "制作过程", "做得好的地方", "问题与改进", "可复用经验"] },
  journal: { name: "每日学习", headings: ["今日目标", "学习记录", "实践成果", "待解决问题", "明日计划"] },
};
export type NoteTemplate = keyof typeof noteTemplates;
export function templateDocument(template: NoteTemplate): JSONContent {
  const content: JSONContent[] = noteTemplates[template].headings.flatMap(text => [
    { type: "heading", attrs: { level: 2 }, content: [{ type: "text", text }] }, { type: "paragraph" },
  ]);
  return { type: "doc", content: content.length ? content : [{ type: "paragraph" }] };
}

/** Serializes updates; a response can never erase edits made while its request was in flight. */
export class NoteSaveQueue {
  private current: KnowledgeNote;
  private pending: NoteInput | null = null;
  private running: Promise<void> | null = null;
  constructor(note: KnowledgeNote, private persist: (input: NoteInput) => Promise<KnowledgeNote>, private accepted: (note: KnowledgeNote) => void) { this.current = note; }
  enqueue(input: NoteInput) { this.pending = input; }
  isDirty() { return this.pending !== null || this.running !== null; }
  reset(note:KnowledgeNote) { if(this.running)throw new Error("请等待当前保存结束");this.pending=null;this.current=note; }
  flush(): Promise<void> {
    if (this.running) return this.running;
    const drain = async () => {
      while (this.pending) {
        const input = this.pending;
        this.pending = null;
        try {
          const saved = await this.persist({ ...input, id: this.current.id, expectedRevision: this.current.revision });
          this.current = saved;
          this.accepted(saved);
        } catch (error) {
          // Retain the newest unsaved draft, not the older failed request.
          this.pending ??= input;
          throw error;
        }
      }
    };
    this.running = drain().finally(() => { this.running = null; });
    return this.running;
  }
}
