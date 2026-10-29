import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import {learningNoteExtensions,type LearningReferenceData} from "../lib/learningNoteNodes";
import { normalizeStructure } from "../lib/noteStructure";
import { EditorContent, useEditor, type JSONContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { open, save } from "@tauri-apps/plugin-dialog";
import { parseNoteMarkdown, serializeNoteMarkdown } from "../lib/noteMarkdown";
import { NoteTopicHierarchy, ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode, NoteRemoteImageNode, NoteMindMapNode } from "../lib/noteExtensions";
import { TableKit } from "@tiptap/extension-table";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, ChevronRight, ChevronDown, FolderPlus, History, Plus, Save, Search, Star, Trash2, X, ListTree, Link2, Tags } from "lucide-react";
import { api } from "../lib/api";
import { knowledgeApi, noteTemplates, NoteSaveQueue, templateDocument, type KnowledgeNote, type KnowledgeNotebook, type NoteInput, type NoteRevision, type NoteRevisionSummary, type NoteTemplate, type NoteSummary, type KnowledgeReference } from "../lib/knowledge";
import { registerNoteSaveGuard, knowledgeChangesPaused, subscribeKnowledgeBarrier, withKnowledgeBarrier } from "../lib/noteSaveGuard";
import "./knowledge.css";
import { NoteAiDialog } from "./NoteAiDialog";
import { NoteSourcePreview } from "./NoteSourcePreview";
import { NoteReferencePicker } from "./NoteReferencePicker";
import { KnowledgeMenu, type KnowledgeMenuState } from "./KnowledgeMenu";
import { NoteEditorTools, NoteHeadingOutline } from "./NoteEditorTools";
import {copyNoteSelection,pasteNoteSelection} from "../lib/noteClipboard";
import {defaultNoteAppearance,normalizeNoteAppearance,openNoteTab,sanitizeNoteTabs,mergeRestoredNoteTabs,type NoteTab} from "../lib/noteWorkspace";
import {noteMapPng,noteSvgPng,printNoteDocument} from "../lib/noteOutput";
import {NoteAppearanceDialog} from "./NoteAppearanceDialog";
import {beginEdgeScroll,beginNavigationFeedback,beginPointerReorder} from "../lib/drag";
import {NoteFormattingShortcuts,activeNoteShortcutAction} from "../lib/noteShortcuts";
import {NoteFindBar,NoteSearchHighlights,NoteGlobalSearch} from "./NoteFindBar";
import { LearningReview } from "./LearningReview";
import { NoteRecovery } from "./NoteRecovery";
import { LoadFeedback, ReadFailure } from "./LoadFeedback";
import { recordDiagnostic } from "../lib/diagnostics";
import {mapFromHeadings,normalizeNoteMap,type NoteMapData} from "../lib/noteMindMap";
import {WorkspacePanel} from "./WorkspacePanel";
import {normalizePanel,persistPanels,setLayoutPanel,togglePanelFocus,type PanelLayout,type PanelState} from "../lib/workspacePanels";
const NoteMindMapDialog=lazy(()=>import("./NoteMindMapDialog"));
const NoteArchiveBrowser=lazy(()=>import("../features/learning/NoteArchiveBrowser"));
const LearningImportPanel=lazy(()=>import("../features/learning/LearningImportPanel"));
const NoteStructureView=lazy(()=>import("./NoteStructureView"));

interface Props { onLearnReference?:(reference:LearningReferenceData,index:number,noteId:string)=>Promise<void>; onOpenProject?:(id:string)=>void; initialNoteId?: string | null; notify: (message: string, error?: boolean) => void }
const infoLimits={min:260,max:480,defaultWidth:320};
const knowledgePanels=():PanelLayout=>({left:{mode:"pinned",width:480,tab:"navigation"},right:{mode:"closed",width:320,tab:"outline"},focusSnapshot:null});
export default function KnowledgeWorkspace({ notify, initialNoteId,onOpenProject,onLearnReference }: Props) {
  const [libraryId, setLibraryId] = useState("");
  const [openError, setOpenError] = useState("");
  const [openAttempt, setOpenAttempt] = useState(0);
  useEffect(() => { let active = true; setOpenError(""); void api.getWorkspaceContext("knowledge").then(v => { if (active) setLibraryId(v.libraryId); }).catch(e => { if(active){setOpenError(String(e));notify(String(e), true);} }); return () => { active = false; }; }, [notify, openAttempt]);
  return libraryId ? <Workspace key={libraryId} libraryId={libraryId} initialNoteId={initialNoteId} onOpenProject={onOpenProject} onLearnReference={onLearnReference} notify={notify} /> : openError ? <div className="knowledge-loading"><p role="alert">知识库打开失败：{openError}</p><button className="secondary-button" onClick={()=>setOpenAttempt(v=>v+1)}>重新打开知识库</button></div> : <LoadFeedback label="正在打开知识库…" skeleton/>;
}

function Workspace({ libraryId, notify, initialNoteId,onOpenProject,onLearnReference }: Props & { libraryId: string }) {
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [panels,setPanels]=useState(knowledgePanels);
  const [navigationScope,setNavigationScope]=useState<"all"|"inbox"|"recent"|"project">("all");
  const [projectFilter,setProjectFilter]=useState<KnowledgeReference|null>(null),[projectPicker,setProjectPicker]=useState(false);
  const [archiveOpen,setArchiveOpen]=useState(false);
  const [importOpen,setImportOpen]=useState(false);
  const [notebooks, setNotebooks] = useState<KnowledgeNotebook[]>([]);
  const [folder, setFolder] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const [query, setQuery] = useState(""); const [favorite, setFavorite] = useState(false); const [trash, setTrash] = useState(false);
  const [manualOrder,setManualOrder]=useState(false);
  const [page, setPage] = useState(0); const [total, setTotal] = useState(0); const [notes, setNotes] = useState<NoteSummary[]>([]);
  const [current, setCurrentState] = useState<KnowledgeNote | null>(null); const [reload, setReload] = useState(0);
  const [tabs,setTabs]=useState<NoteTab[]>([]);const [closedTabs,setClosedTabs]=useState<NoteTab[]>([]);
  const [contextReady,setContextReady]=useState(false),[contextError,setContextError]=useState(""),[contextAttempt,setContextAttempt]=useState(0);
  const contextScope=useRef<{enabled:boolean;value:Record<string,unknown>}|null>(null),contextWrites=useRef(Promise.resolve());
  const layoutDirty=useRef(false),noteAction=useRef(0);
  const [appearance,setAppearance]=useState(defaultNoteAppearance);const [appearanceBefore,setAppearanceBefore]=useState<typeof appearance|null>(null);
  const [appearanceReady,setAppearanceReady]=useState(false),[appearanceError,setAppearanceError]=useState(""),[appearanceAttempt,setAppearanceAttempt]=useState(0);
  const appearanceRef=useRef(appearance),appearanceBeforeRef=useRef(appearanceBefore),appearanceDirty=useRef(false),appearanceWrites=useRef(Promise.resolve());
  appearanceRef.current=appearance;appearanceBeforeRef.current=appearanceBefore;
  const mounted=useRef(true);useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  const leftWidth=appearance.navigationWidth+appearance.listWidth;
  const leftLimits={min:appearance.navigationWidth+220,max:appearance.navigationWidth+440,defaultWidth:appearance.navigationWidth+280};
  const effectiveManual=manualOrder&&navigationScope==="all";
  const focused=!!panels.focusSnapshot;
  const changeLeft=(next:PanelState)=>{
    layoutDirty.current=true;
    if(appearanceReady&&!appearanceBefore&&next.width!==leftWidth){appearanceDirty.current=true;setAppearance(value=>({...value,listWidth:next.width-value.navigationWidth}));}
    setPanels(value=>setLayoutPanel(value,"left",next));
  };
  const expandTimer=useRef<ReturnType<typeof setTimeout>|null>(null);const expandingNotebook=useRef<string|null>(null);
  const [dropTarget,setDropTarget]=useState<{id:string|null;zone:"before"|"inside"|"after";invalid:boolean}|null>(null);const [undoMove,setUndoMove]=useState<string|null>(null);
  const reorderPageTimer=useRef<ReturnType<typeof setTimeout>|null>(null);
  const pointerCleanup=useRef<(()=>void)|null>(null);const suppressClick=useRef(false);
  const [dragPreview,setDragPreview]=useState<{x:number;y:number;label:string}|null>(null);
  const setCurrent=(note:KnowledgeNote|null)=>{if(note)setTabs(value=>openNoteTab(value,note.id,note.title).map(t=>t.id===note.id?{...t,color:note.color}:t));setCurrentState(note);};
  useEffect(()=>{let active=true;setAppearanceReady(false);setAppearanceError("");void invoke("get_note_appearance").then(value=>{if(active){if(!appearanceDirty.current)setAppearance(normalizeNoteAppearance(value));setAppearanceReady(true);}}).catch(e=>{if(active)setAppearanceError(String(e));});return()=>{active=false;};},[appearanceAttempt]);
  const saveAppearance=useCallback((value:typeof appearance)=>{
    const request=appearanceWrites.current.then(async()=>{await invoke("save_note_appearance",{value});if(appearanceRef.current===value||appearanceBeforeRef.current===value)appearanceDirty.current=false;if(mounted.current)setAppearanceError("");});
    appearanceWrites.current=request.catch(error=>{if(mounted.current)setAppearanceError(String(error));});return request;
  },[]);
  useEffect(()=>{if(!appearanceReady||!appearanceDirty.current||appearanceBefore)return;const timer=setTimeout(()=>{void saveAppearance(appearance).catch(()=>{});},400);return()=>clearTimeout(timer);},[appearance,appearanceBefore,appearanceReady,saveAppearance]);
  useEffect(()=>()=>{if(appearanceDirty.current)void saveAppearance(appearanceBeforeRef.current||appearanceRef.current).catch(()=>{});},[saveAppearance]);
  useEffect(()=>{
    let active=true;setContextReady(false);setContextError("");contextScope.current=null;
    void api.getWorkspaceContext("knowledge").then(async context=>{
      if(!active)return;if(context.libraryId!==libraryId)throw new Error("素材库已切换，未恢复其他库的布局");
      const value=(context.enabled&&context.value&&typeof context.value==="object"?context.value:{}) as Record<string,unknown>;
      contextScope.current={enabled:context.enabled,value};
      if(!layoutDirty.current){setManualOrder(value.manualOrder===true);const stored=(value.panels&&typeof value.panels==="object"?value.panels:{}) as Record<string,unknown>;const fallback=knowledgePanels();const left=normalizePanel(stored.left,fallback.left,{min:380,max:780,defaultWidth:480},["navigation"]);const right=normalizePanel(stored.right,fallback.right,infoLimits,["outline","sources","tags","history"]);setPanels({...fallback,left:{...left,mode:left.mode==="peek"?"closed":left.mode},right:{...right,mode:right.mode==="peek"?"closed":right.mode}});}
      const restored=sanitizeNoteTabs(value.tabs);setTabs(opened=>mergeRestoredNoteTabs(restored,opened));
      const activeId=value.activeId;
      if(!initialNoteId&&noteAction.current===0&&typeof activeId==="string"&&restored.some(v=>v.id===activeId)){try{const note=await client.get(activeId);if(active&&noteAction.current===0)setCurrentState(note);}catch{if(active)setTabs(v=>v.filter(t=>t.id!==activeId));}}
      if(active)setContextReady(true);
    }).catch(error=>{if(active)setContextError(String(error));});
    return()=>{active=false;};
  },[client,libraryId,contextAttempt]);
  const contextValue={tabs,activeId:current?.id||null,manualOrder,panels:{...persistPanels(panels),left:{...persistPanels(panels).left,width:(appearanceBefore||appearance).navigationWidth+(appearanceBefore||appearance).listWidth}}};
  const contextValueRef=useRef(contextValue);contextValueRef.current=contextValue;
  const saveContext=useCallback((value:typeof contextValue)=>{
    const scope=contextScope.current;if(!scope?.enabled)return;
    contextWrites.current=contextWrites.current.then(async()=>{if(!await api.saveWorkspaceContext(libraryId,"knowledge",{...scope.value,...value}))throw new Error("素材库已切换");if(mounted.current)setContextError("");}).catch(error=>{if(mounted.current)setContextError(String(error));});
  },[libraryId]);
  useEffect(()=>{if(!contextReady)return;const timer=setTimeout(()=>saveContext(contextValueRef.current),400);return()=>clearTimeout(timer);},[tabs,current?.id,contextReady,manualOrder,panels,appearance,appearanceBefore,saveContext]);
  useEffect(()=>()=>{saveContext(contextValueRef.current);contextScope.current=null;},[saveContext]);
  const [template, setTemplate] = useState<NoteTemplate>("blank");
  const [folderForm, setFolderForm] = useState<{ id?:string; order?:number; parent: string | null; name: string } | null>(null);
  const [listFailed,setListFailed]=useState(false);
  const [busy, setBusy] = useState(false); const [loading, setLoading] = useState(false);
  const [menu,setMenu]=useState<KnowledgeMenuState|null>(null);
  const [globalSearch,setGlobalSearch]=useState(false);
  useEffect(()=>{const open=()=>setGlobalSearch(true);document.addEventListener("note-global-search",open);return()=>document.removeEventListener("note-global-search",open);},[]);
  const [noteForm,setNoteForm]=useState<{note:KnowledgeNote;mode:"rename"|"move";title:string;folder:string|null}|null>(null);
  const [colorForm,setColorForm]=useState<{id:string;color:string|null}|null>(null);
  const [undoDeleted,setUndoDeleted]=useState<KnowledgeNote|null>(null);
  const flush = useRef<() => Promise<void>>(async () => {});
  const operationRunning=useRef(false);
  const run = async (action: () => Promise<void>) => {
    if(operationRunning.current){notify("正在保存或处理笔记，请稍候");return;}
    noteAction.current++;operationRunning.current=true;setBusy(true);
    try{await action();}catch(error){notify(String(error),true);}finally{operationRunning.current=false;setBusy(false);}
  };
  useEffect(() => { void client.notebooks().then(setNotebooks).catch(e => notify(String(e), true)); }, [client, reload, notify]);
  useEffect(() => {
    let active = true; setLoading(true);setListFailed(false);
    const timer = setTimeout(() => { void client.search({ query, notebookId: folder, trash, favorite, offset: page * 40, limit: 40, manualOrder:effectiveManual,...(navigationScope==="inbox"?{unfiledOnly:true}:{}),...(navigationScope==="project"&&projectFilter?{projectId:projectFilter.targetId}:{}) }).then(v => { if (active) { setNotes(v.items); setTotal(v.total); } }).catch(e => { if (active){setListFailed(true);recordDiagnostic("read-failure");notify(String(e), true);} }).finally(() => { if (active) setLoading(false); }); }, 180);
    return () => { active = false; clearTimeout(timer); };
  }, [client, query, folder, trash, favorite, page, reload, notify, effectiveManual,navigationScope,projectFilter]);
  const choose = (note: NoteSummary) => run(async () => { await flush.current(); setCurrent(await client.get(note.id)); });
  const closeTab=(id:string)=>run(async()=>{await flush.current();const tab=tabs.find(v=>v.id===id);if(!tab)return;if(tab.pinned&&!window.confirm("关闭固定的笔记标签？"))return;const next=tabs.filter(v=>v.id!==id);if(current?.id===id){const replacement=next[Math.min(tabs.findIndex(v=>v.id===id),next.length-1)];const note=replacement?await client.get(replacement.id):null;flush.current=async()=>{};setCurrentState(note);}setClosedTabs(v=>[...v.slice(-19),tab]);setTabs(next);});
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
    {label:"打开",run:()=>choose(note)},{label:"新建笔记",run:()=>targetAction(note.id,async v=>{setCurrent(await client.save({id:null,expectedRevision:null,notebookId:v.notebookId,title:"未命名笔记",document:templateDocument("blank"),tags:[],favorite:false,journalDate:null}));setTrash(false);})},
    {label:"重命名…",run:()=>targetAction(note.id,async v=>setNoteForm({note:v,mode:"rename",title:v.title,folder:v.notebookId}))},
    {label:"笔记颜色…",run:()=>setColorForm({id:note.id,color:note.color||null})},
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
  const beginNoteReorder=(event:React.PointerEvent<HTMLElement>,note:NoteSummary)=>{
    if(busy||knowledgeChangesPaused())return;pointerCleanup.current?.();
    pointerCleanup.current=beginPointerReorder(event,{
      selector:"[data-note-order],[data-notebook-drop]",
      placement:(target,point)=>{if(!target.dataset.noteOrder)return "inside";const rect=target.getBoundingClientRect();return point.y>rect.top+rect.height/2?"after":"before";},
      onDrop:(target,after)=>{const targetId=target.dataset.noteOrder;void run(async()=>{await flush.current();if(targetId)await client.reorder(note.id,targetId,after,folder,query,favorite);else {setUndoMove(await client.move("note",note.id,target.dataset.notebookDrop||null,0));if(current)setCurrent(await client.get(current.id));}setReload(v=>v+1);notify(targetId?"笔记顺序已保存":"笔记已移动，可撤销");});},
      onInvalid:()=>notify("这里不能放置：请选择笔记排序位置或目标笔记本"),
      onFinish:()=>{if(reorderPageTimer.current)clearTimeout(reorderPageTimer.current);reorderPageTimer.current=null;},
      onEdge:point=>{const list=document.querySelector(".knowledge-note-list");if(!list)return;const rect=list.getBoundingClientRect();const direction=point.x>=rect.left&&point.x<=rect.right?(point.y<=rect.top+26?-1:point.y>=rect.bottom-26?1:0):0;if(!direction){if(reorderPageTimer.current)clearTimeout(reorderPageTimer.current);reorderPageTimer.current=null;return;}if(reorderPageTimer.current)return;reorderPageTimer.current=setTimeout(()=>{setPage(v=>Math.max(0,Math.min(Math.ceil(total/40)-1,v+direction)));reorderPageTimer.current=null;},650);},
    });
  };
  const beginPointerDrag=(event:React.PointerEvent,item:{kind:"note"|"notebook"|"tab";id:string},label:string)=>{
    if(event.button!==0||busy||(trash&&item.kind!=="tab")||knowledgeChangesPaused())return;
    if(item.kind==="tab"&&(event.target as Element).closest("button:not([role=tab])"))return;
    if(item.kind==="notebook"&&(event.target as Element).closest("button:not(.knowledge-folder-name)"))return;
    pointerCleanup.current?.();
    if(item.kind!=="note"){
      pointerCleanup.current=beginPointerReorder(event as React.PointerEvent<HTMLElement>,{
        selector:item.kind==="tab"?"[data-note-tab]":"[data-notebook-drop]",
        placement:(target,point)=>{if(item.kind==="tab")return point.x>target.getBoundingClientRect().left+target.getBoundingClientRect().width/2?"after":"before";if(!target.dataset.notebookDrop)return "inside";const rect=target.getBoundingClientRect(),ratio=(point.y-rect.top)/Math.max(1,rect.height);return ratio<.25?"before":ratio>.75?"after":"inside";},
        canDrop:(target,zone)=>{const id=item.kind==="tab"?target.dataset.noteTab:target.dataset.notebookDrop;if(id===item.id)return false;if(item.kind==="tab")return true;let parent=zone==="inside"?id:notebooks.find(n=>n.id===id)?.parentId;const seen=new Set<string>();while(parent){if(parent===item.id||seen.has(parent))return false;seen.add(parent);parent=notebooks.find(n=>n.id===parent)?.parentId;}return true;},
        onHover:(target,zone,valid)=>{if(item.kind!=="notebook")return;const id=target?.dataset.notebookDrop;const expand=id&&zone==="inside"&&valid&&collapsed.has(id)?id:null;if(expandingNotebook.current!==expand){if(expandTimer.current)clearTimeout(expandTimer.current);expandingNotebook.current=expand;if(expand)expandTimer.current=setTimeout(()=>setCollapsed(v=>{const next=new Set(v);next.delete(expand);return next;}),600);}},
        onFinish:()=>{if(expandTimer.current)clearTimeout(expandTimer.current);expandingNotebook.current=null;},
        onInvalid:()=>notify("这里不能放置：请选择有效同级位置，不能移入自身或子笔记本"),
        onDrop:(target,after,zone)=>{
          if(item.kind==="tab"){const id=target.dataset.noteTab;setTabs(v=>{const source=v.find(t=>t.id===item.id);if(!source)return v;const next=v.filter(t=>t.id!==item.id);const index=next.findIndex(t=>t.id===id);if(index<0)return v;next.splice(index+(after?1:0),0,source);return next;});return;}
          const id=target.dataset.notebookDrop||null;const folder=notebooks.find(n=>n.id===id);const parent=zone==="inside"?id:folder?.parentId||null;const siblings=notebooks.filter(n=>n.parentId===parent&&n.id!==item.id);const index=zone==="inside"?siblings.length:Math.max(0,siblings.findIndex(n=>n.id===id))+(after?1:0);
          void run(async()=>{await flush.current();setUndoMove(await client.move("notebook",item.id,parent,index));setReload(v=>v+1);notify("笔记本已移动，可撤销");});
        },
      });return;
    }
    const start={x:event.clientX,y:event.clientY};const pointerId=event.pointerId;let active=false;
    let destination:typeof dropTarget=null;
    const sourceElement=event.currentTarget as HTMLElement;
    let feedback:((accepted:boolean)=>void)|null=null;
    const edgeScroll=beginEdgeScroll();
    const cleanup=()=>{edgeScroll.stop();window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",up);window.removeEventListener("pointercancel",cancel);window.removeEventListener("keydown",escape);document.body.classList.remove("library-pointer-dragging");setDropTarget(null);setDragPreview(null);if(expandTimer.current)clearTimeout(expandTimer.current);expandingNotebook.current=null;pointerCleanup.current=null;};
    const move=(e:PointerEvent)=>{if(e.pointerId!==pointerId)return;if(!active&&Math.hypot(e.clientX-start.x,e.clientY-start.y)<6)return;if(!active)feedback=beginNavigationFeedback(sourceElement);active=true;e.preventDefault();document.body.classList.add("library-pointer-dragging");setDragPreview({x:e.clientX,y:e.clientY,label});edgeScroll.update({x:e.clientX,y:e.clientY});
      const row=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>(item.kind==="tab"?"[data-note-tab]":"[data-notebook-drop]");destination=null;
      if(item.kind==="tab"){if(row){const rect=row.getBoundingClientRect();destination={id:row.dataset.noteTab||null,zone:e.clientX<rect.left+rect.width/2?"before":"after",invalid:row.dataset.noteTab===item.id};}setDropTarget(destination);return;}
      if(row){const id=row.dataset.notebookDrop||null;const target=notebooks.find(n=>n.id===id);const rect=row.getBoundingClientRect();const ratio=(e.clientY-rect.top)/Math.max(1,rect.height);const zone=item.kind==="note"||!target?"inside":ratio<.25?"before":ratio>.75?"after":"inside";let parent=target?(zone==="inside"?target.id:target.parentId):null;let invalid=target?.id===item.id;const seen=new Set<string>();while(item.kind==="notebook"&&parent){if(parent===item.id||seen.has(parent)){invalid=true;break;}seen.add(parent);parent=notebooks.find(n=>n.id===parent)?.parentId||null;}destination={id,zone,invalid};
        const expand=target&&collapsed.has(target.id)&&!invalid?target.id:null;if(expandingNotebook.current!==expand){if(expandTimer.current)clearTimeout(expandTimer.current);expandingNotebook.current=expand;if(expand)expandTimer.current=setTimeout(()=>setCollapsed(v=>{const next=new Set(v);next.delete(expand);return next;}),600);}
      }
      setDropTarget(destination);
    };
    const up=(e:PointerEvent)=>{if(e.pointerId!==pointerId)return;const drop=destination;cleanup();if(!active)return;suppressClick.current=true;setTimeout(()=>{suppressClick.current=false;},0);feedback?.(!!drop&&!drop.invalid);if(!drop||drop.invalid){notify("这里不能放置：请选择有效笔记本或标签位置");return;}const kind=item.kind;if(kind==="tab"){setTabs(v=>{const source=v.find(t=>t.id===item.id);if(!source)return v;const next=v.filter(t=>t.id!==item.id);const index=next.findIndex(t=>t.id===drop.id);if(index<0)return v;next.splice(index+(drop.zone==="after"?1:0),0,source);return next;});return;}void run(async()=>{await flush.current();const target=notebooks.find(n=>n.id===drop.id);const parent=target?(drop.zone==="inside"?target.id:target.parentId):null;const siblings=notebooks.filter(n=>n.parentId===parent&&n.id!==item.id);const index=target&&drop.zone!=="inside"?Math.max(0,siblings.findIndex(n=>n.id===target.id))+(drop.zone==="after"?1:0):siblings.length;setUndoMove(await client.move(kind,item.id,parent,index));if(current)setCurrent(await client.get(current.id));setReload(v=>v+1);notify("已移动，可撤销最近一次移动");});};
    const cancel=()=>{feedback?.(false);cleanup();};const escape=(e:KeyboardEvent)=>{if(e.key==="Escape")cancel();};pointerCleanup.current=cancel;window.addEventListener("pointermove",move,{passive:false});window.addEventListener("pointerup",up);window.addEventListener("pointercancel",cancel);window.addEventListener("keydown",escape);
  };
  useEffect(()=>()=>{pointerCleanup.current?.();if(expandTimer.current)clearTimeout(expandTimer.current);},[]);
  const tree = (parent: string | null, depth = 0): React.ReactNode => notebooks.filter(n => n.parentId === parent).map(n => <div key={n.id}>
    <div data-notebook-drop={n.id} onPointerDown={e=>beginPointerDrag(e,{kind:"notebook",id:n.id},n.name)} draggable={false} onContextMenu={e=>folderMenu(e,n)} className={`knowledge-folder ${folder === n.id ? "active" : ""} ${dropTarget?.id===n.id?`drop-${dropTarget.invalid?"invalid":dropTarget.zone}`:""}`} style={{ paddingLeft: 8 + depth * 14 }}>
      <button aria-label={`${collapsed.has(n.id) ? "展开" : "收起"}${n.name}`} onClick={() => setCollapsed(v => { const next = new Set(v); if (next.has(n.id)) next.delete(n.id); else next.add(n.id); return next; })}>{collapsed.has(n.id) ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</button>
      <button className="knowledge-folder-name" onClick={() => { setNavigationScope("all");setProjectFilter(null);setFolder(n.id); setFavorite(false); setTrash(false); setPage(0); }}>{n.name}</button>
      <button title="管理笔记本" onClick={()=>setFolderForm({id:n.id,order:n.sortOrder,parent:n.parentId,name:n.name})}>⋯</button><button title="新建子笔记本" onClick={() => setFolderForm({ parent: n.id, name: "" })}><Plus size={14} /></button>
    </div>{!collapsed.has(n.id) && tree(n.id, depth + 1)}</div>);
  const tab=tabs.find(v=>v.id===current?.id);
  const appearanceStyle={"--note-background":appearance.background,"--note-text":appearance.text,"--note-accent":appearance.accent,"--note-font-size":`${appearance.fontSize}px`,"--note-line-height":appearance.lineHeight,"--note-content-width":`${appearance.contentWidth}px`,"--note-nav-width":`${appearance.navigationWidth}px`,"--note-list-width":`${appearance.listWidth}px`} as CSSProperties;
  return <div onKeyDownCapture={e=>{if(!e.nativeEvent.isComposing&&!document.querySelector(".modal-backdrop")&&activeNoteShortcutAction(e.nativeEvent)==="noteGlobalFind"){e.preventDefault();e.stopPropagation();setGlobalSearch(true);}}} onClickCapture={e=>{if(suppressClick.current){e.preventDefault();e.stopPropagation();}}} className={`knowledge-workspace ${focused?"knowledge-focused":""} ${!appearanceReady?"knowledge-appearance-pending":""}`} style={appearanceStyle} data-code-theme={appearance.codeTheme}>
    {colorForm&&<div className="modal-backdrop" onPointerDown={e=>{if(e.target===e.currentTarget)setColorForm(null);}}><form className="modal-card note-color-dialog" role="dialog" aria-label="笔记导航颜色" onSubmit={e=>{e.preventDefault();targetAction(colorForm.id,async()=>{const value=await client.setColor(colorForm.id,colorForm.color);changed(value);setColorForm(null);setTabs(v=>v.map(t=>t.id===value.id?{...t,color:value.color}:t));});}}><h2>笔记导航颜色</h2><p>只改变列表和标签，不改变正文。</p><div className="note-presets">{["#D99A42","#7CACCB","#85B596","#B097C9","#D38D92"].map(color=><button key={color} type="button" aria-label={`选择 ${color}`} style={{background:color}} onClick={()=>setColorForm({...colorForm,color})}>●</button>)}</div><label>自定义颜色<input type="color" aria-label="自定义笔记颜色" value={colorForm.color||"#D99A42"} onChange={e=>setColorForm({...colorForm,color:e.target.value})}/></label><footer><button type="button" onClick={()=>setColorForm({...colorForm,color:null})}>恢复默认</button><button type="button" onClick={()=>setColorForm(null)}>取消</button><button disabled={busy}>应用</button></footer></form></div>}
    {dragPreview&&<div className="knowledge-drag-preview" style={{left:dragPreview.x+14,top:dragPreview.y+14}}>{dragPreview.label}</div>}
    {undoMove&&<div className="knowledge-move-undo" role="status">已移动<button disabled={busy} onClick={()=>run(async()=>{await flush.current();await client.undoMove(undoMove);setUndoMove(null);if(current)setCurrent(await client.get(current.id));setReload(v=>v+1);})}>撤销移动</button><button aria-label="关闭移动提示" onClick={()=>setUndoMove(null)}>×</button></div>}
    {appearanceBefore&&<NoteAppearanceDialog value={appearanceBefore} onPreview={setAppearance} onCancel={()=>{setAppearance(appearanceBefore);setAppearanceBefore(null);}} onApply={async value=>{await saveAppearance(value);appearanceDirty.current=false;setAppearance(value);setAppearanceBefore(null);}}/>}
    {menu&&<KnowledgeMenu menu={menu} onClose={()=>setMenu(null)}/>}
    {undoDeleted&&<div className="knowledge-undo" role="status">已删除“{undoDeleted.title}”<button disabled={busy} onClick={()=>run(async()=>{const latest=await client.get(undoDeleted.id);if(!latest.deletedAt){setUndoDeleted(null);return;}const restored=await client.setDeleted(latest,false);setUndoDeleted(null);setCurrent(restored);setTrash(false);setReload(v=>v+1);})}>撤销删除</button><button aria-label="关闭撤销提示" onClick={()=>setUndoDeleted(null)}>×</button></div>}
    {noteForm&&<div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-label={noteForm.mode==="rename"?"重命名笔记":"移动笔记"} onSubmit={e=>{e.preventDefault();run(async()=>{const value=await client.save({...noteForm.note,title:noteForm.title,notebookId:noteForm.folder,expectedRevision:noteForm.note.revision});changed(value);setNoteForm(null);setReload(v=>v+1);});}}><h2>{noteForm.mode==="rename"?"重命名笔记":"移动笔记"}</h2>{noteForm.mode==="rename"?<input aria-label="新笔记名称" autoFocus maxLength={300} value={noteForm.title} onChange={e=>setNoteForm({...noteForm,title:e.target.value})}/>:<select aria-label="目标笔记本" value={noteForm.folder||""} onChange={e=>setNoteForm({...noteForm,folder:e.target.value||null})}><option value="">未分笔记本</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select>}<footer><button type="button" onClick={()=>setNoteForm(null)}>取消</button><button disabled={busy||!noteForm.title.trim()}>保存</button></footer></form></div>}
    {globalSearch&&<NoteGlobalSearch libraryId={libraryId} onClose={()=>setGlobalSearch(false)} onOpen={id=>{setGlobalSearch(false);void run(async()=>{await flush.current();setCurrent(await client.get(id));});}}/>}
    {projectPicker&&<NoteReferencePicker libraryId={libraryId} projectFilter onClose={()=>setProjectPicker(false)} onPick={value=>{setProjectFilter(value);setNavigationScope("project");setFolder(null);setFavorite(false);setTrash(false);setPage(0);setProjectPicker(false);}}/>}
    <WorkspacePanel label="知识导航" side="left" state={{...panels.left,width:leftWidth}} limits={leftLimits} onChange={changeLeft} tabs={[{id:"navigation",label:"笔记导航",icon:<BookOpen size={18}/>,content:<div className="knowledge-navigation-content">
    <aside className="knowledge-navigation"><header><BookOpen size={19}/><strong>知识库</strong></header>
      <LearningReview libraryId={libraryId} notify={notify} onOpen={id=>run(async()=>{await flush.current();setCurrent(await client.get(id));})}/>
      <NoteRecovery libraryId={libraryId} notify={notify} onOpen={id=>run(async()=>{await flush.current();setCurrent(await client.get(id));})}/>
      <button data-notebook-drop="" className={`${navigationScope==="all"&&!folder && !favorite && !trash ? "active" : ""} ${dropTarget&&dropTarget.id===null?"drop-inside":""}`} onClick={() => { setNavigationScope("all");setProjectFilter(null);setFolder(null); setFavorite(false); setTrash(false); setPage(0); }}>全部笔记</button>
      <button className={navigationScope==="inbox"?"active":""} onClick={()=>{setNavigationScope("inbox");setProjectFilter(null);setFolder(null);setFavorite(false);setTrash(false);setPage(0);}}>收件箱</button>
      <button className={navigationScope==="recent"?"active":""} onClick={()=>{setNavigationScope("recent");setProjectFilter(null);setFolder(null);setFavorite(false);setTrash(false);setPage(0);}}>最近更新</button>
      <button className={navigationScope==="project"?"active":""} onClick={()=>setProjectPicker(true)}>项目笔记…</button>
      <button className={favorite ? "active" : ""} onClick={() => { setNavigationScope("all");setProjectFilter(null);setFolder(null); setFavorite(true); setTrash(false); setPage(0); }}><Star size={15}/>收藏笔记</button>
      <button onClick={()=>setArchiveOpen(true)}><History size={15}/>原文归档</button>
      {archiveOpen&&<Suspense fallback={<LoadFeedback label="正在打开原文归档…"/>}><NoteArchiveBrowser libraryId={libraryId} onClose={()=>setArchiveOpen(false)}/></Suspense>}
      <button onClick={()=>setImportOpen(true)}><History size={15}/>学习资料迁入</button>
      {importOpen&&<Suspense fallback={<LoadFeedback label="正在打开学习资料迁入…"/>}><LearningImportPanel libraryId={libraryId} onClose={()=>setImportOpen(false)}/></Suspense>}
      <div className="knowledge-section"><span>笔记本</span><button title="新建笔记本" onClick={() => setFolderForm({ parent: null, name: "" })}><FolderPlus size={16}/></button></div>{tree(null)}
      <button className={`knowledge-trash ${trash ? "active" : ""}`} onClick={() => { setNavigationScope("all");setProjectFilter(null);setFolder(null); setFavorite(false); setTrash(true); setPage(0); }}><Trash2 size={15}/>笔记回收站</button>
    </aside>
    <section className="knowledge-list"><header><h2>{trash ? "笔记回收站" :navigationScope==="inbox"?"收件箱":navigationScope==="recent"?"最近更新":navigationScope==="project"?projectFilter?.label:"学习与创作"}</h2><p>{total} 篇笔记，保存在当前素材库</p>{navigationScope==="project"&&<p>仅筛选直接引用该项目的笔记，不会自动关联新笔记。</p>}</header>
      <div className="knowledge-search"><Search size={15}/><input aria-label="搜索笔记" placeholder="搜索标题、正文、标签" value={query} onChange={e => { setQuery(e.target.value); setPage(0); }}/></div>
      {!trash && <div className="knowledge-create"><select aria-label="笔记模板" value={template} onChange={e => setTemplate(e.target.value as NoteTemplate)}>{Object.entries(noteTemplates).map(([id,v]) => <option key={id} value={id}>{v.name}</option>)}</select><button className="primary-button" disabled={busy} onClick={()=>create()}><Plus size={15}/>新建</button><button disabled={busy} title="导入 Markdown 或笔记包" onClick={()=>run(async()=>{const path=await open({multiple:false,filters:[{name:"笔记",extensions:["md","dccnotes"]}]});if(!path)return;const result=await withKnowledgeBarrier(async()=>{if(path.toLowerCase().endsWith(".dccnotes"))return client.importPackage(path);const source=await client.readMarkdown(path);return client.importMarkdown(path,source.sha256,parseNoteMarkdown(source.text),folder);});setCurrent(result.note);setReload(v=>v+1);notify(result.warnings.length?result.warnings.join("；"):"笔记已导入");})}>导入</button></div>}
      {!trash&&<label className="note-sort-control">排序 <select aria-label="笔记排序" disabled={navigationScope!=="all"} value={effectiveManual?"manual":"updated"} onChange={e=>{layoutDirty.current=true;setManualOrder(e.target.value==="manual");setPage(0);}}><option value="updated">最近更新</option><option value="manual">手动排序</option></select></label>}
      {listFailed&&<ReadFailure onRetry={()=>setReload(v=>v+1)}/>}{loading&&!notes.length&&<LoadFeedback label="正在读取笔记…" skeleton/>}
      <div className="knowledge-note-list" aria-busy={loading} onContextMenu={e=>{if((e.target as Element).closest(".knowledge-note-card")||trash)return;e.preventDefault();setMenu({x:e.clientX,y:e.clientY,actions:[{label:"新建笔记",run:()=>create(folder,"blank")}]});}}>{notes.map(note => <button key={note.id} data-note-order={note.id} onPointerDown={e=>effectiveManual&&!trash?beginNoteReorder(e,note):beginPointerDrag(e,{kind:"note",id:note.id},note.title)} draggable={false} disabled={busy} style={note.color?{"--note-navigation-color":note.color} as CSSProperties:undefined} data-note-colored={!!note.color} className={`knowledge-note-card ${current?.id === note.id ? "active" : ""}`} onContextMenu={e=>noteMenu(e,note)} onClick={() => choose(note)}><strong>{note.favorite && "★ "}{note.title}</strong><span>{note.plainText.trim().slice(0,110) || "暂无正文"}</span><small>{new Date(note.updatedAt).toLocaleDateString()} {note.tags.slice(0,3).join(" · ")}</small></button>)}{!listFailed && !loading && !notes.length && <p className="knowledge-empty">{trash ? "没有已删除的笔记" : "记录一个知识点，让下次创作少走弯路。"}</p>}</div>
      <footer><button disabled={!page || loading} onClick={() => setPage(v => v - 1)}>上一页</button><span>{page + 1} / {Math.max(1,Math.ceil(total/40))}</span><button disabled={(page+1)*40 >= total || loading} onClick={() => setPage(v => v + 1)}>下一页</button></footer>
    </section></div>}]}/>
    <main className="knowledge-main"><nav className="knowledge-tabs" aria-label="打开的笔记"><div role="tablist">{tabs.map(t=><div key={t.id} style={t.color?{"--note-navigation-color":t.color} as CSSProperties:undefined} data-note-colored={!!t.color} className={`knowledge-tab ${current?.id===t.id?"active":""}`} draggable={false} data-note-tab={t.id} onPointerDown={e=>beginPointerDrag(e,{kind:"tab",id:t.id},t.title)} onContextMenu={e=>{e.preventDefault();setMenu({x:e.clientX,y:e.clientY,actions:[{label:"笔记颜色…",run:()=>targetAction(t.id,async n=>setColorForm({id:n.id,color:n.color||null}))},{label:t.pinned?"取消固定":"固定标签",run:()=>setTabs(v=>v.map(tab=>tab.id===t.id?{...tab,pinned:!tab.pinned}:tab))},{label:"关闭标签",run:()=>closeTab(t.id)}]});}}><button role="tab" aria-selected={current?.id===t.id} onClick={()=>run(async()=>{await flush.current();setCurrent(await client.get(t.id));})} title={t.title}>{t.pinned?"◆ ":""}{t.title}</button><button aria-label={`关闭标签 ${t.title}`} onClick={()=>closeTab(t.id)}><X size={12}/></button></div>)}</div><button title="重新打开关闭的标签" disabled={!closedTabs.length||busy} onClick={()=>run(async()=>{await flush.current();const t=closedTabs[closedTabs.length-1];const note=await client.get(t.id);setTabs(v=>[...v.filter(v=>v.id!==t.id),t]);setClosedTabs(v=>v.slice(0,-1));setCurrentState(note);})}>重新打开</button><button aria-pressed={focused} onClick={()=>{layoutDirty.current=true;setPanels(togglePanelFocus);}}>专注</button><button disabled={!appearanceReady} onClick={()=>setAppearanceBefore(appearance)}>外观</button></nav>
    {contextError&&<div className="knowledge-preference-error" role="alert">布局读取或保存失败：{contextError}<button onClick={()=>setContextAttempt(v=>v+1)}>重新读取布局</button></div>}
    {appearanceError&&<div className="knowledge-preference-error" role="alert">外观读取或保存失败：{appearanceError}<button onClick={()=>setAppearanceAttempt(v=>v+1)}>重新读取外观</button></div>}
    {current ? <NoteEditor panel={panels.right} onPanelChange={next=>{layoutDirty.current=true;setPanels(value=>setLayoutPanel(value,"right",next));}} key={`${current.id}-${current.revision}-${current.deletedAt || "active"}`} tab={tab} onTabChange={patch=>setTabs(v=>v.map(t=>t.id===current.id?{...t,...patch}:t))} libraryId={libraryId} historyRequested={historyTarget===current.id} onHistoryConsumed={()=>setHistoryTarget(null)} onOpenProject={onOpenProject} onLearnReference={onLearnReference} note={current} notebooks={notebooks} notify={notify} register={handler => { flush.current = handler; }} onNavigateNote={id=>run(async()=>{await flush.current();setCurrent(await client.get(id));})} onSaved={note => {setReload(v => v+1);if(note)setTabs(v=>v.map(t=>t.id===note.id?{...t,title:note.title}:t));}} onDeleted={note => { if(note?.deletedAt)setUndoDeleted(note);if(!note)setTabs(v=>v.filter(t=>t.id!==current.id));setCurrent(note); setReload(v=>v+1); }} /> : <section className="knowledge-editor-empty"><BookOpen size={42}/><h2>把学习变成可复用的经验</h2><p>从左侧选择笔记，或使用模板开始记录。</p><p>支持 Markdown 快捷输入、表格、清单与代码块。</p></section>}</main>
    {folderForm && <div className="modal-backdrop"><form className="modal-card hierarchy-name-dialog" role="dialog" aria-modal="true" aria-label={folderForm.id?"管理笔记本":"新建笔记本"} onSubmit={e => { e.preventDefault(); run(async () => { await client.upsertNotebook(folderForm.name, folderForm.parent,folderForm.id||null,folderForm.order??notebooks.filter(n=>n.parentId===folderForm.parent).length); setFolderForm(null); setReload(v => v + 1); }); }}><header><h2>{folderForm.id?"管理笔记本":folderForm.parent ? "新建子笔记本" : "新建笔记本"}</h2><button type="button" onClick={() => setFolderForm(null)}><X size={18}/></button></header><select aria-label="上级笔记本" value={folderForm.parent||""} onChange={e=>setFolderForm({...folderForm,parent:e.target.value||null})}><option value="">根级</option>{notebooks.filter(n=>n.id!==folderForm.id).map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select><input aria-label="笔记本名称" autoFocus maxLength={100} value={folderForm.name} onChange={e => setFolderForm({ ...folderForm, name: e.target.value })}/><footer>{folderForm.id&&<button type="button" disabled={busy} onClick={()=>run(async()=>{if(!window.confirm("删除这个空笔记本？含笔记或子笔记本时将拒绝删除。"))return;await client.deleteNotebook(folderForm.id!);if(folder===folderForm.id)setFolder(folderForm.parent);setFolderForm(null);setReload(v=>v+1);})}>删除空笔记本</button>}<button type="button" onClick={() => setFolderForm(null)}>取消</button><button className="primary-button" disabled={busy || !folderForm.name.trim()}>{folderForm.id?"保存":"创建"}</button></footer></form></div>}
  </div>;
}

function NoteEditor({ libraryId, note, panel,onPanelChange,tab,onTabChange,notebooks, notify, register, onSaved, onDeleted, onNavigateNote,onOpenProject,onLearnReference,historyRequested,onHistoryConsumed }: Props & {panel:PanelState;onPanelChange:(next:PanelState)=>void;tab?:NoteTab;onTabChange:(patch:Partial<NoteTab>)=>void; historyRequested:boolean;onHistoryConsumed:()=>void;libraryId: string; note: KnowledgeNote; notebooks: KnowledgeNotebook[]; register: (flush: () => Promise<void>) => void; onSaved: (note?:KnowledgeNote) => void; onDeleted: (note: KnowledgeNote | null) => void; onNavigateNote: (id: string) => void }) {
  const paused = useSyncExternalStore(subscribeKnowledgeBarrier,knowledgeChangesPaused);
  const client = useMemo(() => knowledgeApi(libraryId), [libraryId]);
  const [savedCount,setSavedCount]=useState(0);
  const [source,setSource]=useState<{kind:string;id:string}|null>(null);
  const [showReferences,setShowReferences]=useState(false);
  const [ai,setAi]=useState<{note:KnowledgeNote;selection:string}|null>(null);
  const [backlinks,setBacklinks]=useState<NoteSummary[]>([]);
  const [references,setReferences]=useState<KnowledgeReference[]>([]);
  const [sourcesRequested,setSourcesRequested]=useState(false),[sourcesLoading,setSourcesLoading]=useState(false),[sourcesError,setSourcesError]=useState(""),[sourceRetry,setSourceRetry]=useState(0);
  useEffect(()=>{if(panel.mode!=="closed"&&panel.tab==="sources")setSourcesRequested(true);},[panel.mode,panel.tab]);
  useEffect(()=>{if(!sourcesRequested)return;let active=true;setSourcesLoading(true);setSourcesError("");void Promise.allSettled([client.references(note.id),client.backlinks(note.id)]).then(([refs,links])=>{if(!active)return;if(refs.status==="fulfilled")setReferences(refs.value);if(links.status==="fulfilled")setBacklinks(links.value);setSourcesError([refs,links].flatMap(result=>result.status==="rejected"?[String(result.reason)]:[]).join("；"));setSourcesLoading(false);});return()=>{active=false;};},[client,note.id,note.revision,savedCount,sourcesRequested,sourceRetry]);
  const [tagsText,setTagsText]=useState(note.tags.join("; "));
  const [draft, setDraft] = useState<NoteInput>(()=>({ ...note, document:normalizeStructure(note.document), expectedRevision: note.revision }));
  const imports=useRef(new Set<Promise<unknown>>());
  const trackImport=useCallback(<T,>(work:Promise<T>):Promise<T>=>{imports.current.add(work);void work.then(()=>imports.current.delete(work),()=>imports.current.delete(work));return work;},[]);
  const draftRef = useRef(draft); const savedRef = useRef(note); const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [status, setStatus] = useState("已保存"); const [error, setError] = useState(""); const [readyState, setReady] = useState(false); const [recoveryPending,setRecoveryPending]=useState(true); const ready=readyState&&!recoveryPending;
  const [draftExported,setDraftExported]=useState(false);
  const [history, setHistory] = useState<NoteRevisionSummary[] | null>(null); const [preview, setPreview] = useState<NoteRevision | null>(null);
  const historyGeneration=useRef(0);const [historyError,setHistoryError]=useState("");
  const [historyBusy,setHistoryBusy]=useState(false);
  const [activeMap,setActiveMap]=useState<{id:string;data:NoteMapData}|null>(null);
  const reading=tab?.view==="read";const readingRef=useRef(reading);readingRef.current=reading;
  const layout=tab?.layout||"document";
  const [findOpen,setFindOpen]=useState(false);const [documentZoom,setDocumentZoom]=useState(1);
  const editorRoot=useRef<HTMLElement>(null);const capture=useRef<()=>void>(()=>{});
  useEffect(()=>{const host=editorRoot.current;if(!host)return;const report=(event:Event)=>notify((event as CustomEvent<string>).detail,true);host.addEventListener("note-structure-error",report);return()=>host.removeEventListener("note-structure-error",report);},[notify]);
  const onSavedRef = useRef(onSaved); onSavedRef.current = onSaved;
  const queue = useMemo(() => new NoteSaveQueue(note, client.save, value => { savedRef.current = value; setSavedCount(count=>count+1);onSavedRef.current(value); },client.stashRecovery?{stash:client.stashRecovery,clear:client.clearRecovery,failed:error=>{recordDiagnostic("save-failure");notify(`恢复副本保存失败：${String(error)}`,true)}}:undefined), [client, note.id]);
  const mapDraft=useRef<(()=>void)|null>(null);const flushingMap=useRef(false);
  const flush = useCallback(async () => {
    flushingMap.current=true;try{mapDraft.current?.();}finally{flushingMap.current=false;}
    if (timer.current) clearTimeout(timer.current);
    capture.current();
    await Promise.all([...imports.current]);
    if (!queue.isDirty()) return;
    setStatus("保存中…");
    try { await queue.flush(); setStatus("已保存"); setError(""); } catch (e) { setStatus("保存失败"); setError(String(e)); throw e; }
  }, [queue]);
  const change = useCallback((patch: Partial<NoteInput>) => {
    if(knowledgeChangesPaused() && imports.current.size===0&&!flushingMap.current)return;
    const value = { ...draftRef.current, ...patch }; draftRef.current = value; setDraft(value); queue.enqueue(value);setDraftExported(false); setStatus("待保存");
    if (timer.current) clearTimeout(timer.current); timer.current = setTimeout(() => { void flush().catch(() => {}); }, 600);
  }, [queue, flush]);
  const editor = useEditor({ extensions: [NoteTopicHierarchy, NoteFormattingShortcuts, NoteSearchHighlights, StarterKit.configure({ link: { openOnClick: false, protocols: ["https","http"] } }), TableKit, TaskList, TaskItem.configure({ nested: true }), ManagedNoteImage, NoteAttachmentNode, NoteReferenceNode, NoteRemoteImageNode, NoteMindMapNode,...learningNoteExtensions({readPreserved:id=>client.readPreserved(note.id,id),openReference:onLearnReference?(reference,index)=>onLearnReference(reference,index,note.id):undefined})], content: draft.document, editable: false, onUpdate: ({ editor }) => change({ document: editor.getJSON() }), editorProps: { handlePaste: (view,event)=>{if(!Array.from(event.clipboardData?.items||[]).some(item=>item.type.startsWith("image/")))return false;if(knowledgeChangesPaused()||!view.editable)return true;void trackImport(client.pasteImage(note.id).then(item=>{if(view.isDestroyed)return;const node=view.state.schema.nodes.image.create({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename});view.dispatch(view.state.tr.replaceSelectionWith(node));})).catch(e=>notify(String(e),true));return true;}, attributes: { class: "knowledge-prose", "aria-label": "笔记正文", role:"textbox", "aria-multiline":"true" }, handleDOMEvents: { click: (_view,event) => { const mapId=(event.target as Element).closest("[data-note-map]")?.getAttribute("data-note-map");if(mapId){event.preventDefault();_view.state.doc.descendants(node=>{if(node.type.name==="mindMap"&&node.attrs.id===mapId){try{setActiveMap({id:mapId,data:normalizeNoteMap(node.attrs.data)});}catch(e){notify(String(e),true);}}});return true;}const retry=(event.target as Element).closest("[data-retry-remote-image]")?.getAttribute("data-retry-remote-image");if(retry){event.preventDefault();void retryRemoteImage(retry).catch(e=>notify(String(e),true));return true;}const ref=(event.target as Element).closest("[data-note-reference]");if(ref?.getAttribute("data-reference-kind")==="note"){event.preventDefault();onNavigateNote(ref.getAttribute("data-note-reference")||"");return true;}if(ref){event.preventDefault();setSource({kind:ref.getAttribute("data-reference-kind")||"",id:ref.getAttribute("data-note-reference")||""});return true;}const attachment=(event.target as Element).closest("[data-note-attachment]")?.getAttribute("data-note-attachment"); if(attachment){event.preventDefault();void client.openAttachment(attachment).catch(e=>notify(String(e),true));return true;}const link=(event.target as Element).closest("a"); if(link) {event.preventDefault();const href=link.getAttribute("href");if(href&&/^https?:\/\//i.test(href)) void api.openExternal(href).catch(e=>notify(String(e),true));return true;}return false; } } } });
  useEffect(() => { register(flush); }, [flush, register]);
  useEffect(() => registerNoteSaveGuard(flush), [flush]);
  useEffect(() => {
    let active = true;
    const unlisten = listen("knowledge-close-requested", () => { void flush().then(() => invoke("finish_knowledge_close")).catch(e => notify(String(e),true)); });
    void invoke("set_knowledge_editing", { editing: true }).then(() => { if(active) setReady(true); }).catch(e=>notify(String(e),true));
    return () => { active=false; if(timer.current) clearTimeout(timer.current); void unlisten.then(v=>v()); void invoke("set_knowledge_editing", { editing: false }); };
  }, [flush,notify]);
  capture.current=()=>{if(editor&&!editor.isDestroyed)onTabChange({scroll:editorRoot.current?.querySelector(".knowledge-document")?.scrollTop||0,from:editor.state.selection.from,to:editor.state.selection.to});};
  useEffect(()=>{if(!editor||!ready)return;const size=editor.state.doc.content.size;editor.commands.setTextSelection({from:Math.min(size,tab?.from||1),to:Math.min(size,tab?.to||1)});const scroller=editorRoot.current?.querySelector(".knowledge-document");if(scroller)scroller.scrollTop=tab?.scroll||0;},[editor,ready]);
  useEffect(()=>{if(!editor)return;const copy=(event:ClipboardEvent)=>{if(editor.state.selection.empty)return;event.preventDefault();void copyNoteSelection(editor,libraryId,note.id,false).catch(e=>notify(String(e),true));};const cut=(event:ClipboardEvent)=>{if(!editor.isEditable){event.preventDefault();return;}if(editor.state.selection.empty)return;event.preventDefault();void copyNoteSelection(editor,libraryId,note.id,true).catch(e=>notify(String(e),true));};const paste=(event:ClipboardEvent)=>{if(!event.clipboardData?.getData("text/html").includes("data-stack-note-clip"))return;event.preventDefault();event.stopImmediatePropagation();if(editor.isEditable){const text=event.clipboardData?.getData("text/plain")||"";void trackImport(pasteNoteSelection(editor,libraryId,note.id).then(pasted=>{if(!pasted&&!editor.isDestroyed&&editor.isEditable)editor.chain().focus().insertContent(text.split(/\r?\n/).map(value=>({type:"paragraph",content:value?[{type:"text",text:value}]:[]}))).run();})).catch(e=>notify(String(e),true));}};const dom=editor.view.dom;dom.addEventListener("copy",copy);dom.addEventListener("cut",cut);dom.addEventListener("paste",paste,true);return()=>{dom.removeEventListener("copy",copy);dom.removeEventListener("cut",cut);dom.removeEventListener("paste",paste,true);};},[editor,libraryId,note.id,trackImport,notify]);
  useEffect(() => { editor?.setEditable(ready && !reading && !paused && !note.deletedAt && !preview, false); return subscribeKnowledgeBarrier(()=>editor?.setEditable(ready && !readingRef.current && !knowledgeChangesPaused() && !note.deletedAt && !preview,false)); }, [editor,reading,ready,paused,note.deletedAt,preview]);
  const action = (fn: () => Promise<void>) => { void fn().catch(e => notify(String(e),true)); };
  const retryRemoteImage=async(url:string)=>{
    if(!ready||paused||preview||note.deletedAt)throw new Error("请返回当前笔记的编辑状态后重试图片");
    await flush();notify("正在下载网络图片…");
    await trackImport((async()=>{const item=await client.retryImage(savedRef.current,url);if(!editor||editor.isDestroyed)return;
      const matches:Array<{position:number;size:number;alt:string}>=[];editor.state.doc.descendants((node,position)=>{if(node.type.name==="remoteImage"&&node.attrs.src===url)matches.push({position,size:node.nodeSize,alt:String(node.attrs.alt||item.filename)});});
      if(!matches.length)return;let transaction=editor.state.tr;for(const match of matches.reverse())transaction=transaction.replaceWith(match.position,match.position+match.size,editor.state.schema.nodes.image.create({src:`http://dcc-media.localhost/note/${item.id}`,alt:match.alt}));editor.view.dispatch(transaction);notify("图片已下载并保存到素材库");
    })());
  };
  const display = (revision: NoteRevision | null) => { setPreview(revision); editor?.commands.setContent(normalizeStructure(revision ? revision.snapshot.document : draftRef.current.document), { emitUpdate:false }); };
  const closeHistory=()=>{historyGeneration.current++;if(preview)display(null);setHistory(null);setHistoryBusy(false);if(panel.tab==="history"&&panel.mode!=="closed")onPanelChange({...panel,mode:"closed"});};
  const openHistory=async()=>{const generation=++historyGeneration.current;setHistory([]);setHistoryBusy(true);setHistoryError("");try{await flush();const items=await client.revisions(note.id);if(generation===historyGeneration.current)setHistory(items);}catch(error){if(generation===historyGeneration.current)setHistoryError(String(error));}finally{if(generation===historyGeneration.current)setHistoryBusy(false);}};
  const historyVisible=panel.mode!=="closed"&&panel.tab==="history";
  useEffect(()=>{if(!ready)return;if(historyVisible)void openHistory();else closeHistory();},[historyVisible,ready]);
  useEffect(()=>{if(historyRequested){onHistoryConsumed();onPanelChange({...panel,tab:"history",mode:panel.mode==="pinned"?"pinned":"peek"});}},[historyRequested]);
  const toolbarAction=(label:string)=>{const buttons=editor?.view.dom.closest(".knowledge-editor")?.querySelectorAll<HTMLButtonElement>(".knowledge-toolbar button");const button=Array.from(buttons||[]).find(v=>v.textContent===label);button?.click();};
  const switchView=()=>action(async()=>{await flush();closeHistory();onTabChange({view:reading?"edit":"read"});});
  useEffect(()=>{const shortcut=(event:KeyboardEvent)=>{if(event.defaultPrevented||event.repeat||event.isComposing||event.altKey||!ready||paused||document.querySelector(".modal-backdrop,.knowledge-context-menu"))return;if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="e"){event.preventDefault();event.stopPropagation();switchView();}};document.addEventListener("keydown",shortcut);return()=>document.removeEventListener("keydown",shortcut);});
  useEffect(()=>{const root=editorRoot.current;const toggle=()=>switchView();root?.addEventListener("note-switch-view",toggle);return()=>root?.removeEventListener("note-switch-view",toggle);});
  return <section ref={editorRoot} onKeyDownCapture={e=>{if(e.nativeEvent.isComposing||document.querySelector(".modal-backdrop"))return;const id=activeNoteShortcutAction(e.nativeEvent);if(!["noteFind","noteZoomIn","noteZoomOut","noteScreenshot"].includes(id||""))return;if((e.target as Element).closest("input,textarea,select")&&id!=="noteFind")return;e.preventDefault();e.stopPropagation();if(id==="noteFind")setFindOpen(true);else if(id==="noteScreenshot"){if(!reading&&!paused&&!preview&&!note.deletedAt)action(async()=>{await invoke("start_note_screenshot");notify("截取后返回笔记，用 Ctrl+V 插入截图");});}else if(layout==="document")setDocumentZoom(value=>Math.max(.5,Math.min(2,value+(id==="noteZoomIn"?.1:-.1))));else editor?.view.dom.dispatchEvent(new CustomEvent("note-zoom",{bubbles:true,detail:id==="noteZoomIn"?1:-1}));}} className={`knowledge-editor ${reading?"knowledge-reading":""}`} onKeyDown={e => { if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==="s") {e.preventDefault(); e.stopPropagation(); action(flush);} }}>
    {findOpen&&editor&&<NoteFindBar editor={editor} onClose={()=>setFindOpen(false)}/>}
    <div className="knowledge-view-switch"><button aria-pressed={!reading} disabled={!ready||paused} onClick={()=>{if(reading)switchView();}}>编辑</button><button aria-pressed={reading} disabled={!ready||paused} onClick={()=>{if(!reading)switchView();}}>阅读</button>{(["document","outline","mindMap"] as const).map((mode,i)=><button key={mode} aria-pressed={layout===mode} disabled={!ready||paused} onClick={()=>action(async()=>{await flush();closeHistory();onTabChange({layout:mode});})}>{["正文","大纲","导图"][i]}</button>)}<span>Ctrl+E 切换编辑/阅读</span></div>
    <NoteRecovery libraryId={libraryId} id={note.id} onOpen={onNavigateNote} notify={notify} onPending={setRecoveryPending}/>
    <header className="knowledge-editor-heading"><span className={error ? "knowledge-save-error" : "knowledge-save-status"} role="status">{status}</span><div><button disabled={!ready || paused || !!note.deletedAt} onClick={()=>change({favorite:!draft.favorite})} title="收藏笔记"><Star size={16} fill={draft.favorite ? "currentColor" : "none"}/></button><LearningReview libraryId={libraryId} noteId={note.id} notify={notify}/><button disabled={!ready} onClick={()=>action(flush)}><Save size={16}/>保存</button><select aria-label="导出笔记" value="" disabled={paused} onChange={e=>{const format=e.target.value;if(!format)return;action(async()=>{await flush();if(format==="pdf"){const latest=await client.get(note.id);const html=await invoke<string>("get_note_export_html",{libraryId,id:note.id,revision:latest.revision});await printNoteDocument(html);notify("请在打印窗口选择另存为 PDF");return;}const path=await save({defaultPath:`${draftRef.current.title.replace(/[<>:"/\\|?*]/g,"_")}.${format}`,filters:[{name:format==="md"?"Markdown（附件另存同目录）":"笔记包（当前版本与附件）",extensions:[format]}]});if(!path)return;await withKnowledgeBarrier(async()=>{const latest=await client.get(note.id);if(format==="md")await client.exportMarkdown(latest,serializeNoteMarkdown(latest.document),path);else if(format==="html"||format==="txt")await invoke("export_note_document",{libraryId,id:note.id,revision:latest.revision,format,path});else await client.exportPackage(note.id,path);});notify("已导出当前版本及附件；完整历史请使用整库备份");});}}><option value="">导出…</option><option value="md">Markdown＋附件</option><option value="pdf">PDF（打印另存）</option><option value="html">离线 HTML（内嵌图片）</option><option value="txt">纯文本 TXT</option><option value="dccnotes">结构化笔记包</option></select><button onClick={()=>action(async()=>{await flush();if(note.deletedAt || window.confirm("将笔记移入回收站？历史版本和附件会保留。")) onDeleted(await client.setDeleted(savedRef.current,!note.deletedAt));})}>{note.deletedAt ? "恢复笔记" : <Trash2 size={16}/>}</button>{note.deletedAt && <button onClick={()=>action(async()=>{if(window.confirm("永久删除这篇笔记、全部历史版本和附件？此操作无法撤销。")){await client.purge(savedRef.current);onDeleted(null);}})}>永久删除</button>}</div></header>
    {paused&&<p className="knowledge-history-banner">正在安全保存、备份或切换素材库，编辑暂时锁定。</p>}
    {error && <div className="knowledge-save-error" role="alert">{error}<button onClick={()=>action(flush)}>重试保存</button><button onClick={()=>action(async()=>{await Promise.all([...imports.current]);const path=await save({defaultPath:"未保存草稿.dccnotes",filters:[{name:"当前草稿与附件",extensions:["dccnotes"]}]});if(path){await client.exportDraft(draftRef.current,path);setDraftExported(true);notify("当前草稿已另存，源笔记未修改");}})}>另存当前草稿</button><button disabled={!draftExported} title="先另存草稿后可重新读取库中版本" onClick={()=>action(async()=>{if(!window.confirm("草稿已另存。重新读取素材库中的版本？"))return;if(timer.current)clearTimeout(timer.current);const latest=await client.get(note.id);queue.reset(latest);const fresh={...latest,expectedRevision:latest.revision};draftRef.current=fresh;savedRef.current=latest;setDraft(fresh);setTagsText(latest.tags.join("; "));editor?.commands.setContent(normalizeStructure(latest.document),{emitUpdate:false});setError("");setStatus("已保存");setDraftExported(false);onDeleted(latest);})}>重新读取库中版本</button></div>}
    {preview && <div className="knowledge-history-banner">正在预览历史版本 · {new Date(preview.createdAt).toLocaleString()}<button onClick={()=>display(null)}>返回当前版本</button><button onClick={()=>action(async()=>{const restored=await client.restore(savedRef.current,preview.id);onDeleted(restored);setPreview(null);setHistory(null);})}>恢复此版本</button></div>}
    <div className="knowledge-editor-body"><div className={`knowledge-document ${layout!=="document"?"knowledge-structured-document":""}`}><fieldset className="knowledge-inputs" disabled={reading}><input className="knowledge-title" aria-label="笔记标题" disabled={!ready || paused || !!note.deletedAt || !!preview} value={preview?.snapshot.title ?? draft.title} onChange={e=>change({title:e.target.value})}/>
      <div className="knowledge-metadata"><select aria-label="所属笔记本" value={draft.notebookId || ""} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>change({notebookId:e.target.value||null})}><option value="">未分笔记本</option>{notebooks.map(n=><option key={n.id} value={n.id}>{n.name}</option>)}</select><input aria-label="笔记标签" placeholder="标签，用分号分隔" value={tagsText} disabled={!ready || paused || !!note.deletedAt || !!preview} onChange={e=>{setTagsText(e.target.value);change({tags:e.target.value.split(/[;；]/)})}}/></div>
      {ready && !paused && !note.deletedAt && !preview && <div hidden={layout!=="document"} className="knowledge-toolbar" aria-label="笔记格式"><button onClick={()=>editor?.chain().focus().toggleBold().run()}>粗体</button><button onClick={()=>editor?.chain().focus().toggleItalic().run()}>斜体</button><button onClick={()=>editor?.chain().focus().toggleBulletList().run()}>列表</button><button onClick={()=>editor?.chain().focus().toggleTaskList().run()}>待办</button><button onClick={()=>editor?.chain().focus().toggleCodeBlock().run()}>代码</button><button onClick={()=>editor?.chain().focus().insertTable({rows:3,cols:3,withHeaderRow:true}).run()}>表格</button><button onClick={()=>{const id=crypto.randomUUID();const data=mapFromHeadings(editor?.getJSON()||{type:"doc"},draftRef.current.title);editor?.chain().focus().insertContent({type:"mindMap",attrs:{id,data}}).run();setActiveMap({id,data});}}>插入独立导图</button><button onClick={()=>action(async()=>{const paths=await open({multiple:true,filters:[{name:"笔记附件",extensions:["png","jpg","jpeg","webp","gif","pdf","txt","md"]}]});if(!paths)return;await flush();await trackImport((async()=>{for(const path of paths){const item=await client.importAttachment(note.id,path);if(item.mime.startsWith("image/"))editor?.chain().focus().setImage({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename}).run();else editor?.chain().focus().insertContent({type:"attachment",attrs:{attachmentId:item.id,label:item.filename}}).run();}})());})}>图片 / 附件</button><button onClick={()=>setShowReferences(true)}>插入引用</button><button onClick={()=>action(async()=>{await trackImport(client.pasteImage(note.id).then(item=>{editor?.chain().focus().setImage({src:`http://dcc-media.localhost/note/${item.id}`,alt:item.filename}).run();}));})}>粘贴截图</button><button onClick={()=>action(async()=>{const selection=editor ? editor.state.doc.textBetween(editor.state.selection.from,editor.state.selection.to,"\n") : "";await flush();setAi({note:await client.get(note.id),selection});})}>AI 辅助</button></div>}
      </fieldset>{activeMap&&<Suspense fallback={<div role="status">正在打开思维导图…</div>}><NoteMindMapDialog onSave={flush} onExport={async(format,data)=>{await flush();const path=await save({defaultPath:`思维导图.${format}`,filters:[{name:format.toUpperCase(),extensions:[format]}]});if(!path)return;const latest=await client.get(note.id);await invoke("export_note_map",{libraryId,id:note.id,revision:latest.revision,mapId:activeMap.id,format,png:format==="png"?await noteMapPng(data):null,path});notify("思维导图已导出");}} registerDraft={commit=>{mapDraft.current=commit;}} data={activeMap.data} readOnly={reading||paused||!!note.deletedAt||!!preview} onClose={()=>setActiveMap(null)} onChange={data=>{if(!editor||reading||!!preview||!!note.deletedAt||(!editor.isEditable&&!flushingMap.current))return;let position:number|null=null;editor.state.doc.descendants((node,pos)=>{if(node.type.name==="mindMap"&&node.attrs.id===activeMap.id&&JSON.stringify(node.attrs.data)!==JSON.stringify(data))position=pos;});if(position!==null)editor.view.dispatch(editor.state.tr.setNodeMarkup(position,undefined,{id:activeMap.id,data}));}}/></Suspense>}<div hidden={layout!=="document"}><NoteEditorTools panelOutline editor={editor} libraryId={libraryId} noteId={note.id} disabled={!ready||reading||paused||!!note.deletedAt||!!preview} onAi={()=>toolbarAction("AI 辅助")} onInsert={kind=>toolbarAction(kind==="attachment"?"图片 / 附件":kind==="screenshot"?"粘贴截图":"插入引用")}/><div style={{zoom:documentZoom}}><EditorContent editor={editor}/></div></div>{layout!=="document"&&editor&&<Suspense fallback={<p role="status">正在打开内容视图…</p>}><NoteStructureView editor={editor} document={preview?.snapshot.document||draft.document} title={preview?.snapshot.title||draft.title} layout={layout} disabled={!ready||reading||paused||!!note.deletedAt||!!preview} libraryId={libraryId} noteId={note.id} onInsert={kind=>toolbarAction(kind==="attachment"?"图片 / 附件":kind==="screenshot"?"粘贴截图":"插入引用")} onAi={()=>toolbarAction("AI 辅助")} trackImport={trackImport} notify={notify} onExport={format=>action(async()=>{await flush();const path=await save({defaultPath:`整篇笔记导图.${format}`,filters:[{name:format.toUpperCase(),extensions:[format]}]});if(!path)return;const latest=await client.get(note.id);await invoke("export_note_map",{libraryId,id:note.id,revision:latest.revision,mapId:"__document__",format,png:format==="png"?await noteSvgPng(await invoke<string>("get_note_structure_svg",{libraryId,id:note.id,revision:latest.revision})):null,path});notify("已导出整篇笔记结构");})}/></Suspense>}
    </div>
    <WorkspacePanel label="知识信息" side="right" state={panel} limits={infoLimits} onChange={onPanelChange} tabs={[
      {id:"outline",label:"标题大纲",icon:<ListTree size={18}/>,content:<NoteHeadingOutline editor={editor}/>},
      {id:"sources",label:"来源与反链",icon:<Link2 size={18}/>,content:<div className="knowledge-info-content">{sourcesLoading&&<LoadFeedback label="正在读取来源…"/>}{sourcesError&&<p role="alert">{sourcesError}<button onClick={()=>setSourceRetry(v=>v+1)}>重新读取来源</button></p>}<h3>这篇笔记引用的内容</h3>{references.map(v=>v.available?<button key={v.kind+v.targetId} onClick={()=>v.kind==="note"?onNavigateNote(v.targetId):setSource({kind:v.kind,id:v.targetId})}>{v.label}</button>:<p key={v.kind+v.targetId}>{v.label} · 来源暂不可用，引用保留</p>)}{!sourcesLoading&&!sourcesError&&!references.length&&<p>暂无引用。</p>}<h3>哪些笔记引用了这里</h3>{backlinks.map(v=><button key={v.id} onClick={()=>onNavigateNote(v.id)}>{v.title}</button>)}{!sourcesLoading&&!sourcesError&&!backlinks.length&&<p>暂无反链。</p>}</div>},
      {id:"tags",label:"标签信息",icon:<Tags size={18}/>,content:<div className="knowledge-info-content"><label>笔记标签<input aria-label="信息区标签" placeholder="用分号分隔" value={tagsText} disabled={!ready||reading||paused||!!note.deletedAt||!!preview} onChange={e=>{setTagsText(e.target.value);change({tags:e.target.value.split(/[;；]/)});}}/></label><p>与正文上方的标签同步，随笔记自动保存。</p></div>},
      {id:"history",label:"历史",icon:<History size={18}/>,content:<aside className="knowledge-history" aria-label="历史版本"><p>最近 50 个未固定版本；短时间编辑合并记录。</p>{historyError&&<p role="alert">{historyError}<button onClick={()=>void openHistory()}>重新读取历史</button></p>}{historyBusy?<p>正在读取历史版本…</p>:history?.length===0&&!historyError&&<p>首次修改后会保留历史版本。</p>}{(history||[]).map(v=><div key={v.id}><button onClick={()=>action(async()=>{const generation=historyGeneration.current;const revision=await client.revision(note.id,v.id);if(generation===historyGeneration.current)display(revision);})}>{v.title}<small>{new Date(v.createdAt).toLocaleString()}</small></button><button title={v.pinned?"取消固定":"固定版本"} onClick={()=>action(async()=>{const generation=historyGeneration.current;await client.pin(v.id,!v.pinned);const items=await client.revisions(note.id);if(generation===historyGeneration.current)setHistory(items);})}>{v.pinned?"已固定":"固定"}</button></div>)}{!!history?.length&&history.length%100===0&&<button onClick={()=>action(async()=>{const generation=historyGeneration.current;const more=await client.revisions(note.id,history.length);if(generation===historyGeneration.current)setHistory([...history,...more]);})}>加载更多历史</button>}</aside>},
    ]}/></div>
    {source&&<NoteSourcePreview {...source} onClose={()=>setSource(null)} onProject={onOpenProject?id=>action(async()=>{await flush();onOpenProject(id);}):undefined}/>}
    {ai&&<NoteAiDialog libraryId={libraryId} note={ai.note} selection={ai.selection} onClose={()=>setAi(null)} onNavigate={onNavigateNote} onApply={text=>{editor?.chain().focus().insertContentAt(editor.state.doc.content.size,{type:"paragraph",content:[{type:"text",text}]}).run();setAi(null);}}/>}
    {showReferences&&<NoteReferencePicker libraryId={libraryId} onClose={()=>setShowReferences(false)} onPick={v=>{editor?.chain().focus().insertContent({type:"reference",attrs:{kind:v.kind,targetId:v.targetId,label:v.label}}).run();setShowReferences(false);}}/>}

  </section>;
}
