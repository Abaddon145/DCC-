import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { EditorContent, useEditor, type JSONContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { open } from "@tauri-apps/plugin-dialog";
import { ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode } from "../lib/noteExtensions";
import { TableKit } from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, ChevronRight, ChevronDown, FolderPlus, History, Plus, Save, Search, Star, Trash2, X } from "lucide-react";
import { api } from "../lib/api";
import { knowledgeApi, noteTemplates, NoteSaveQueue, templateDocument, type KnowledgeNote, type KnowledgeNotebook, type NoteInput, type NoteRevision, type NoteTemplate, type NoteSummary, type KnowledgeReference } from "../lib/knowledge";
import { registerNoteSaveGuard, knowledgeChangesPaused, subscribeKnowledgeBarrier } from "../lib/noteSaveGuard";
import "./knowledge.css";
import { NoteAiDialog } from "./NoteAiDialog";
import { NoteReferencePicker } from "./NoteReferencePicker";

interface Props { initialNoteId?: string | null; notify: (message: string, error?: boolean) => void }
export default function KnowledgeWorkspace({ notify, initialNoteId }: Props) {
  const [libraryId, setLibraryId] = useState("");
  useEffect(() => { let active = true; void api.getWorkspaceContext("knowledge").then(v => { if (active) setLibraryId(v.libraryId); }).catch(e => notify(String(e), true)); return () => { active = false; }; }, [notify]);
  return libraryId ? <Workspace key={libraryId} libraryId={libraryId} initialNoteId={initialNoteId} notify={notify} /> : <div className="knowledge-loading">正在打开知识库…</div>;
}

function Workspace({ libraryId, notify, initialNoteId }: Props & { libraryId: string }) {
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [notebooks, setNotebooks] = useState<KnowledgeNotebook[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const [query, setQuery] = useState(""); const [favorite, setFavorite] = useState(false); const [trash, setTrash] = useState(false);
  const [page, setPage] = useState(0); const [total, setTotal] = useState(0); const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [current, setCurrent] = useState<KnowledgeNote | null>(null); const [reload, setReload] = useState(0);
  const [template, setTemplate] = useState<NoteTemplate>("blank");
  const [folderForm, setFolderForm] = useState<{ parent: string | null; name: string } | null>(null);
  const [busy, setBusy] = useState(false); const [loading, setLoading] = useState(false);
  const flush = useRef<() => Promise<void>>(async () => {});
  const run = (action: () => Promise<void>) => { setBusy(true); void action().catch(e => notify(String(e), true)).finally(() => setBusy(false)); };
  useEffect(() => { void client.notebooks().then(setNotebooks).catch(e => notify(String(e), true)); }, [client, reload, notify]);
  useEffect(() => {
    let active = true; setLoading(true);
    const timer = setTimeout(() => { void client.search({ query, notebookId: folder, trash, favorite, offset: page * 40, limit: 40 }).then(v => { if (active) { setNotes(v.items); setTotal(v.total); } }).catch(e => { if (active) notify(String(e), true); }).finally(() => { if (active) setLoading(false); }); }, 180);
    return () => { active = false; clearTimeout(timer); };
  }, [client, query, folder, trash, favorite, page, reload, notify]);
  const choose = (note: NoteSummary) => run(async () => { await flush.current(); setCurrent(await client.get(note.id)); });
  useEffect(()=>{if(!initialNoteId)return;let active=true;void client.get(initialNoteId).then(v=>{if(active)setCurrent(v);}).catch(e=>notify(String(e),true));return()=>{active=false;};},[client,initialNoteId,notify]);
  const create = () => run(async () => {
    await flush.current();
    const date = new Date(); const journalDate = template === "journal" ? `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,"0")}-${String(date.getDate()).padStart(2,"0")}` : null;
    if (journalDate) { const existing=await client.journal(journalDate); if(existing) {setCurrent(existing);setTrash(!!existing.deletedAt);return;} }
    const note = await client.save({ id: null, expectedRevision: null, notebookId: folder, title: journalDate ? `${journalDate} 学习日志` : "未命名笔记", document: templateDocument(template), tags: [], favorite: false, journalDate });
    setCurrent(note); setTrash(false); setReload(v => v + 1);
  });
  const tree = (parent: string | null, depth = 0): React.ReactNode => notebooks.filter(n => n.parentId === parent).map(n => <div key={n.id}>
    <div className={`knowledge-folder ${folder === n.id ? "active" : ""}`} style={{ paddingLeft: 8 + depth * 14 }}>
      <button aria-label={`${collapsed.has(n.id) ? "展开" : "收起"}${n.name}`} onClick={() => setCollapsed(v => { const next = new Set(v); if (next.has(n.id)) next.delete(n.id); else next.add(n.id); return next; })}>{collapsed.has(n.id) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</button>
      <button className="knowledge-folder-name" onClick={() => { setFolder(n.id); setTrash(false); setPage(0); }}>{n.name}</button>
      <button title="新建子笔记本" onClick={() => setFolderForm({ parent: n.id, name: "" })}><Plus size={14} /></button>
    </div>{!collapsed.has(n.id) && tree(n.id, depth + 1)}</div>);
  return <div className="knowledge-workspace">
    <aside className="knowledge-navigation"><header><BookOpen size={19}/><strong>知识库</strong></header>
      <button className={!folder && !favorite && !trash ? "active" : ""} onClick={() => { setFolder(null); setFavorite(false); setTrash(false); setPage(0); }}>全部笔记</button>
      <button className={favorite ? "active" : ""} onClick={() => { setFolder(null); setFavorite(true); setTrash(false); setPage(0); }}><Star size={15}/>收藏笔记</button>
      <div className="knowledge-section"><span>笔记本</span><button title="新建笔记本" onClick={() => setFolderForm({ parent: null, name: "" })}><FolderPlus size={16}/></button></div>{tree(null)}
      <button className={`knowledge-trash ${trash ? "active" : ""}`} onClick={() => { setFolder(null); setFavorite(false); setTrash(true); setPage(0); }}><Trash2 size={15}/>笔记回收站</button>
    </aside>
    <section className="knowledge-list"><header><span className="eyebrow">KNOWLEDGE WORKSPACE</span><h2>{trash ? "笔记回收站" : "学习与创作"}</h2><p>{total} 篇笔记 · 仅保存在当前素材库</p></header>
      <div className="knowledge-search"><Search size={15}/><input aria-label="搜索笔记" placeholder="搜索标题、正文、标签" value={query} onChange={e => { setQuery(e.target.value); setPage(0); }}/></div>
      {!trash && <div className="knowledge-create"><select aria-label="笔记模板" value={template} onChange={e => setTemplate(e.target.value as NoteTemplate)}>{Object.entries(noteTemplates).map(([id,v]) => <option key={id} value={id}>{v.name}</option>)}</select><button className="primary-button" disabled={busy} onClick={create}><Plus size={15}/>新建</button></div>}
      <div className="knowledge-note-list" aria-busy={loading}>{notes.map(note => <button key={note.id} disabled={busy} className={`knowledge-note-card ${current?.id === note.id ? "active" : ""}`} onClick={() => choose(note)}><strong>{note.favorite && "★ "}{note.title}</strong><span>{note.plainText.trim().slice(0,110) || "暂无正文"}</span><small>{new Date(note.updatedAt).toLocaleDateString()} {note.tags.slice(0,3).join(" · ")}</small></button>)}{!loading && !notes.length && <p className="knowledge-empty">{trash ? "没有已删除的笔记" : "记录一个知识点，让下次创作少走弯路。"}</p>}</div>
      <footer><button disabled={!page || loading} onClick={() => setPage(v => v - 1)}>上一页</button><span>{page + 1} / {Math.max(1,Math.ceil(total/40))}</span><button disabled={(page+1)*40 >= total || loading} onClick={() => setPage(v => v + 1)}>下一页</button></footer>
    </section>
    {current ? <NoteEditor key={`${current.id}-${current.revision}-${current.deletedAt || "active"}`} libraryId={libraryId} note={current} notebooks={notebooks} notify={notify} register={handler => { flush.current = handler; }} onNavigateNote={id=>run(async()=>{await flush.current();setCurrent(await client.get(id));})} onSaved={() => setReload(v => v+1)} onDeleted={note => { setCurrent(note); setReload(v=>v+1); }} /> : <section className="knowledge-editor-empty"><BookOpen size={42}/><h2>把学习变成可复用的经验</h2><p>从左侧选择笔记，或使用模板开始记录。</p><p>支持 Markdown 快捷输入、表格、清单与代码块。</p></section>}
    {folderForm && <div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-modal="true" aria-label="新建笔记本" onSubmit={e => { e.preventDefault(); run(async () => { await client.upsertNotebook(folderForm.name, folderForm.parent); setFolderForm(null); setReload(v => v + 1); }); }}><header><h2>{folderForm.parent ? "新建子笔记本" : "新建笔记本"}</h2><button type="button" onClick={() => setFolderForm(null)}><X size={18}/></button></header><input aria-label="笔记本名称" autoFocus maxLength={100} value={folderForm.name} onChange={e => setFolderForm({ ...folderForm, name: e.target.value })}/><footer><button type="button" onClick={() => setFolderForm(null)}>取消</button><button className="primary-button" disabled={busy || !folderForm.name.trim()}>创建</button></footer></form></div>}
  </div>;
}

function NoteEditor({ libraryId, note, notebooks, notify, register, onSaved, onDeleted, onNavigateNote }: Props & { libraryId: string; note: KnowledgeNote; notebooks: KnowledgeNotebook[]; register: (flush: () => Promise<void>) => void; onSaved: () => void; onDeleted: (note: KnowledgeNote | null) => void; onNavigateNote: (id: string) => void }) {
  const paused = useSyncExternalStore(subscribeKnowledgeBarrier,knowledgeChangesPaused);
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [showReferences,setShowReferences]=useState(false);
  const [ai,setAi]=useState<{note:KnowledgeNote;selection:string}|null>(null);
  const [backlinks,setBacklinks]=useState<KnowledgeNote[]>([]);
  const [references,setReferences]=useState<KnowledgeReference[]>([]);
  useEffect(()=>{let active=true;void client.references(note.id).then(v=>{if(active)setReferences(v);}).catch(()=>{});return()=>{active=false;};},[client,note.id,note.revision]);
  useEffect(()=>{let active=true;void client.backlinks(note.id).then(v=>{if(active)setBacklinks(v);}).catch(()=>{});return()=>{active=false;};},[client,note.id]);
  const [draft, setDraft] = useState<NoteInput>({ ...note, expectedRevision: note.revision });
  const draftRef = useRef(draft); const savedRef = useRef(note); const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [status, setStatus] = useState("已保存"); const [error, setError] = useState(""); const [ready, setReady] = useState(false);
  const [history, setHistory] = useState<NoteRevision[] | null>(null); const [preview, setPreview] = useState<NoteRevision | null>(null);
  const onSavedRef = useRef(onSaved); onSavedRef.current = onSaved;
  const queue = useMemo(() => new NoteSaveQueue(note, client.save, value => { savedRef.current = value; onSavedRef.current(); }), [client, note.id]);
  const flush = useCallback(async () => {
    if (timer.current) clearTimeout(timer.current);
    if (!queue.isDirty()) return;
    setStatus("保存中…");
    try { await queue.flush(); setStatus("已保存"); setError(""); } catch (e) { setStatus("保存失败"); setError(String(e)); throw e; }
  }, [queue]);
  const change = useCallback((patch: Partial<NoteInput>) => {
    if(knowledgeChangesPaused())return;
    const value = { ...draftRef.current, ...patch }; draftRef.current = value; setDraft(value); queue.enqueue(value); setStatus("待保存");
    if (timer.current) clearTimeout(timer.current); timer.current = setTimeout(() => { void flush().catch(() => {}); }, 600);
  }, [queue, flush]);
  const editor = useEditor({ extensions: [StarterKit.configure({ link: { openOnClick: false, protocols: ["https","http"] } }), TableKit, TaskList, TaskItem.configure({ nested: true }), ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode], content: note.document, editable: false, onUpdate: ({ editor }) => change({ document: editor.getJSON() }), editorProps: { attributes: { class: "knowledge-prose", "aria-label": "笔记正文" }, handleDOMEvents: { click: (_view,event) => { const ref=(event.target as Element).closest("[data-note-reference]");if(ref?.getAttribute("data-reference-kind")==="note"){event.preventDefault();onNavigateNote(ref.getAttribute("data-note-reference")||"");return true;}const attachment=(event.target as Element).closest("[data-note-attachment]")?.getAttribute("data-note-attachment"); if(attachment){event.preventDefault();void client.openAttachment(attachment).catch(e=>notify(String(e),true));return true;}const link=(event.target as Element).closest("a"); if(link) {event.preventDefault();const href=link.getAttribute("href");if(href&&/^https?:\/\//i.test(href)) void api.openExternal(href).catch(e=>notify(String(e),true));return true;}return false; } } } });
  useEffect(() => { register(flush); return registerNoteSaveGuard(flush); }, [flush, register]);
  useEffect(() => {
    let active = true;
    const unlisten = listen("knowledge-close-requested", () => { void flush().then(() => invoke("finish_knowledge_close")).catch(e => notify(String(e),true)); });
    void invoke("set_knowledge_editing", { editing: true }).then(() => { if(active) setReady(true); }).catch(e=>notify(String(e),true));
    return () => { active=false; if(timer.current) clearTimeout(timer.current); void unlisten.then(v=>v()); void invoke("set_knowledge_editing", { editing: false }); };
  }, [flush,notify]);
  useEffect(() => { editor?.setEditable(ready && !paused && !note.deletedAt && !preview, false); return subscribeKnowledgeBarrier(()=>editor?.setEditable(ready && !knowledgeChangesPaused() && !note.deletedAt && !preview,false)); }, [editor,ready,paused,note.deletedAt,preview]);
  const action = (fn: () => Promise<void>) => { void fn().catch(e => notify(String(e),true)); };
  const display = (revision: NoteRevision | null) => { setPreview(revision); editor?.commands.setContent(revision ? revision.snapshot.document : draftRef.current.document, { emitUpdate:false }); };
  return <section className="knowledge-editor" onKeyDown={e => { if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==="s") {e.preventDefault(); e.stopPropagation(); action(flush);} }}>
    <header className="knowledge-editor-heading"><span className={error ? "knowledge-save-error" : "knowledge-save-status"} role="status">{status}</span><div><button disabled={!ready || paused || !!note.deletedAt} onClick={()=>change({favorite:!draft.favorite})} title="收藏笔记"><Star size={16} fill={draft.favorite ? "currentColor" : "none"}/></button><button onClick={()=>action(async()=>{await flush();setHistory(await client.revisions(note.id));})}><History size={16}/>历史</button><button disabled={!ready} onClick={()=>action(flush)}><Save size={16}/>保存</button><button onClick={()=>action(async()=>{await flush();if(note.deletedAt || window.confirm("将笔记移入回收站？历史版本和附件会保留。")) onDeleted(await client.setDeleted(savedRef.current,!note.deletedAt));})}>{note.deletedAt ? "恢复笔记" : <Trash2 size={16}/>}</button>{note.deletedAt && <button onClick={()=>action(async()=>{if(window.confirm("永久删除这篇笔记、全部历史版本和附件？此操作无法撤销。")){await client.purge(savedRef.current);onDeleted(null);}})}>永久删除</button>}</div></header>
    {paused&&<p className="knowledge-history-banner">正在安全保存、备份或切换素材库，编辑暂时锁定。</p>}
    {error && <div className="knowledge-save-error" role="alert">{error}<button onClick={()=>action(flush)}>重试保存</button></div>}
    {preview && <div className="knowledge-history-banner">正在预览历史版本 · {new Date(preview.createdAt).toLocaleString()}<button onClick={()=>display(null)}>返回当前版本</button><button onClick={()=>action(async()=>{const restored=await client.restore(savedRef.current,preview.id);onDeleted(restored);setPreview(null);setHistory(null);})}>恢复此版本</button></div>}
    <div className="knowledge-document"><input className="knowledge-title" aria-label="笔记标题" disabled={!ready || paused || !!note.deletedAt || !!preview} value={preview?.snapshot.title ?? draft.title} onChange={e=>change({title:e.target.value})}/>
      <div className="knowledge-metadata"><select aria-label="所属笔记本" value={draft.notebookId || ""} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>change({notebookId:e.target.value||null})}><option value="">未分笔记本</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select><input aria-label="笔记标签" placeholder="标签，用分号分隔" value={draft.tags.join("; ")} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>change({tags:e.target.value.split(";")})}/></div>
      {ready && !paused && !note.deletedAt && !preview && <div className="knowledge-toolbar" aria-label="笔记格式"><button onClick={()=>editor?.chain().focus().toggleBold().run()}>粗体</button><button onClick={()=>editor?.chain().focus().toggleItalic().run()}>斜体</button><button onClick={()=>editor?.chain().focus().toggleHeading({level:2}).run()}>标题</button><button onClick={()=>editor?.chain().focus().toggleBulletList().run()}>列表</button><button onClick={()=>editor?.chain().focus().toggleTaskList().run()}>待办</button><button onClick={()=>editor?.chain().focus().toggleCodeBlock().run()}>代码</button><button onClick={()=>editor?.chain().focus().insertTable({rows:3,cols:3,withHeaderRow:true}).run()}>表格</button><button onClick={()=>action(async()=>{const paths=await open({multiple:true,filters:[{name:"笔记附件",extensions:["png","jpg","jpeg","webp","gif","pdf","txt","md"]}]});if(!paths)return;await flush();for(const path of paths){const item=await client.importAttachment(note.id,path);if(item.mime.startsWith("image/"))editor?.chain().focus().setImage({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename}).run();else editor?.chain().focus().insertContent({type:"attachment",attrs:{attachmentId:item.id,label:item.filename}}).run();}})}>图片 / 附件</button><button onClick={()=>setShowReferences(true)}>插入引用</button><button onClick={()=>action(async()=>{const selection=editor ? editor.state.doc.textBetween(editor.state.selection.from,editor.state.selection.to,"\n") : "";await flush();setAi({note:await client.get(note.id),selection});})}>AI 辅助</button></div>}
      <EditorContent editor={editor}/>
      {references.some(v=>!v.available)&&<section className="knowledge-backlinks"><h3>来源暂不可用</h3>{references.filter(v=>!v.available).map(v=><p key={`${v.kind}-${v.targetId}`}>{v.label} · 已删除或不在当前素材库，引用保留</p>)}</section>}
      {backlinks.length>0&&<section className="knowledge-backlinks"><h3>哪些笔记引用了这里</h3>{backlinks.map(v=><button key={v.id} onClick={()=>onNavigateNote(v.id)}>{v.title}</button>)}</section>}
    </div>
    {ai&&<NoteAiDialog libraryId={libraryId} note={ai.note} selection={ai.selection} onClose={()=>setAi(null)} onNavigate={onNavigateNote} onApply={text=>{editor?.chain().focus().insertContentAt(editor.state.doc.content.size,{type:"paragraph",content:[{type:"text",text}]}).run();setAi(null);}}/>}
    {showReferences&&<NoteReferencePicker libraryId={libraryId} onClose={()=>setShowReferences(false)} onPick={v=>{editor?.chain().focus().insertContent({type:"reference",attrs:{kind:v.kind,targetId:v.targetId,label:v.label}}).run();setShowReferences(false);}}/>}
    {history && <aside className="knowledge-history"><header><strong>历史版本</strong><button onClick={()=>{display(null);setHistory(null);}}><X size={16}/></button></header><p>最近 50 个未固定版本；短时间编辑合并记录。</p>{history.length===0&&<p>首次修改后会保留历史版本。</p>}{history.map(v=><div key={v.id}><button onClick={()=>display(v)}>{v.snapshot.title}<small>{new Date(v.createdAt).toLocaleString()}</small></button><button title={v.pinned ? "取消固定" : "固定版本"} onClick={()=>action(async()=>{await client.pin(v.id,!v.pinned);setHistory(await client.revisions(note.id));})}>{v.pinned ? "已固定" : "固定"}</button></div>)}</aside>}
  </section>;
}
