import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { EditorContent, useEditor, type JSONContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { open, save } from "@tauri-apps/plugin-dialog";
import { parseNoteMarkdown, serializeNoteMarkdown } from "../lib/noteMarkdown";
import { ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode } from "../lib/noteExtensions";
import { TableKit } from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, ChevronRight, ChevronDown, FolderPlus, History, Plus, Save, Search, Star, Trash2, X } from "lucide-react";
import { api } from "../lib/api";
import { knowledgeApi, noteTemplates, NoteSaveQueue, templateDocument, type KnowledgeNote, type KnowledgeNotebook, type NoteInput, type NoteRevision, type NoteRevisionSummary, type NoteTemplate, type NoteSummary, type KnowledgeReference } from "../lib/knowledge";
import { registerNoteSaveGuard, knowledgeChangesPaused, subscribeKnowledgeBarrier, withKnowledgeBarrier } from "../lib/noteSaveGuard";
import "./knowledge.css";
import { NoteAiDialog } from "./NoteAiDialog";
import { NoteSourcePreview } from "./NoteSourcePreview";
import { NoteReferencePicker } from "./NoteReferencePicker";
import { KnowledgeMenu, type KnowledgeMenuState } from "./KnowledgeMenu";
import { NoteEditorTools } from "./NoteEditorTools";

interface Props { onOpenProject?:(id:string)=>void; initialNoteId?: string | null; notify: (message: string, error?: boolean) => void }
export default function KnowledgeWorkspace({ notify, initialNoteId,onOpenProject }: Props) {
  const [libraryId, setLibraryId] = useState("");
  const [openError, setOpenError] = useState("");
  const [openAttempt, setOpenAttempt] = useState(0);
  useEffect(() => { let active = true; setOpenError(""); void api.getWorkspaceContext("knowledge").then(v => { if (active) setLibraryId(v.libraryId); }).catch(e => { if(active){setOpenError(String(e));notify(String(e), true);} }); return () => { active = false; }; }, [notify, openAttempt]);
  return libraryId ? <Workspace key={libraryId} libraryId={libraryId} initialNoteId={initialNoteId} onOpenProject={onOpenProject} notify={notify} /> : openError ? <div className="knowledge-loading"><p role="alert">知识库打开失败：{openError}</p><button className="secondary-button" onClick={()=>setOpenAttempt(v=>v+1)}>重新打开知识库</button></div> : <div className="knowledge-loading">正在打开知识库…</div>;
}

function Workspace({ libraryId, notify, initialNoteId,onOpenProject }: Props & { libraryId: string }) {
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [notebooks, setNotebooks] = useState<KnowledgeNotebook[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const [query, setQuery] = useState(""); const [favorite, setFavorite] = useState(false); const [trash, setTrash] = useState(false);
  const [page, setPage] = useState(0); const [total, setTotal] = useState(0); const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [current, setCurrent] = useState<KnowledgeNote | null>(null); const [reload, setReload] = useState(0);
  const [template, setTemplate] = useState<NoteTemplate>("blank");
  const [folderForm, setFolderForm] = useState<{ id?:string; order?:number; parent: string | null; name: string } | null>(null);
  const [busy, setBusy] = useState(false); const [loading, setLoading] = useState(false);
  const [menu,setMenu]=useState<KnowledgeMenuState|null>(null);
  const [noteForm,setNoteForm]=useState<{note:KnowledgeNote;mode:"rename"|"move";title:string;folder:string|null}|null>(null);
  const [undoDeleted,setUndoDeleted]=useState<KnowledgeNote|null>(null);
  const flush = useRef<() => Promise<void>>(async () => {});
  const run = (action: () => Promise<void>) => { setBusy(true); return action().catch(e => notify(String(e), true)).finally(() => setBusy(false)); };
  useEffect(() => { void client.notebooks().then(setNotebooks).catch(e => notify(String(e), true)); }, [client, reload, notify]);
  useEffect(() => {
    let active = true; setLoading(true);
    const timer = setTimeout(() => { void client.search({ query, notebookId: folder, trash, favorite, offset: page * 40, limit: 40 }).then(v => { if (active) { setNotes(v.items); setTotal(v.total); } }).catch(e => { if (active) notify(String(e), true); }).finally(() => { if (active) setLoading(false); }); }, 180);
    return () => { active = false; clearTimeout(timer); };
  }, [client, query, folder, trash, favorite, page, reload, notify]);
  const choose = (note: NoteSummary) => run(async () => { await flush.current(); setCurrent(await client.get(note.id)); });
  useEffect(()=>{if(!initialNoteId)return;let active=true;void flush.current().then(()=>client.get(initialNoteId)).then(v=>{if(active){setCurrent(v);setTrash(!!v.deletedAt);}}).catch(e=>notify(String(e),true));return()=>{active=false;};},[client,initialNoteId,notify]);
  const create = (target=folder, kind=template) => run(async () => {
    await flush.current();
    const date = new Date(); const journalDate = kind === "journal" ? `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,"0")}-${String(date.getDate()).padStart(2,"0")}` : null;
    if (journalDate) { const existing=await client.journal(journalDate); if(existing) {setCurrent(existing);setTrash(!!existing.deletedAt);return;} }
    const note = await client.save({ id: null, expectedRevision: null, notebookId: target, title: journalDate ? `${journalDate} 学习日志` : "未命名笔记", document: templateDocument(kind), tags: [], favorite: false, journalDate });
    setCurrent(note); setTrash(false); setReload(v => v + 1);
  });
  const targetAction=(id:string,work:(note:KnowledgeNote)=>Promise<void>)=>run(async()=>{await flush.current();await work(await client.get(id));setReload(v=>v+1);});
  const changed=(value:KnowledgeNote)=>{if(current?.id===value.id)setCurrent(value);};
  const removeNote=(id:string)=>targetAction(id,async value=>{if(!window.confirm(`将“${value.title}”移入回收站？历史与附件会保留。`))return;const deleted=await client.setDeleted(value,true);setUndoDeleted(deleted);if(current?.id===id)setCurrent(null);notify("笔记已移入回收站，可撤销");});
  const exportTarget=(id:string)=>targetAction(id,async value=>{const path=await save({defaultPath:`${value.title.replace(/[<>:"/\\|?*]/g,"_")}.dccnotes`,filters:[{name:"当前笔记与附件",extensions:["dccnotes"]}]});if(path)await withKnowledgeBarrier(()=>client.exportPackage(id,path));});
  const noteMenu=(event:React.MouseEvent,note:NoteSummary)=>{event.preventDefault();event.stopPropagation();setMenu({x:event.clientX,y:event.clientY,actions:trash?[
    {label:"打开",run:()=>choose(note)},{label:"恢复笔记",run:()=>targetAction(note.id,async v=>{changed(await client.setDeleted(v,false));})},
    {label:"永久删除…",danger:true,run:()=>targetAction(note.id,async v=>{if(!window.confirm(`永久删除“${v.title}”、历史和附件？无法撤销。`))return;await client.purge(v);if(current?.id===v.id)setCurrent(null);if(undoDeleted?.id===v.id)setUndoDeleted(null);})}
  ]:[
    {label:"打开",run:()=>choose(note)},{label:"新建笔记",run:()=>targetAction(note.id,async v=>{await create(v.notebookId,"blank");})},
    {label:"重命名…",run:()=>targetAction(note.id,async v=>setNoteForm({note:v,mode:"rename",title:v.title,folder:v.notebookId}))},
    {label:"移动到笔记本…",run:()=>targetAction(note.id,async v=>setNoteForm({note:v,mode:"move",title:v.title,folder:v.notebookId}))},
    {label:note.favorite?"取消收藏":"收藏",run:()=>targetAction(note.id,async v=>changed(await client.save({...v,expectedRevision:v.revision,favorite:!v.favorite})))},
    {label:"复制笔记",run:()=>targetAction(note.id,async v=>{setCurrent(await client.duplicate(v));setTrash(false);})},
    {label:"导出笔记包…",run:()=>exportTarget(note.id)},
    {label:"历史版本",run:()=>targetAction(note.id,async v=>{setCurrent(v);setHistoryTarget(v.id);})},
    {label:"移入回收站…",danger:true,run:()=>removeNote(note.id)}
  ]});};
  const [historyTarget,setHistoryTarget]=useState<string|null>(null);
  const folderMenu=(event:React.MouseEvent,n:KnowledgeNotebook)=>{event.preventDefault();event.stopPropagation();setMenu({x:event.clientX,y:event.clientY,actions:[
    {label:"新建笔记",run:()=>create(n.id,"blank")},{label:"新建子笔记本",run:()=>setFolderForm({parent:n.id,name:""})},
    {label:"重命名 / 移动…",run:()=>setFolderForm({id:n.id,parent:n.parentId,name:n.name,order:n.sortOrder})},
    {label:"删除空笔记本…",danger:true,run:()=>run(async()=>{await flush.current();if(!window.confirm(`删除空笔记本“${n.name}”？`))return;await client.deleteNotebook(n.id);if(folder===n.id)setFolder(n.parentId);setReload(v=>v+1);})}
  ]});};
  const tree = (parent: string | null, depth = 0): React.ReactNode => notebooks.filter(n => n.parentId === parent).map(n => <div key={n.id}>
    <div onContextMenu={e=>folderMenu(e,n)} className={`knowledge-folder ${folder === n.id ? "active" : ""}`} style={{ paddingLeft: 8 + depth * 14 }}>
      <button aria-label={`${collapsed.has(n.id) ? "展开" : "收起"}${n.name}`} onClick={() => setCollapsed(v => { const next = new Set(v); if (next.has(n.id)) next.delete(n.id); else next.add(n.id); return next; })}>{collapsed.has(n.id) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</button>
      <button className="knowledge-folder-name" onClick={() => { setFolder(n.id); setFavorite(false); setTrash(false); setPage(0); }}>{n.name}</button>
      <button title="管理笔记本" onClick={()=>setFolderForm({id:n.id,order:n.sortOrder,parent:n.parentId,name:n.name})}>⋯</button><button title="新建子笔记本" onClick={() => setFolderForm({ parent: n.id, name: "" })}><Plus size={14} /></button>
    </div>{!collapsed.has(n.id) && tree(n.id, depth + 1)}</div>);
  return <div className="knowledge-workspace">
    {menu&&<KnowledgeMenu menu={menu} onClose={()=>setMenu(null)}/>}
    {undoDeleted&&<div className="knowledge-undo" role="status">已删除“{undoDeleted.title}”<button disabled={busy} onClick={()=>run(async()=>{const latest=await client.get(undoDeleted.id);if(!latest.deletedAt){setUndoDeleted(null);return;}const restored=await client.setDeleted(latest,false);setUndoDeleted(null);setCurrent(restored);setTrash(false);setReload(v=>v+1);})}>撤销删除</button><button aria-label="关闭撤销提示" onClick={()=>setUndoDeleted(null)}>×</button></div>}
    {noteForm&&<div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-label={noteForm.mode==="rename"?"重命名笔记":"移动笔记"} onSubmit={e=>{e.preventDefault();run(async()=>{const value=await client.save({...noteForm.note,title:noteForm.title,notebookId:noteForm.folder,expectedRevision:noteForm.note.revision});changed(value);setNoteForm(null);setReload(v=>v+1);});}}><h2>{noteForm.mode==="rename"?"重命名笔记":"移动笔记"}</h2>{noteForm.mode==="rename"?<input aria-label="新笔记名称" autoFocus maxLength={300} value={noteForm.title} onChange={e=>setNoteForm({...noteForm,title:e.target.value})}/>:<select aria-label="目标笔记本" value={noteForm.folder||""} onChange={e=>setNoteForm({...noteForm,folder:e.target.value||null})}><option value="">未分笔记本</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select>}<footer><button type="button" onClick={()=>setNoteForm(null)}>取消</button><button disabled={busy||!noteForm.title.trim()}>保存</button></footer></form></div>}
    <aside className="knowledge-navigation"><header><BookOpen size={19}/><strong>知识库</strong></header>
      <button className={!folder && !favorite && !trash ? "active" : ""} onClick={() => { setFolder(null); setFavorite(false); setTrash(false); setPage(0); }}>全部笔记</button>
      <button className={favorite ? "active" : ""} onClick={() => { setFolder(null); setFavorite(true); setTrash(false); setPage(0); }}><Star size={15}/>收藏笔记</button>
      <div className="knowledge-section"><span>笔记本</span><button title="新建笔记本" onClick={() => setFolderForm({ parent: null, name: "" })}><FolderPlus size={16}/></button></div>{tree(null)}
      <button className={`knowledge-trash ${trash ? "active" : ""}`} onClick={() => { setFolder(null); setFavorite(false); setTrash(true); setPage(0); }}><Trash2 size={15}/>笔记回收站</button>
    </aside>
    <section className="knowledge-list"><header><span className="eyebrow">KNOWLEDGE WORKSPACE</span><h2>{trash ? "笔记回收站" : "学习与创作"}</h2><p>{total} 篇笔记 · 仅保存在当前素材库</p></header>
      <div className="knowledge-search"><Search size={15}/><input aria-label="搜索笔记" placeholder="搜索标题、正文、标签" value={query} onChange={e => { setQuery(e.target.value); setPage(0); }}/></div>
      {!trash && <div className="knowledge-create"><select aria-label="笔记模板" value={template} onChange={e => setTemplate(e.target.value as NoteTemplate)}>{Object.entries(noteTemplates).map(([id,v]) => <option key={id} value={id}>{v.name}</option>)}</select><button className="primary-button" disabled={busy} onClick={()=>create()}><Plus size={15}/>新建</button><button disabled={busy} title="导入 Markdown 或笔记包" onClick={()=>run(async()=>{const path=await open({multiple:false,filters:[{name:"笔记",extensions:["md","dccnotes"]}]});if(!path)return;const result=await withKnowledgeBarrier(async()=>{if(path.toLowerCase().endsWith(".dccnotes"))return client.importPackage(path);const source=await client.readMarkdown(path);return client.importMarkdown(path,source.sha256,parseNoteMarkdown(source.text),folder);});setCurrent(result.note);setReload(v=>v+1);notify(result.warnings.length?result.warnings.join("；"):"笔记已导入");})}>导入</button></div>}
      <div className="knowledge-note-list" aria-busy={loading} onContextMenu={e=>{if((e.target as Element).closest(".knowledge-note-card")||trash)return;e.preventDefault();setMenu({x:e.clientX,y:e.clientY,actions:[{label:"新建笔记",run:()=>create(folder,"blank")}]});}}>{notes.map(note => <button key={note.id} disabled={busy} className={`knowledge-note-card ${current?.id === note.id ? "active" : ""}`} onContextMenu={e=>noteMenu(e,note)} onClick={() => choose(note)}><strong>{note.favorite && "★ "}{note.title}</strong><span>{note.plainText.trim().slice(0,110) || "暂无正文"}</span><small>{new Date(note.updatedAt).toLocaleDateString()} {note.tags.slice(0,3).join(" · ")}</small></button>)}{!loading && !notes.length && <p className="knowledge-empty">{trash ? "没有已删除的笔记" : "记录一个知识点，让下次创作少走弯路。"}</p>}</div>
      <footer><button disabled={!page || loading} onClick={() => setPage(v => v - 1)}>上一页</button><span>{page + 1} / {Math.max(1,Math.ceil(total/40))}</span><button disabled={(page+1)*40 >= total || loading} onClick={() => setPage(v => v + 1)}>下一页</button></footer>
    </section>
    {current ? <NoteEditor key={`${current.id}-${current.revision}-${current.deletedAt || "active"}`} libraryId={libraryId} historyRequested={historyTarget===current.id} onHistoryConsumed={()=>setHistoryTarget(null)} onOpenProject={onOpenProject} note={current} notebooks={notebooks} notify={notify} register={handler => { flush.current = handler; }} onNavigateNote={id=>run(async()=>{await flush.current();setCurrent(await client.get(id));})} onSaved={() => setReload(v => v+1)} onDeleted={note => { if(note?.deletedAt)setUndoDeleted(note);setCurrent(note); setReload(v=>v+1); }} /> : <section className="knowledge-editor-empty"><BookOpen size={42}/><h2>把学习变成可复用的经验</h2><p>从左侧选择笔记，或使用模板开始记录。</p><p>支持 Markdown 快捷输入、表格、清单与代码块。</p></section>}
    {folderForm && <div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-modal="true" aria-label={folderForm.id?"管理笔记本":"新建笔记本"} onSubmit={e => { e.preventDefault(); run(async () => { await client.upsertNotebook(folderForm.name, folderForm.parent,folderForm.id||null,folderForm.order??notebooks.filter(n=>n.parentId===folderForm.parent).length); setFolderForm(null); setReload(v => v + 1); }); }}><header><h2>{folderForm.id?"管理笔记本":folderForm.parent ? "新建子笔记本" : "新建笔记本"}</h2><button type="button" onClick={() => setFolderForm(null)}><X size={18}/></button></header><select aria-label="上级笔记本" value={folderForm.parent||""} onChange={e=>setFolderForm({...folderForm,parent:e.target.value||null})}><option value="">根级</option>{notebooks.filter(n=>n.id!==folderForm.id).map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select><input aria-label="笔记本名称" autoFocus maxLength={100} value={folderForm.name} onChange={e => setFolderForm({ ...folderForm, name: e.target.value })}/><footer>{folderForm.id&&<button type="button" disabled={busy} onClick={()=>run(async()=>{if(!window.confirm("删除这个空笔记本？含笔记或子笔记本时将拒绝删除。"))return;await client.deleteNotebook(folderForm.id!);if(folder===folderForm.id)setFolder(folderForm.parent);setFolderForm(null);setReload(v=>v+1);})}>删除空笔记本</button>}<button type="button" onClick={() => setFolderForm(null)}>取消</button><button className="primary-button" disabled={busy || !folderForm.name.trim()}>{folderForm.id?"保存":"创建"}</button></footer></form></div>}
  </div>;
}

function NoteEditor({ libraryId, note, notebooks, notify, register, onSaved, onDeleted, onNavigateNote,onOpenProject,historyRequested,onHistoryConsumed }: Props & { historyRequested:boolean;onHistoryConsumed:()=>void;libraryId: string; note: KnowledgeNote; notebooks: KnowledgeNotebook[]; register: (flush: () => Promise<void>) => void; onSaved: () => void; onDeleted: (note: KnowledgeNote | null) => void; onNavigateNote: (id: string) => void }) {
  const paused = useSyncExternalStore(subscribeKnowledgeBarrier,knowledgeChangesPaused);
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [savedCount,setSavedCount]=useState(0);
  const [source,setSource]=useState<{kind:string;id:string}|null>(null);
  const [showReferences,setShowReferences]=useState(false);
  const [ai,setAi]=useState<{note:KnowledgeNote;selection:string}|null>(null);
  const [backlinks,setBacklinks]=useState<NoteSummary[]>([]);
  const [references,setReferences]=useState<KnowledgeReference[]>([]);
  useEffect(()=>{let active=true;void client.references(note.id).then(v=>{if(active)setReferences(v);}).catch(()=>{});return()=>{active=false;};},[client,note.id,note.revision,savedCount]);
  useEffect(()=>{let active=true;void client.backlinks(note.id).then(v=>{if(active)setBacklinks(v);}).catch(()=>{});return()=>{active=false;};},[client,note.id]);
  const [tagsText,setTagsText]=useState(note.tags.join("; "));
  const [draft, setDraft] = useState<NoteInput>({ ...note, expectedRevision: note.revision });
  const imports=useRef(new Set<Promise<unknown>>());
  const trackImport=useCallback(<T,>(work:Promise<T>):Promise<T>=>{imports.current.add(work);void work.then(()=>imports.current.delete(work),()=>imports.current.delete(work));return work;},[]);
  const draftRef = useRef(draft); const savedRef = useRef(note); const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [status, setStatus] = useState("已保存"); const [error, setError] = useState(""); const [ready, setReady] = useState(false);
  const [draftExported,setDraftExported]=useState(false);
  const [history, setHistory] = useState<NoteRevisionSummary[] | null>(null); const [preview, setPreview] = useState<NoteRevision | null>(null);
  const historyPanel=useRef<HTMLElement>(null);const historyButton=useRef<HTMLButtonElement>(null);const historyGeneration=useRef(0);
  const [historyBusy,setHistoryBusy]=useState(false);
  const onSavedRef = useRef(onSaved); onSavedRef.current = onSaved;
  const queue = useMemo(() => new NoteSaveQueue(note, client.save, value => { savedRef.current = value; setSavedCount(count=>count+1);onSavedRef.current(); }), [client, note.id]);
  const flush = useCallback(async () => {
    if (timer.current) clearTimeout(timer.current);
    await Promise.all([...imports.current]);
    if (!queue.isDirty()) return;
    setStatus("保存中…");
    try { await queue.flush(); setStatus("已保存"); setError(""); } catch (e) { setStatus("保存失败"); setError(String(e)); throw e; }
  }, [queue]);
  const change = useCallback((patch: Partial<NoteInput>) => {
    if(knowledgeChangesPaused() && imports.current.size===0)return;
    const value = { ...draftRef.current, ...patch }; draftRef.current = value; setDraft(value); queue.enqueue(value);setDraftExported(false); setStatus("待保存");
    if (timer.current) clearTimeout(timer.current); timer.current = setTimeout(() => { void flush().catch(() => {}); }, 600);
  }, [queue, flush]);
  const editor = useEditor({ extensions: [StarterKit.configure({ link: { openOnClick: false, protocols: ["https","http"] } }), TableKit, TaskList, TaskItem.configure({ nested: true }), ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode], content: note.document, editable: false, onUpdate: ({ editor }) => change({ document: editor.getJSON() }), editorProps: { handlePaste: (view,event)=>{if(!Array.from(event.clipboardData?.items||[]).some(item=>item.type.startsWith("image/")))return false;if(knowledgeChangesPaused()||!view.editable)return true;void trackImport(client.pasteImage(note.id).then(item=>{if(view.isDestroyed)return;const node=view.state.schema.nodes.image.create({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename});view.dispatch(view.state.tr.replaceSelectionWith(node));})).catch(e=>notify(String(e),true));return true;}, attributes: { class: "knowledge-prose", "aria-label": "笔记正文", role:"textbox", "aria-multiline":"true" }, handleDOMEvents: { click: (_view,event) => { const ref=(event.target as Element).closest("[data-note-reference]");if(ref?.getAttribute("data-reference-kind")==="note"){event.preventDefault();onNavigateNote(ref.getAttribute("data-note-reference")||"");return true;}if(ref){event.preventDefault();setSource({kind:ref.getAttribute("data-reference-kind")||"",id:ref.getAttribute("data-note-reference")||""});return true;}const attachment=(event.target as Element).closest("[data-note-attachment]")?.getAttribute("data-note-attachment"); if(attachment){event.preventDefault();void client.openAttachment(attachment).catch(e=>notify(String(e),true));return true;}const link=(event.target as Element).closest("a"); if(link) {event.preventDefault();const href=link.getAttribute("href");if(href&&/^https?:\/\//i.test(href)) void api.openExternal(href).catch(e=>notify(String(e),true));return true;}return false; } } } });
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
  const closeHistory=()=>{historyGeneration.current++;display(null);setHistory(null);setHistoryBusy(false);};
  const openHistory=async()=>{const generation=++historyGeneration.current;setHistory([]);setHistoryBusy(true);try{await flush();const items=await client.revisions(note.id);if(generation===historyGeneration.current)setHistory(items);}finally{if(generation===historyGeneration.current)setHistoryBusy(false);}};
  useEffect(()=>{if(historyRequested){onHistoryConsumed();action(openHistory);}},[historyRequested]);
  useEffect(()=>{
    if(!history)return;
    const outside=(event:PointerEvent)=>{const target=event.target as Node;if(!historyPanel.current?.contains(target)&&!historyButton.current?.contains(target)&&!(event.target as Element).closest?.(".modal-backdrop,.knowledge-context-menu"))closeHistory();};
    const escape=(event:KeyboardEvent)=>{if(event.key==="Escape"&&!document.querySelector(".modal-backdrop,.knowledge-context-menu")){event.preventDefault();closeHistory();}};
    document.addEventListener("pointerdown",outside,true);document.addEventListener("keydown",escape);
    return()=>{document.removeEventListener("pointerdown",outside,true);document.removeEventListener("keydown",escape);};
  },[history,editor]);
  const toolbarAction=(label:string)=>{const buttons=editor?.view.dom.closest(".knowledge-editor")?.querySelectorAll<HTMLButtonElement>(".knowledge-toolbar button");const button=Array.from(buttons||[]).find(v=>v.textContent===label);button?.click();};
  return <section className="knowledge-editor" onKeyDown={e => { if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==="s") {e.preventDefault(); e.stopPropagation(); action(flush);} }}>
    <header className="knowledge-editor-heading"><span className={error ? "knowledge-save-error" : "knowledge-save-status"} role="status">{status}</span><div><button disabled={!ready || paused || !!note.deletedAt} onClick={()=>change({favorite:!draft.favorite})} title="收藏笔记"><Star size={16} fill={draft.favorite ? "currentColor" : "none"}/></button><button ref={historyButton} aria-expanded={!!history} onClick={()=>{if(history)closeHistory();else action(openHistory);}}><History size={16}/>历史</button><button disabled={!ready} onClick={()=>action(flush)}><Save size={16}/>保存</button><select aria-label="导出笔记" value="" disabled={paused} onChange={e=>{const format=e.target.value;if(!format)return;action(async()=>{await flush();const path=await save({defaultPath:`${draftRef.current.title.replace(/[<>:"/\\|?*]/g,"_")}.${format}`,filters:[{name:format==="md"?"Markdown（附件另存同目录）":"笔记包（当前版本与附件）",extensions:[format]}]});if(!path)return;await withKnowledgeBarrier(async()=>{const latest=await client.get(note.id);if(format==="md")await client.exportMarkdown(latest,serializeNoteMarkdown(latest.document),path);else await client.exportPackage(note.id,path);});notify("已导出当前版本及附件；完整历史请使用整库备份");});}}><option value="">导出…</option><option value="md">Markdown＋附件</option><option value="dccnotes">结构化笔记包</option></select><button onClick={()=>action(async()=>{await flush();if(note.deletedAt || window.confirm("将笔记移入回收站？历史版本和附件会保留。")) onDeleted(await client.setDeleted(savedRef.current,!note.deletedAt));})}>{note.deletedAt ? "恢复笔记" : <Trash2 size={16}/>}</button>{note.deletedAt && <button onClick={()=>action(async()=>{if(window.confirm("永久删除这篇笔记、全部历史版本和附件？此操作无法撤销。")){await client.purge(savedRef.current);onDeleted(null);}})}>永久删除</button>}</div></header>
    {paused&&<p className="knowledge-history-banner">正在安全保存、备份或切换素材库，编辑暂时锁定。</p>}
    {error && <div className="knowledge-save-error" role="alert">{error}<button onClick={()=>action(flush)}>重试保存</button><button onClick={()=>action(async()=>{await Promise.all([...imports.current]);const path=await save({defaultPath:"未保存草稿.dccnotes",filters:[{name:"当前草稿与附件",extensions:["dccnotes"]}]});if(path){await client.exportDraft(draftRef.current,path);setDraftExported(true);notify("当前草稿已另存，源笔记未修改");}})}>另存当前草稿</button><button disabled={!draftExported} title="先另存草稿后可重新读取库中版本" onClick={()=>action(async()=>{if(!window.confirm("草稿已另存。重新读取素材库中的版本？"))return;if(timer.current)clearTimeout(timer.current);const latest=await client.get(note.id);queue.reset(latest);const fresh={...latest,expectedRevision:latest.revision};draftRef.current=fresh;savedRef.current=latest;setDraft(fresh);setTagsText(latest.tags.join("; "));editor?.commands.setContent(latest.document,{emitUpdate:false});setError("");setStatus("已保存");setDraftExported(false);onDeleted(latest);})}>重新读取库中版本</button></div>}
    {preview && <div className="knowledge-history-banner">正在预览历史版本 · {new Date(preview.createdAt).toLocaleString()}<button onClick={()=>display(null)}>返回当前版本</button><button onClick={()=>action(async()=>{const restored=await client.restore(savedRef.current,preview.id);onDeleted(restored);setPreview(null);setHistory(null);})}>恢复此版本</button></div>}
    <div className="knowledge-document"><input className="knowledge-title" aria-label="笔记标题" disabled={!ready || paused || !!note.deletedAt || !!preview} value={preview?.snapshot.title ?? draft.title} onChange={e=>change({title:e.target.value})}/>
      <div className="knowledge-metadata"><select aria-label="所属笔记本" value={draft.notebookId || ""} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>change({notebookId:e.target.value||null})}><option value="">未分笔记本</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select><input aria-label="笔记标签" placeholder="标签，用分号分隔" value={tagsText} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>{setTagsText(e.target.value);change({tags:e.target.value.split(/[;；]/)})}}/></div>
      {ready && !paused && !note.deletedAt && !preview && <div className="knowledge-toolbar" aria-label="笔记格式"><button onClick={()=>editor?.chain().focus().toggleBold().run()}>粗体</button><button onClick={()=>editor?.chain().focus().toggleItalic().run()}>斜体</button><button onClick={()=>editor?.chain().focus().toggleBulletList().run()}>列表</button><button onClick={()=>editor?.chain().focus().toggleTaskList().run()}>待办</button><button onClick={()=>editor?.chain().focus().toggleCodeBlock().run()}>代码</button><button onClick={()=>editor?.chain().focus().insertTable({rows:3,cols:3,withHeaderRow:true}).run()}>表格</button><button onClick={()=>action(async()=>{const paths=await open({multiple:true,filters:[{name:"笔记附件",extensions:["png","jpg","jpeg","webp","gif","pdf","txt","md"]}]});if(!paths)return;await flush();await trackImport((async()=>{for(const path of paths){const item=await client.importAttachment(note.id,path);if(item.mime.startsWith("image/"))editor?.chain().focus().setImage({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename}).run();else editor?.chain().focus().insertContent({type:"attachment",attrs:{attachmentId:item.id,label:item.filename}}).run();}})());})}>图片 / 附件</button><button onClick={()=>setShowReferences(true)}>插入引用</button><button onClick={()=>action(async()=>{await trackImport(client.pasteImage(note.id).then(item=>{editor?.chain().focus().setImage({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename}).run();}));})}>粘贴截图</button><button onClick={()=>action(async()=>{const selection=editor ? editor.state.doc.textBetween(editor.state.selection.from,editor.state.selection.to,"\n") : "";await flush();setAi({note:await client.get(note.id),selection});})}>AI 辅助</button></div>}
      <NoteEditorTools editor={editor} libraryId={libraryId} disabled={!ready||paused||!!note.deletedAt||!!preview} onAi={()=>toolbarAction("AI 辅助")} onInsert={kind=>toolbarAction(kind==="attachment"?"图片 / 附件":kind==="screenshot"?"粘贴截图":"插入引用")}/><EditorContent editor={editor}/>
      {references.some(v=>!v.available)&&<section className="knowledge-backlinks"><h3>来源暂不可用</h3>{references.filter(v=>!v.available).map(v=><p key={`${v.kind}-${v.targetId}`}>{v.label} · 已删除或不在当前素材库，引用保留</p>)}</section>}
      {backlinks.length>0&&<section className="knowledge-backlinks"><h3>哪些笔记引用了这里</h3>{backlinks.map(v=><button key={v.id} onClick={()=>onNavigateNote(v.id)}>{v.title}</button>)}</section>}
    </div>
    {source&&<NoteSourcePreview {...source} onClose={()=>setSource(null)} onProject={onOpenProject?id=>action(async()=>{await flush();onOpenProject(id);}):undefined}/>}
    {ai&&<NoteAiDialog libraryId={libraryId} note={ai.note} selection={ai.selection} onClose={()=>setAi(null)} onNavigate={onNavigateNote} onApply={text=>{editor?.chain().focus().insertContentAt(editor.state.doc.content.size,{type:"paragraph",content:[{type:"text",text}]}).run();setAi(null);}}/>}
    {showReferences&&<NoteReferencePicker libraryId={libraryId} onClose={()=>setShowReferences(false)} onPick={v=>{editor?.chain().focus().insertContent({type:"reference",attrs:{kind:v.kind,targetId:v.targetId,label:v.label}}).run();setShowReferences(false);}}/>}
    {history && <aside ref={historyPanel} className="knowledge-history" aria-label="历史版本"><header><strong>历史版本</strong><button onClick={closeHistory}><X size={16}/></button></header><p>最近 50 个未固定版本；短时间编辑合并记录。</p>{historyBusy?<p>正在读取历史版本…</p>:history.length===0&&<p>首次修改后会保留历史版本。</p>}{history.map(v=><div key={v.id}><button onClick={()=>action(async()=>{const generation=historyGeneration.current;const revision=await client.revision(note.id,v.id);if(generation===historyGeneration.current)display(revision);})}>{v.title}<small>{new Date(v.createdAt).toLocaleString()}</small></button><button title={v.pinned ? "取消固定" : "固定版本"} onClick={()=>action(async()=>{const generation=historyGeneration.current;await client.pin(v.id,!v.pinned);const items=await client.revisions(note.id);if(generation===historyGeneration.current)setHistory(items);})}>{v.pinned ? "已固定" : "固定"}</button></div>)}{history.length>0&&history.length%100===0&&<button onClick={()=>action(async()=>{const generation=historyGeneration.current;const more=await client.revisions(note.id,history.length);if(generation!==historyGeneration.current)return;setHistory([...history,...more]);if(!more.length)notify("已加载全部历史版本");})}>加载更多历史</button>}</aside>}
  </section>;
}
