import { BrowserCaptureInbox } from "./components/BrowserCaptureInbox";
import type { BrowserCaptureState } from "./types";
import { AutomaticBackupService } from "./components/ReliabilitySettings";
import { LoadFeedback, ReadFailure, WorkspaceBoundary } from "./components/LoadFeedback";
import { recordDiagnostic } from "./lib/diagnostics";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { Inbox, ArchiveRestore, AudioLines, BriefcaseBusiness, Box, ChevronDown, ChevronRight, Clock3, DatabaseBackup, Download, Film, FolderTree, Grid3X3, Heart, Image, Library, ListChecks, PanelLeftOpen, Plus, Search, Settings, ShieldCheck, Sparkles, Tags, Trash2, Upload, X } from "lucide-react";
import { api } from "./lib/api";
import { GlobalNoteSearch } from "./components/GlobalNoteSearch";
import { flushWorkspaceEdits } from "./lib/noteSaveGuard";
import {LearningWorkspace} from "./features/learning/LearningWorkspace";
import {LearningLibraryHome} from "./features/learning/LearningLibraryHome";
import LearningImportPanel from "./features/learning/LearningImportPanel";
import {learningApi} from "./features/learning/api";
import type {ExcerptReceipt} from "./features/learning/types";
const KnowledgeWorkspace = lazy(() => import("./components/KnowledgeWorkspace"));
import type { AssetCard, AssetDetail, AssetInput, BatchAssetUpdate, BatchTranslationReport, ContentLanguage, DeleteRequest, HealthSummary, ImportProgress, LibraryDragPayload, LibraryMeta, LinkCheckProgress, MediaKind, MoveCategoryRequest, MoveResult, ParsedShareText, PersonalizationState, SearchRequest, SmartCollection, SmartCollectionInput, SmartCollectionRule, ViewMode } from "./types";
import { CategoryTree, categoryDropZone, categoryMoveRequest } from "./components/CategoryTree";
import { FilterBar } from "./components/FilterBar";
import { AssetGrid } from "./components/AssetGrid";
import { DetailPanel } from "./components/DetailPanel";
import { AssetEditor } from "./components/AssetEditor";
import { ImportDialog } from "./components/ImportDialog";
import { FabLibraryDialog } from "./components/FabLibraryDialog";
import { importTask } from "./components/ImportTaskMap";
import { QuickAddDialog } from "./components/QuickAddDialog";
import { BatchToolbar } from "./components/BatchToolbar";
import { HealthPanel } from "./components/HealthPanel";
import { StorageSettings } from "./components/StorageSettings";
import {DownloadWorkspace} from "./features/download/DownloadWorkspace";
import {AssetScope,AssetBrowser} from "./features/assets/AssetBrowser";
import { beginEdgeScroll, beginPointerReorder,dragPreviewText, type BeginLibraryPointerDrag, type LibraryDragPoint } from "./lib/drag";
import { ReferenceBoardHub } from "./components/ReferenceBoardHub";
import { ReferenceBoardPicker } from "./components/ReferenceBoardPicker";
import { TagManager } from "./components/TagManager";
import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { SmartCollections } from "./components/SmartCollections";
import { TrashPanel } from "./components/TrashPanel";
import { ProjectHub,type ProjectLearningContext } from "./components/ProjectHub";
import { ProjectPicker } from "./components/ProjectPicker";
import { MediaLibraryView } from "./components/MediaLibraryView";
import { ResizableContextPane } from "./components/ResizableContextPane";
import { SearchHelpPopover } from "./components/SearchHelpPopover";
import { ContextMenu, type ContextMenuItem } from "./components/ContextMenu";
import { HierarchyNameDialog } from "./components/HierarchyNameDialog";
import { TranslationBatchReportDialog } from "./components/TranslationBatchReportDialog";
import { TranslationCostDialog, combineTranslationCosts } from "./components/TranslationCostDialog";
import type { TranslationCostEstimate } from "./types";
import { isAssetWorkspace, readContextPage, useRequestGate, useWorkspaceState } from "./lib/workspace";
import { SelectionSummary } from "./components/SelectionSummary";
import { AiAssistant } from "./components/AiAssistant";
import { ImageCompareProvider } from "./components/ImageCompare";
import "./components/Experience.css";
import type { AiLaunch } from "./lib/ai";
import { TaskCenter } from "./components/TaskCenter";
import { applyGlobalPreferences, defaultGlobalPreferences, defaultLibraryPreferences, enabledModules, moduleRegistry, shortcutMatches } from "./lib/personalization";

const emptyMeta: LibraryMeta = { categories: [], filters: { tags: [], dccTools: [], versions: [], formats: [], licenses: [] }, totalAssets: 0 };
const initialRequest: SearchRequest = {
  query: "", categoryIds: [], tags: [], tagIds: [], dccTools: [], versions: [], formats: [], licenses: [],
  favoriteOnly: false, recentOnly: false, healthIssue: null, smartCollectionId: null, projectId: null, projectAssetStatus: null, sort: "updated", offset: 0, limit: 80, contentLanguage: "zh-CN"
};

interface Toast { id: number; message: string; error: boolean; kind?: "move" | "delete"; actionLabel?: string; action?: () => Promise<void> }

export default function App() {
  const [aiLaunch, setAiLaunch] = useState<AiLaunch | undefined>();
  const [meta, setMeta] = useState<LibraryMeta>(emptyMeta);
  const [request, setRequest] = useState<SearchRequest>(initialRequest);
  const [queryInput, setQueryInput] = useState("");
  const [items, setItems] = useState<AssetCard[]>([]);
  const [total, setTotal] = useState(0);
  const [searchFailed,setSearchFailed]=useState(false);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<AssetDetail | null>(null);
  const [coreScope,setCoreScope]=useState<"normal"|"all">("normal");
  const [coreMediaId,setCoreMediaId]=useState<string|null>(null);
  const [learningTarget,setLearningTarget]=useState<{libraryId:string;courseId:string;mediaId:string;fromLearningHome?:boolean;projectId?:string;projectContext?:ProjectLearningContext;returnNoteId?:string;returnBoardId?:string;returnBoardItemId?:string;positionMs?:number;sourceFingerprint?:string}|null>(null);
  const [showLearningImport,setShowLearningImport]=useState(false);
  const [projectReturnMediaId,setProjectReturnMediaId]=useState<string|null>(null);
  const [projectReturnContext,setProjectReturnContext]=useState<ProjectLearningContext|undefined>();
  const learningGate=useRequestGate();
  const navigationBusy=useRef(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [editor, setEditor] = useState<"new" | "edit" | null>(null);
  const [showImport, setShowImport] = useState(false);
  const [showFabLibrary, setShowFabLibrary] = useState(false);
  const [showQuickAdd, setShowQuickAdd] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsTab,setSettingsTab]=useState<"appearance"|"download"|"online">("appearance");
  const [initialShare, setInitialShare] = useState<ParsedShareText | null>(null);
  const [libraryEpoch,setLibraryEpoch]=useState(0);
  const librarySaveEpoch = useRef(0);
  const [activeLibraryId, setActiveLibraryId] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    void api.getWorkspaceContext("library").then(value => { if (active) setActiveLibraryId(value.libraryId); }).catch(() => { if (active) setActiveLibraryId(null); });
    return () => { active = false; };
  }, [libraryEpoch]);
  const [knowledgeProjectId,setKnowledgeProjectId]=useState<string|null>(null);
  const [knowledgeInitialId,setKnowledgeInitialId]=useState<string|null>(null);
  const [navigation,setNavigation]=useWorkspaceState("navigation",{view:"library" as ViewMode},libraryEpoch);
  const view=navigation.view;
  const setView=async(value:ViewMode,onCommit?:()=>void)=>{
    if(navigationBusy.current)return false;
    navigationBusy.current=true;
    try{await flushWorkspaceEdits();learningGate.invalidate();setLearningTarget(null);onCommit?.();setNavigation({view:value});return true;}
    catch(error){notify(String(error),true);return false;}
    finally{navigationBusy.current=false;}
  };
  const [contextReady,setContextReady]=useState(false);
  const assetContextLocation=useRef<string|null>(null);
  const desiredScroll=useRef(0);
  const [history,setHistory]=useState<ViewMode[]>(["library"]);
  const [historyIndex,setHistoryIndex]=useState(0);
  const historyTraversal=useRef(false);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [health, setHealth] = useState<HealthSummary | null>(null);
  const [healthLoading, setHealthLoading] = useState(false);
  const [linkCheckRunning, setLinkCheckRunning] = useState(false);
  const [linkProgress, setLinkProgress] = useState<LinkCheckProgress | null>(null);
  const [checkingDetailLink, setCheckingDetailLink] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [contentLanguage, setContentLanguageState] = useState<ContentLanguage>("zh-CN");
  const [activeDrag, setActiveDrag] = useState<LibraryDragPayload | null>(null);
  const [detachedBoardId, setDetachedBoardId] = useState<string | null>(null);
  const [referencePicker, setReferencePicker] = useState<{ imageIds: string[]; title: string } | null>(null);
  const [projectPicker, setProjectPicker] = useState<string[] | null>(null);
  const [projectRefreshKey, setProjectRefreshKey] = useState(0);
  const [referenceInitialBoardId, setReferenceInitialBoardId] = useState<string | null>(null);
  const [referenceInitialItemId, setReferenceInitialItemId] = useState<string | null>(null);
  const [dragPoint, setDragPoint] = useState<LibraryDragPoint | null>(null);
  const [personalization, setPersonalization] = useState<PersonalizationState>({ global: defaultGlobalPreferences, library: defaultLibraryPreferences });
  const [smartCollections, setSmartCollections] = useState<SmartCollection[]>([]);
  const [activeSmartId, setActiveSmartId] = useState<string | null>(null);
  const [sourceSmartId, setSourceSmartId] = useState<string | null>(null);
  const [showCommandPalette, setShowCommandPalette] = useState(false);
  const [selectingAll, setSelectingAll] = useState(false);
  const [assetContextMenu, setAssetContextMenu] = useState<{ x: number; y: number; title: string; items: ContextMenuItem[] } | null>(null);
  const [categoryEditor, setCategoryEditor] = useState<{ id: string | null; parentId: string | null; name: string } | null>(null);
  const [savingCategory, setSavingCategory] = useState(false);
  const [translatingBatch, setTranslatingBatch] = useState(false);
  const costGate = useRequestGate();
  const [batchCost, setBatchCost] = useState<{ ids: string[]; libraryId: string; epoch: number; estimate: TranslationCostEstimate | null; loading: boolean; error: string } | null>(null);
  const cancelBatchCost = () => { costGate.invalidate(); setBatchCost(null); };
  useEffect(() => { costGate.invalidate(); setBatchCost(null); }, [libraryEpoch, activeLibraryId, costGate]);
  const estimateSelected = async (ids: string[], libraryId: string, epoch: number) => {
    const ticket = costGate.begin();
    setBatchCost({ ids, libraryId, epoch, estimate: null, loading: true, error: "" });
    try {
      const parts: TranslationCostEstimate[] = [];
      for (let start = 0; start < ids.length; start += 5000) {
        if (!costGate.isCurrent(ticket) || epoch !== librarySaveEpoch.current) return;
        parts.push(await api.estimateTranslationCost({ ids: ids.slice(start, start + 5000) }, libraryId));
      }
      const estimate = combineTranslationCosts(parts);
      if (costGate.isCurrent(ticket) && epoch === librarySaveEpoch.current) setBatchCost({ ids, libraryId, epoch, estimate, loading: false, error: "" });
    } catch (error) { if (costGate.isCurrent(ticket) && epoch === librarySaveEpoch.current) setBatchCost({ ids, libraryId, epoch, estimate: null, loading: false, error: String(error) }); }
  };
  const [translationProgress, setTranslationProgress] = useState<ImportProgress | null>(null);
  const [translationReport, setTranslationReport] = useState<BatchTranslationReport | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const requestVersion = useRef(0);
  const moveBusy = useRef(false);
  const pointerDragCleanup = useRef<(() => void) | null>(null);
  const detailReturnFocus = useRef<HTMLElement | null>(null);
  const detailGate = useRequestGate();
  useEffect(() => { detailGate.invalidate();setSelectedId(null);setDetail(null);setDetailLoading(false);setSelectionMode(false);setSelectedIds(new Set()); }, [view, detailGate]);

  const notify = useCallback((message: string, error = false, action?: { label: string; run: () => Promise<void> }, kind?: "move" | "delete") => {
    const id = Date.now() + Math.random();
    setToasts(previous => [...(kind ? previous.filter(item => item.kind !== kind) : previous), { id, message, error, kind, actionLabel: action?.label, action: action?.run }]);
    window.setTimeout(() => setToasts(previous => previous.filter(item => item.id !== id)), kind ? 8000 : 3500);
  }, []);
  const openLearning=async(mediaId:string,projectId?:string,projectContext?:ProjectLearningContext)=>{
    if(!activeLibraryId){notify("素材库尚未就绪，请稍后重试",true);return;}
    const ticket=learningGate.begin();
    try{
      await flushWorkspaceEdits();
      const course=await learningApi.ensureCourse(activeLibraryId,mediaId);
      if(learningGate.isCurrent(ticket))setLearningTarget({libraryId:activeLibraryId,courseId:course.id,mediaId,projectId,projectContext});
    }catch(error){if(learningGate.isCurrent(ticket))notify(String(error),true);}
  };
  const openNoteLearning=async(reference:import("./lib/learningNoteNodes").LearningReferenceData,index:number,noteId:string)=>{
    if(!activeLibraryId)throw Error("素材库尚未就绪");
    const ticket=learningGate.begin();
    await flushWorkspaceEdits();
    const jump=await learningApi.prepareNoteJump(activeLibraryId,reference,index);
    if(learningGate.isCurrent(ticket))setLearningTarget({libraryId:activeLibraryId,courseId:jump.courseId,mediaId:jump.mediaId,positionMs:jump.positionMs,sourceFingerprint:reference.fingerprint,returnNoteId:noteId});
  };
  const openBoardLearning=async(boardId:string,itemId:string)=>{
    if(!activeLibraryId)throw new Error("素材库尚未就绪");
    const ticket=learningGate.begin();const jump=await learningApi.prepareBoardExcerptJump(activeLibraryId,boardId,itemId);
    if(!jump)throw new Error("该图片没有关联视频学习来源");
    if(!learningGate.isCurrent(ticket))throw new Error("素材库或页面已切换，请重新选择图片");
    await flushWorkspaceEdits();
    if(!learningGate.isCurrent(ticket))throw new Error("素材库或页面已切换，请重新选择图片");
    setLearningTarget({libraryId:activeLibraryId,courseId:jump.courseId,mediaId:jump.mediaId,positionMs:jump.positionMs,sourceFingerprint:jump.sourceFingerprint,returnBoardId:boardId,returnBoardItemId:itemId});
  };
  const returnFromLearning=()=>{
    if(!learningTarget)return;
    if(learningTarget.fromLearningHome){setView("learningLibrary");}else if(learningTarget.returnNoteId){setKnowledgeInitialId(learningTarget.returnNoteId);setView("knowledge");}else if(learningTarget.returnBoardId){setReferenceInitialBoardId(learningTarget.returnBoardId);setReferenceInitialItemId(learningTarget.returnBoardItemId||null);setView("reference");}else if(learningTarget.projectId){setKnowledgeProjectId(learningTarget.projectId);setProjectReturnMediaId(learningTarget.mediaId);setProjectReturnContext(learningTarget.projectContext);}else setCoreMediaId(learningTarget.mediaId);
    setLearningTarget(null);
  };
  const openExcerptReceipt=async(receipt:ExcerptReceipt)=>{
    let opened=false;
    if(receipt.noteId)opened=await setView("knowledge",()=>setKnowledgeInitialId(receipt.noteId!));
    else if(receipt.boardId)opened=await setView("reference",()=>{setReferenceInitialBoardId(receipt.boardId!);setReferenceInitialItemId(receipt.boardItemId||null);});
    if(!opened)throw new Error("无法打开保存目标，请重试");
  };
  useEffect(()=>{learningGate.invalidate();setLearningTarget(null);setProjectReturnMediaId(null);},[libraryEpoch,learningGate]);

  const updateLibraryUi = useCallback((updater: (current: PersonalizationState["library"]) => PersonalizationState["library"]) => {
    setPersonalization(previous => {
      const library = updater(previous.library);
      if (library === previous.library) return previous;
      void api.saveLibraryPreferences(library).catch(error => notify(`界面设置保存失败：${String(error)}`, true));
      return { ...previous, library };
    });
  }, [notify]);

  const setContextPaneCollapsed = useCallback((id: string, collapsed: boolean) => {
    updateLibraryUi(current => ({ ...current, collapsedContextPanes: collapsed ? [...new Set([...current.collapsedContextPanes, id])] : current.collapsedContextPanes.filter(value => value !== id) }));
  }, [updateLibraryUi]);
  const setContextPaneWidth = useCallback((id: string, width: number) => {
    updateLibraryUi(current => ({ ...current, contextPaneWidths: { ...current.contextPaneWidths, [id]: width } }));
  }, [updateLibraryUi]);
  const toggleModuleGroup = useCallback((id: string) => {
    updateLibraryUi(current => ({ ...current, collapsedModuleGroups: current.collapsedModuleGroups.includes(id) ? current.collapsedModuleGroups.filter(value => value !== id) : [...current.collapsedModuleGroups, id] }));
  }, [updateLibraryUi]);

  const refreshMeta = useCallback(async () => {
    try { setMeta(await api.getMeta(contentLanguage)); } catch (error) { notify(`读取素材库失败：${String(error)}`, true); }
  }, [notify, contentLanguage]);
  const refreshSmartCollections = useCallback(async () => { try { setSmartCollections(await api.listSmartCollections()); } catch (error) { notify(`读取智能集合失败：${String(error)}`, true); } }, [notify]);

  const runSearch = useCallback(async (next: SearchRequest, append = false) => {
    const version = ++requestVersion.current; setLoading(true);setSearchFailed(false);
    try {
      const page = append ? await api.search(next) : await readContextPage(api.search,{...next,limit:Math.min(next.limit,200)},next.limit,()=>version===requestVersion.current);
      if (version !== requestVersion.current) return;
      setItems(prev => append ? [...prev, ...page.items.filter(item => !prev.some(old => old.id === item.id))] : page.items);
      setTotal(page.total);
    } catch (error) { if (version === requestVersion.current) {setSearchFailed(true);recordDiagnostic("read-failure");notify(`搜索失败：${String(error)}`, true);} }
    finally { if (version === requestVersion.current) setLoading(false); }
  }, [notify]);

  useEffect(() => {
    void api.getTranslationSettings().then(settings => { setContentLanguageState(settings.contentLanguage); setRequest(previous => ({ ...previous, contentLanguage: settings.contentLanguage, offset: 0 })); }).catch(() => undefined);
    void api.getPersonalization().then(value => { setPersonalization(value); applyGlobalPreferences(value.global); const savedContext = value.library.lastContext; const startup = value.library.rememberLastContext && savedContext ? savedContext.view : value.library.startupModule; const startupView = ["browserCapture","imageLibrary","modelLibrary","audioLibrary","videoLibrary","learningLibrary","projects","knowledge","aiAssistant","favorites","recent","tagManager","health","reference","trash"].includes(startup) ? startup as ViewMode : "library"; setView((value.library.rememberWorkspace && (value.library.workspaceContexts?.navigation as {view?:ViewMode})?.view) || startupView); if (value.library.rememberSearch && savedContext) { setQueryInput(savedContext.request.query); setRequest(previous => ({ ...initialRequest, ...savedContext.request, contentLanguage: previous.contentLanguage, offset: 0 })); } else setRequest(previous => ({ ...previous, sort: startupView === "recent" ? "recent" : value.library.defaultSort, favoriteOnly: startupView === "favorites", recentOnly: startupView === "recent" })); }).catch(() => undefined);
    void refreshSmartCollections();
  }, [refreshSmartCollections]);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<LinkCheckProgress>("share-link-check-progress", event => setLinkProgress(event.payload)).then(unlisten => { dispose = unlisten; });
    return () => dispose?.();
  }, []);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<ImportProgress>("batch-translation-progress", event => setTranslationProgress(event.payload)).then(unlisten => { dispose = unlisten; });
    return () => dispose?.();
  }, []);
  useEffect(() => {
    const disposes: Array<() => void> = [];
    void listen<string>("reference-window-opened", event => setDetachedBoardId(event.payload)).then(dispose => disposes.push(dispose));
    void listen<string>("reference-window-attached", event => { setDetachedBoardId(current => current === event.payload ? null : current); notify("参考板已收回主窗口"); }).then(dispose => disposes.push(dispose));
    return () => disposes.forEach(dispose => dispose());
  }, [notify]);
  useEffect(() => {
    if (!detachedBoardId) return;
    const reconcile = () => { void api.isReferenceWindowOpen().then(opened => { if (!opened) setDetachedBoardId(null); }).catch(() => undefined); };
    window.addEventListener("focus", reconcile);
    return () => window.removeEventListener("focus", reconcile);
  }, [detachedBoardId]);
  useEffect(() => { refreshMeta(); }, [refreshMeta]);
  useEffect(() => { const handler = (event: KeyboardEvent) => {
    const typing = Boolean((event.target as HTMLElement)?.closest("input,textarea,select,[contenteditable=true]"));
    if (document.querySelector(".modal-backdrop,.lightbox,.command-backdrop,.core-detail")) return;
    if (shortcutMatches(event, personalization.global.shortcuts.commandPalette || "Ctrl+P")) { event.preventDefault(); setShowCommandPalette(true); return; }
    if (typing || document.querySelector(".modal-backdrop,.lightbox,.command-backdrop")) return;
    if (shortcutMatches(event, personalization.global.shortcuts.focusSearch || "Ctrl+K")) { event.preventDefault(); document.querySelector<HTMLInputElement>("[data-workspace-search]")?.focus(); }
    else if (shortcutMatches(event, personalization.global.shortcuts.settings || "Ctrl+,")) { event.preventDefault(); setShowSettings(true); }
    else if (!isAssetWorkspace(view) || coreScope !== "normal") return;
    else if (shortcutMatches(event, personalization.global.shortcuts.addAsset || "Ctrl+N")) { event.preventDefault(); setInitialShare(null); setEditor("new"); }
    else if (shortcutMatches(event, personalization.global.shortcuts.quickAdd || "Ctrl+Shift+N")) { event.preventDefault(); setShowQuickAdd(true); }
    else if (shortcutMatches(event, personalization.global.shortcuts.settings || "Ctrl+,")) { event.preventDefault(); setShowSettings(true); }
    else if (shortcutMatches(event, personalization.global.shortcuts.toggleSelection || "Ctrl+M")) { event.preventDefault(); setSelectionMode(value => !value); setSelectedIds(new Set()); }
  }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, [personalization.global.shortcuts, view, coreScope]);
  useEffect(() => () => pointerDragCleanup.current?.(), []);
  useEffect(() => { if(!contextReady)return;const timer = window.setTimeout(() => {
    const changed = request.query !== queryInput;
    if (changed) setActiveSmartId(null);
    setRequest(prev => ({ ...prev, query: queryInput, smartCollectionId: changed ? null : prev.smartCollectionId, offset: 0, sort: queryInput.trim() && prev.sort === "updated" ? "relevance" : prev.sort }));
  }, 180); return () => clearTimeout(timer); }, [queryInput, request.query,contextReady]);
  useEffect(() => { if(!contextReady||!isAssetWorkspace(view))return;void runSearch({...request,limit:Math.max(request.limit,Math.min(20000,(Math.ceil(desiredScroll.current/80)+1)*6))}); }, [request, runSearch,contextReady,view]);
  useEffect(()=>{
    if(!isAssetWorkspace(view)){setContextReady(false);return}
    let active=true;setContextReady(false);assetContextLocation.current=null;
    void api.getWorkspaceContext(view).then(saved=>{
      if(!active)return;assetContextLocation.current=saved.enabled?saved.libraryId:null;
      const value=saved.enabled?saved.value as {request?:SearchRequest;scroll?:number}|null:null;
      const next={...initialRequest,...value?.request,contentLanguage,offset:0,favoriteOnly:view==="favorites",recentOnly:view==="recent"};
      desiredScroll.current=Math.max(0,value?.scroll||0);setRequest(next);setQueryInput(next.query);setContextReady(true);
    }).catch(()=>{if(active)setContextReady(true)});
    return()=>{active=false};
  },[view,libraryEpoch]);
  useEffect(()=>{if(!loading&&desiredScroll.current){const top=desiredScroll.current;desiredScroll.current=0;window.requestAnimationFrame(()=>{if(scrollRef.current)scrollRef.current.scrollTop=top})}},[loading,items]);
  useEffect(()=>{if(!contextReady||!isAssetWorkspace(view)||!assetContextLocation.current)return;const id=assetContextLocation.current;const timer=window.setTimeout(()=>void api.saveWorkspaceContext(id,view,{request:{...request,offset:0},scroll:scrollRef.current?.scrollTop||0}).catch(()=>undefined),500);return()=>window.clearTimeout(timer)},[request,contextReady,view]);
  useEffect(()=>{const node=scrollRef.current;if(!node||!contextReady||!isAssetWorkspace(view))return;let timer:number|undefined;const persist=()=>{const id=assetContextLocation.current;if(id)void api.saveWorkspaceContext(id,view,{request:{...request,offset:0},scroll:node.scrollTop}).catch(()=>undefined)};const changed=()=>{window.clearTimeout(timer);timer=window.setTimeout(persist,500)};node.addEventListener("scroll",changed);return()=>{window.clearTimeout(timer);persist();node.removeEventListener("scroll",changed)}},[view,contextReady,request]);
  useEffect(()=>{if(historyTraversal.current){historyTraversal.current=false;return}setHistory(previous=>{if(previous[historyIndex]===view)return previous;const next=[...previous.slice(0,historyIndex+1),view].slice(-50);setHistoryIndex(next.length-1);return next})},[view]);
  const traverseHistory=(delta:number)=>{const index=historyIndex+delta;if(index<0||index>=history.length)return;void setView(history[index],()=>{historyTraversal.current=true;setHistoryIndex(index);});};
  useEffect(()=>{const changed=(event:Event)=>{const kind=(event as CustomEvent<import("./types").LibraryTask>).detail?.payload.kind;if(kind==="restore"||kind==="migration"){void libraryChanged();return}void refreshAll();setProjectRefreshKey(value=>value+1)};window.addEventListener("library-task-completed",changed);return()=>window.removeEventListener("library-task-completed",changed)},[refreshMeta,refreshSmartCollections,runSearch,request]);
  useEffect(() => {
    if (!personalization.library.rememberSearch && !personalization.library.rememberLastContext) return;
    const timer = window.setTimeout(() => {
      const next = { ...personalization.library, lastContext: { view, request: { ...request, offset: 0 } } };
      void api.saveLibraryPreferences(next).catch(() => undefined);
    }, 650);
    return () => window.clearTimeout(timer);
  }, [request, view, personalization.library]);

  const patchRequest = (patch: Partial<SearchRequest>) => { setActiveSmartId(null); setRequest(prev => ({ ...prev, ...patch, ...(Object.hasOwn(patch, "tags") ? { tagIds: [] } : {}), smartCollectionId: null, offset: 0 })); };
  const changeView = (next: ViewMode) => setView(next,()=>{
    detailGate.invalidate();
    if(next!=="projects")setKnowledgeProjectId(null);
    const group = ["projects", "knowledge", "reference"].includes(next) ? "creation" : ["tagManager", "health", "trash"].includes(next) ? "manage" : "content";
    updateLibraryUi(current => current.collapsedModuleGroups.includes(group) ? { ...current, collapsedModuleGroups: current.collapsedModuleGroups.filter(value => value !== group) } : current);
    setActiveSmartId(null); setSourceSmartId(null);
    setSelectionMode(false); setSelectedIds(new Set());
    if (["browserCapture","imageLibrary","modelLibrary","audioLibrary","videoLibrary","learningLibrary","projects","knowledge","reference","tagManager","trash"].includes(next)) { setSelectedId(null); setDetail(null); return; }
    patchRequest({ favoriteOnly: next === "favorites", recentOnly: next === "recent", healthIssue: next === "health" ? request.healthIssue || null : null, sort: next === "recent" ? "recent" : request.query ? "relevance" : personalization.library.defaultSort });
    if (next === "health") refreshHealth();
  });
  const loadMore = useCallback(() => {
    if (searchFailed || loading || items.length >= total) return;
    runSearch({ ...request, offset: items.length }, true);
  }, [searchFailed,loading, items.length, total, request, runSearch]);

  const refreshHealth = async () => { setHealthLoading(true); try { setHealth(await api.getHealth()); } catch (error) { notify(String(error), true); } finally { setHealthLoading(false); } };

  const checkAllLinks = async () => {
    setLinkCheckRunning(true); setLinkProgress({ checked: 0, total: 0, valid: 0, invalid: 0, error: 0, currentUrl: null });
    try {
      const report = await api.checkAllShareLinks();
      const prefix = report.cancelled ? "检查已停止" : report.stoppedReason ? "检查提前停止" : "链接检查完成";
      notify(`${prefix}：有效 ${report.valid}，失效 ${report.invalid}，无法判断 ${report.error}${report.stoppedReason ? `。${report.stoppedReason}` : ""}`, Boolean(report.stoppedReason && !report.cancelled));
      await Promise.all([refreshHealth(), refreshAll()]);
    } catch (error) { notify(`网盘链接检查失败：${String(error)}`, true); }
    finally { setLinkCheckRunning(false); }
  };
  const cancelLinkCheck = async () => { try { await api.cancelShareLinkCheck(); } catch (error) { notify(String(error), true); } };
  const checkDetailLink = async () => {
    if (!detail) return;
    setCheckingDetailLink(true);
    try {
      const result = await api.checkAssetShareLink(detail.id);
      setDetail(await api.getAsset(detail.id, contentLanguage));
      setItems(previous => previous.map(item => item.id === detail.id ? { ...item, linkCheckStatus: result.status, linkCheckedAt: result.checkedAt, linkCheckMessage: result.message } : item));
      notify(result.status === "valid" ? "网盘链接有效" : result.status === "invalid" ? "网盘链接已失效" : result.message, result.status !== "valid");
      if (view === "health") await refreshHealth();
    } catch (error) { notify(`检查失败：${String(error)}`, true); }
    finally { setCheckingDetailLink(false); }
  };

  const closeAssetDetail = useCallback(() => {
    detailGate.invalidate(); setDetailLoading(false);
    setSelectedId(null); setDetail(null);
    window.requestAnimationFrame(() => detailReturnFocus.current?.focus());
  }, []);
  const selectAsset = async (id: string, trigger?: HTMLElement) => {
    const version = detailGate.begin();
    if (trigger) detailReturnFocus.current = trigger;
    setSelectedId(id); setDetailLoading(true); setDetail(null);
    try { const asset = await api.getAsset(id, contentLanguage); if (detailGate.isCurrent(version)) setDetail(asset); }
    catch (error) { if (detailGate.isCurrent(version)) { notify(String(error), true); setSelectedId(null); } }
    finally { if (detailGate.isCurrent(version)) setDetailLoading(false); }
  };
  const refreshAll = useCallback(async () => { await Promise.all([refreshMeta(), refreshSmartCollections(), runSearch({ ...request, offset: 0 })]); }, [refreshMeta, refreshSmartCollections, runSearch, request]);
  const selectAllResults = useCallback(async () => {
    setSelectingAll(true);
    try {
      const result = await api.selectAssetIds({ ...request, offset: 0, limit: 1 });
      setSelectedIds(new Set(result.ids));
      setSelectionMode(true);
      notify(`已选择当前条件下的 ${result.total.toLocaleString("zh-CN")} 项素材`);
    } catch (error) { notify(`全选失败：${String(error)}`, true); }
    finally { setSelectingAll(false); }
  }, [request, notify]);

  const deleteLibraryItems = useCallback(async (deleteRequest: DeleteRequest) => {
    try {
      const impact = await api.getDeleteImpact(deleteRequest);
      const detailText = impact.categoryCount ? `将删除 ${impact.categoryCount} 个分类及 ${impact.assetCount} 项素材。` : `将删除 ${impact.assetCount} 项素材。`;
      if (!window.confirm(`${detailText}\n内容会进入回收站，可随时恢复。是否继续？`)) return;
      const result = await api.deleteLibraryItems(deleteRequest);
      setSelectedId(null); setDetail(null); setSelectedIds(new Set());
      if (deleteRequest.categoryId) patchRequest({ categoryIds: [], healthIssue: null });
      await refreshAll();
      notify(`已移至回收站：${result.label}`, false, { label: "撤销", run: async () => { await api.restoreTrashBatch(result.batchId); await refreshAll(); notify("删除已撤销"); } }, "delete");
    } catch (error) { notify(`删除失败：${String(error)}`, true); }
  }, [notify, refreshAll]);
  const refreshAfterDrag = async () => {
    await refreshAll();
    if (selectedId) {
      try { setDetail(await api.getAsset(selectedId, contentLanguage)); }
      catch { setSelectedId(null); setDetail(null); }
    }
    if (view === "health") await refreshHealth();
  };
  const undoMove = async (token: string) => {
    try {
      const result = await api.undoLibraryMove(token);
      await refreshAfterDrag();
      notify(result.message);
    } catch (error) { notify(`撤销失败：${String(error)}`, true); }
  };
  const announceMove = (result: MoveResult) => {
    notify(result.message, false, result.undoToken ? { label: "撤销", run: () => undoMove(result.undoToken!) } : undefined, "move");
  };
  const moveAssetsByDrag = async (ids: string[], categoryId: string | null) => {
    if (moveBusy.current) return;
    moveBusy.current = true; setActiveDrag(null);
    try {
      const result = await api.moveAssetsToCategory(ids, categoryId);
      setSelectedIds(previous => { const next = new Set(previous); ids.forEach(id => next.delete(id)); return next; });
      announceMove(result); await refreshAfterDrag();
    } catch (error) { notify(`移动素材失败：${String(error)}`, true); }
    finally { moveBusy.current = false; }
  };
  const moveCategoryByDrag = async (moveRequest: MoveCategoryRequest) => {
    if (moveBusy.current) return;
    moveBusy.current = true; setActiveDrag(null);
    try { const result = await api.moveCategory(moveRequest); announceMove(result); await refreshAfterDrag(); }
    catch (error) { notify(`移动分类失败：${String(error)}`, true); }
    finally { moveBusy.current = false; }
  };
  const dropLibraryPayloadAt = (payload: LibraryDragPayload, x: number, y: number) => {
    const element = document.elementFromPoint(x, y);
    const row = element?.closest<HTMLElement>("[data-category-drop-id]");
    if (!row) return false;
    const rawTargetId = row.dataset.categoryDropId;
    if (!rawTargetId) return false;
    const targetId = rawTargetId === "__root__" ? null : rawTargetId;
    if (payload.kind === "assets") {
      void moveAssetsByDrag(payload.ids, targetId);
      return true;
    }
    const zone = categoryDropZone(row.getBoundingClientRect(), y, payload, targetId === null);
    const moveRequest = categoryMoveRequest(meta.categories, payload.id, targetId, zone);
    if (moveRequest) void moveCategoryByDrag(moveRequest);
    return Boolean(moveRequest);
  };
  const beginLibraryPointerDrag: BeginLibraryPointerDrag = (payload, event) => {
    if (event.button !== 0 || moveBusy.current) return;
    pointerDragCleanup.current?.();
    if(payload.kind==="category"){
      const id=(target:HTMLElement)=>target.dataset.categoryDropId==="__root__"?null:target.dataset.categoryDropId!;
      pointerDragCleanup.current=beginPointerReorder(event,{
        source:event.currentTarget.closest<HTMLElement>(".category-row")||event.currentTarget,
        selector:"[data-category-drop-id]",
        placement:(target,point)=>categoryDropZone(target.getBoundingClientRect(),point.y,payload,id(target)===null),
        canDrop:(target,zone)=>categoryMoveRequest(meta.categories,payload.id,id(target),zone)!==null,
        onStart:()=>setActiveDrag(payload),onFinish:()=>setActiveDrag(null),
        onDrop:(target,_after,zone)=>{const request=categoryMoveRequest(meta.categories,payload.id,id(target),zone);if(request)void moveCategoryByDrag(request);},
        onInvalid:()=>notify("这里不能放置：分类不能移入自身或子分类"),
      });return;
    }
    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    let dragging = false;
    const edgeScroll = beginEdgeScroll();
    let clickBlocker: ((event: MouseEvent) => void) | null = null;

    const cleanup = () => {
      edgeScroll.stop();
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", handleUp);
      window.removeEventListener("pointercancel", handleCancel);
      window.removeEventListener("keydown", handleKeyDown);
      document.body.classList.remove("library-pointer-dragging");
      pointerDragCleanup.current = null;
    };
    const suppressGeneratedClick = () => {
      clickBlocker = clickEvent => {
        clickEvent.preventDefault();
        clickEvent.stopPropagation();
        clickEvent.stopImmediatePropagation();
      };
      window.addEventListener("click", clickBlocker, { capture: true, once: true });
      window.setTimeout(() => {
        if (clickBlocker) window.removeEventListener("click", clickBlocker, { capture: true });
        clickBlocker = null;
      }, 100);
    };
    const finish = (commit: boolean, x = 0, y = 0) => {
      cleanup();
      setActiveDrag(null);
      setDragPoint(null);
      if (!dragging) return;
      suppressGeneratedClick();
      const accepted = commit && dropLibraryPayloadAt(payload, x, y);
      if (commit && !accepted) notify("这里不能放置：请选择有效分类，且不能移入自身或子分类");
    };
    const handleMove = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId !== pointerId) return;
      if (!dragging && Math.hypot(pointerEvent.clientX - startX, pointerEvent.clientY - startY) < 6) return;
      if (!dragging) {
        dragging = true;
        document.body.classList.add("library-pointer-dragging");
        setActiveDrag(payload);
      }
      pointerEvent.preventDefault();
      setDragPoint({ x: pointerEvent.clientX, y: pointerEvent.clientY });
      edgeScroll.update({ x: pointerEvent.clientX, y: pointerEvent.clientY });
    };
    const handleUp = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId === pointerId) finish(true, pointerEvent.clientX, pointerEvent.clientY);
    };
    const handleCancel = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId === pointerId) finish(false);
    };
    const handleKeyDown = (keyEvent: KeyboardEvent) => {
      if (keyEvent.key === "Escape") finish(false);
    };

    window.addEventListener("pointermove", handleMove, { passive: false });
    window.addEventListener("pointerup", handleUp);
    window.addEventListener("pointercancel", handleCancel);
    window.addEventListener("keydown", handleKeyDown);
    pointerDragCleanup.current = cleanup;
  };
  const saveAsset = async (input: AssetInput, options?: import("./types").SingleAssetSaveOptions) => {
    const epoch = libraryEpoch;
    try {
      if (epoch !== librarySaveEpoch.current) throw new Error("素材库已切换，请重新打开录入窗口");
      if (!activeLibraryId) throw new Error("正在连接素材库，请稍后重新保存");
      const saved = await api.saveAsset(input, activeLibraryId, options);
      if (epoch !== librarySaveEpoch.current) return saved;
      setSelectedId(saved.id); setDetail(saved);
      const warnings = saved.singleImportWarnings || [];
      notify(warnings.length ? `素材已保存，待处理：${warnings.join("；")}` : input.id ? "素材已更新" : "素材已添加", warnings.length > 0);
      window.dispatchEvent(new Event("library-task-started"));
      try { await refreshAll(); if (view === "health") await refreshHealth(); }
      catch (error) { notify(`素材已保存，但列表刷新失败：${String(error)}`, true); }
      return saved;
    }
    catch (error) { notify(`保存失败：${String(error)}`, true); throw error; }
  };
  const retrySingleTranslation = async (id: string) => {
    const epoch = libraryEpoch;
    if (!activeLibraryId) throw new Error("正在连接素材库，请稍后重试");
    const report = await api.batchTranslateMissingAssets([id], activeLibraryId);
    if (epoch !== librarySaveEpoch.current) throw new Error("素材库已切换，请在原素材库查看补译结果");
    const updated = await api.getAsset(id, contentLanguage);
    if (epoch !== librarySaveEpoch.current) throw new Error("素材库已切换");
    updated.singleImportWarnings = report.rows.find(row => row.assetId === id)?.messages || [];
    setDetail(previous => previous?.id === id ? updated : previous);
    void refreshAll().catch(error => notify(`补译结果已保存，但刷新失败：${String(error)}`, true));
    return updated;
  };
  const toggleFavorite = async (item: { id: string; favorite: boolean }) => {
    const favorite = !item.favorite;
    try { await api.setFavorite(item.id, favorite); setItems(prev => prev.map(value => value.id === item.id ? { ...value, favorite } : value)); setDetail(prev => prev?.id === item.id ? { ...prev, favorite } : prev); }
    catch (error) { notify(String(error), true); }
  };
  const deleteAsset = async () => { if (detail) await deleteLibraryItems({ assetIds: [detail.id], categoryId: null }); };

  const toggleSelection = (id: string, rangeIds?: string[]) => setSelectedIds(previous => {
    const next = new Set(previous);
    if (rangeIds?.length) rangeIds.forEach(value => next.add(value));
    else if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });
  const applyBatch = async (update: Omit<BatchAssetUpdate, "ids" | "contentLanguage">) => {
    const submittedIds=[...selectedIds];
    try {
      let updated=0;let identical=true;
      for(let start=0;start<submittedIds.length;start+=5000){const report=await api.batchUpdate({...update,ids:submittedIds.slice(start,start+5000),contentLanguage});updated+=report.updated;if(!report.identical)identical=false;}
      if(identical&&updated===0){notify("所选操作与当前值相同，不会更新记录");return "identical" as const;}
      notify(`已更新 ${updated} 项素材`); setSelectedIds(previous=>{const next=new Set(previous);for(const id of submittedIds)next.delete(id);return next;}); await refreshAll(); if (view === "health") refreshHealth();
      return "updated" as const;
    } catch (error) { notify(String(error), true); throw error; }
  };
  const translateSelected = async (requestedIds: string[], confirmed = false) => {
    const ids = [...new Set(requestedIds)];
    if (!ids.length || translatingBatch) return;
    const epoch = libraryEpoch;
    const boundLibrary = activeLibraryId;
    if (!confirmed) {
      if (!boundLibrary) { notify("正在连接素材库，请稍后重试", true); return; }
      await estimateSelected(ids, boundLibrary, epoch); return;
    }
    setTranslatingBatch(true);
    try {
      if (!boundLibrary) throw new Error("正在连接素材库，请稍后重试");
      setTranslatingBatch(true); setTranslationProgress(null); setTranslationReport(null);
      const reports: BatchTranslationReport[] = [];
      for (let start = 0; start < ids.length; start += 5000) {
        if (epoch !== librarySaveEpoch.current) throw new Error("素材库已切换，已停止提交补译");
        reports.push(await api.batchTranslateMissingAssets(ids.slice(start, start + 5000), boundLibrary));
      }
      if (epoch !== librarySaveEpoch.current) return;
      const report = { completed: reports.reduce((sum, item) => sum + item.completed, 0), pending: reports.reduce((sum, item) => sum + item.pending, 0), skipped: reports.reduce((sum, item) => sum + item.skipped, 0), rows: reports.flatMap(item => item.rows) };
      setTranslationReport(report);
      if (selectedId && ids.includes(selectedId)) {
        const updated = await api.getAsset(selectedId, contentLanguage);
        if (epoch === librarySaveEpoch.current) setDetail(previous => previous?.id === updated.id ? updated : previous);
      }
      await refreshAll(); if (view === "health") await refreshHealth();
      notify(`双语补译完成 ${report.completed} 项，待补译 ${report.pending} 项`);
    } catch (error) { notify(`补译失败：${String(error)}`, true); }
    finally { setTranslatingBatch(false); setTranslationProgress(null); }
  };
  const openAssetContextMenu = (item: AssetCard, x: number, y: number) => {
    const ids = selectedIds.has(item.id) ? [...selectedIds] : [item.id];
    const single = ids.length === 1;
    const title = single ? item.name : `${ids.length} 项已选素材`;
    const menuItems: ContextMenuItem[] = [
      { label: "打开详情", disabled: !single, run: () => void selectAsset(item.id) },
      { label: "编辑基本信息", disabled: !single, run: () => { void api.getAsset(item.id, contentLanguage).then(value => { setSelectedId(item.id); setDetail(value); setEditor("edit"); }).catch(error => notify(String(error), true)); } },
      { label: single && item.favorite ? "取消收藏" : `收藏${single ? "" : ` ${ids.length} 项`}`, run: () => { void (async () => { try { for (let start = 0; start < ids.length; start += 5000) await api.batchUpdate({ ids: ids.slice(start, start + 5000), categoryId: null, clearCategory: false, addTags: [], removeTags: [], favorite: single ? !item.favorite : true, contentLanguage }); await refreshAll(); } catch (error) { notify(String(error), true); } })(); } },
      { label: `加入项目${single ? "" : `（${ids.length} 项）`}`, run: () => setProjectPicker(ids) },
      { label: "加入参考板", run: () => { void api.prepareReferenceCoverIds(ids).then(imageIds => imageIds.length ? setReferencePicker({ imageIds, title }) : notify("所选素材没有可用预览图", true)).catch(error => notify(String(error), true)); } },
      { label: "在批量栏中移动分类…", run: () => { setSelectionMode(true); setSelectedIds(new Set(ids)); } },
      { label: "打开百度网盘", disabled: !single || !item.hasShareLink, run: () => { void api.openShare(item.id).catch(error => notify(String(error), true)); } },
      { label: `移入回收站${single ? "" : `（${ids.length} 项）`}`, danger: true, run: () => { void deleteLibraryItems({ assetIds: ids, categoryId: null }); } },
    ];
    setAssetContextMenu({ x, y, title, items: menuItems });
  };
  const addSelectionToReference = async () => {
    if (!selectedIds.size) return;
    try {
      const imageIds = await api.prepareReferenceCoverIds([...selectedIds]);
      if (!imageIds.length) { notify("选中的素材没有可用封面", true); return; }
      if (imageIds.length < selectedIds.size) notify(`有 ${selectedIds.size - imageIds.length} 项素材没有封面，已跳过`);
      setReferencePicker({ imageIds, title: `${selectedIds.size} 项素材` });
    } catch (error) { notify(`准备参考图失败：${String(error)}`, true); }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = Boolean(target?.closest("input,textarea,select,[contenteditable=true]"));
      if (typing || document.querySelector(".modal-backdrop,.lightbox,.command-backdrop,.core-detail") || !isAssetWorkspace(view) || coreScope !== "normal") return;
      if (selectionMode && shortcutMatches(event, personalization.global.shortcuts.selectAll || "Ctrl+A")) { event.preventDefault(); void selectAllResults(); return; }
      if (shortcutMatches(event, personalization.global.shortcuts.deleteSelected || "Delete")) {
        const categoryId = request.categoryIds.length === 1 ? request.categoryIds[0] : null;
        if (!selectedIds.size && !categoryId && !detail) return;
        event.preventDefault();
        void deleteLibraryItems({ assetIds: selectedIds.size ? [...selectedIds] : detail ? [detail.id] : [], categoryId: selectedIds.size || detail ? null : categoryId });
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [view, coreScope, selectionMode, selectedIds, request.categoryIds, detail, personalization.global.shortcuts, selectAllResults, deleteLibraryItems]);

  useEffect(() => {
    if (!selectedId || editor) return;
    const closeOnOutside = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target || target.closest(".detail-panel,.asset-card,.asset-list-item,.modal-backdrop,.reference-portal-menu,.popover-dismiss-layer,.app-context-menu")) return;
      setSelectedId(null); setDetail(null);
      window.requestAnimationFrame(() => detailReturnFocus.current?.focus());
    };
    window.addEventListener("pointerdown", closeOnOutside);
    return () => window.removeEventListener("pointerdown", closeOnOutside);
  }, [selectedId, editor]);

  const changeContentLanguage = async (language: ContentLanguage) => {
    const version=detailGate.begin();
    setContentLanguageState(language);
    setRequest(previous => ({ ...previous, contentLanguage: language, tags: [], tagIds: [], licenses: [], smartCollectionId: null, offset: 0 }));
    try {
      await api.setContentLanguage(language);
      if (selectedId) { const value=await api.getAsset(selectedId, language);if(detailGate.isCurrent(version))setDetail(value); }
    } catch (error) { notify(`保存语言偏好失败：${String(error)}`, true); }
  };

  const libraryChanged = async () => {
    setKnowledgeProjectId(null);setKnowledgeInitialId(null);
    librarySaveEpoch.current += 1;
    setActiveLibraryId(null); setEditor(null); setInitialShare(null); setShowQuickAdd(false);
    setLibraryEpoch(value=>value+1);setHistory(["library"]);setHistoryIndex(0);window.dispatchEvent(new Event("library-location-changed"));detailGate.invalidate();pointerDragCleanup.current?.(); setSelectedId(null); setDetail(null); setSelectedIds(new Set()); setSelectionMode(false); setTranslationReport(null); setActiveDrag(null); setDragPoint(null); setToasts(previous => previous.filter(item => item.kind !== "move")); setView("library"); setQueryInput("");
    const nextPersonalization = await api.getPersonalization(); setPersonalization(nextPersonalization); applyGlobalPreferences(nextPersonalization.global);
    const context = nextPersonalization.library.lastContext;
    const savedNavigation=nextPersonalization.library.workspaceContexts?.navigation as {view?:ViewMode}|undefined;
    const startup = (nextPersonalization.library.rememberWorkspace??true)&&savedNavigation?.view ? savedNavigation.view : nextPersonalization.library.rememberLastContext && context ? context.view : nextPersonalization.library.startupModule;
    const startupView = ["browserCapture","imageLibrary","modelLibrary","audioLibrary","videoLibrary","learningLibrary","projects","knowledge","aiAssistant","favorites","recent","tagManager","health","reference","trash"].includes(startup) ? startup as ViewMode : "library";
    setView(startupView);
    const nextRequest = nextPersonalization.library.rememberSearch && context ? { ...context.request, contentLanguage, offset: 0 } : { ...initialRequest, contentLanguage, sort: startupView === "recent" ? "recent" as const : nextPersonalization.library.defaultSort, favoriteOnly: startupView === "favorites", recentOnly: startupView === "recent" };
    setQueryInput(nextRequest.query);
    setRequest(nextRequest); await Promise.all([refreshMeta(), refreshSmartCollections(), runSearch(nextRequest)]);
  };
  const addCategory = (parentId: string | null) => setCategoryEditor({ id: null, parentId, name: "" });
  const renameCategory = (category: LibraryMeta["categories"][number]) => setCategoryEditor({ id: category.id, parentId: category.parentId, name: category.name });
  const saveCategory = async () => {
    if (!categoryEditor?.name.trim() || savingCategory) return;
    setSavingCategory(true);
    try {
      const { id, parentId, name } = categoryEditor;
      const savedId = id ? await api.renameCategory(id, name.trim()) : await api.addCategory(name.trim(), parentId);
      setCategoryEditor(null);
      if (id) await refreshAll();
      else { await refreshMeta(); setView("library"); patchRequest({ categoryIds: [savedId], favoriteOnly: false, recentOnly: false, healthIssue: null }); }
      notify(id ? "分类已重命名" : "分类已创建");
    } catch (error) { notify(String(error), true); }
    finally { setSavingCategory(false); }
  };
  const deleteCategory = async (category: LibraryMeta["categories"][number]) => {
    await deleteLibraryItems({ assetIds: [], categoryId: category.id });
  };

  const exportBackup = async () => {
    const filename = `栈藏备份-${new Date().toISOString().slice(0, 10)}.dccassetlib`;
    const path = await save({ defaultPath: filename, filters: [{ name: "栈藏素材库备份", extensions: ["dccassetlib"] }] });
    if (!path) return;
    try { await api.exportBackup(path); notify("素材库备份已导出"); } catch (error) { notify(`备份失败：${String(error)}`, true); }
  };
  const restoreBackup = async () => {
    const path = await open({ multiple: false, filters: [{ name: "栈藏素材库备份", extensions: ["dccassetlib"] }] });
    if (typeof path !== "string" || !window.confirm("恢复将替换当前素材库。应用会先创建安全副本，是否继续？")) return;
    try { await api.restoreBackup(path); await libraryChanged(); notify("素材库已恢复"); } catch (error) { notify(`恢复失败：${String(error)}`, true); }
  };

  const openSmartCollection = async (collection: SmartCollection) => {
    const tagNames: string[] = [];
    for (const locale of ["zh-CN", "en"] as ContentLanguage[]) {
      let offset = 0; let total = 1;
      while (offset < total) { const page = await api.listTags(locale, "", offset, 200); total = page.total; page.items.filter(tag => collection.rule.tagIds.includes(tag.id)).forEach(tag => tagNames.push(tag.name)); offset += page.items.length || 200; }
    }
    setView("library"); setSelectionMode(false); setSelectedIds(new Set()); setSelectedId(null); setDetail(null);
    setActiveSmartId(collection.id); setSourceSmartId(collection.id); setQueryInput(collection.rule.query);
    setRequest(previous => ({ ...previous, query: collection.rule.query, categoryIds: collection.rule.categoryIds, tags: tagNames, tagIds: collection.rule.tagIds, dccTools: collection.rule.dccTools, versions: collection.rule.versions, formats: collection.rule.formats, licenses: collection.rule.licenses, favoriteOnly: collection.rule.favoriteOnly, recentOnly: collection.rule.recentOnly, healthIssue: collection.rule.healthIssue, sort: collection.rule.sort, smartCollectionId: collection.id, offset: 0 }));
    if (collection.invalidConditions.length) notify(`集合条件已失效：${collection.invalidConditions.join("、")}`, true);
  };
  const currentSmartRule = async (): Promise<SmartCollectionRule> => {
    const tagIds: string[] = [...(request.tagIds || [])];
    if (!tagIds.length) for (const name of request.tags) { const page = await api.listTags(contentLanguage, name, 0, 50); const exact = page.items.find(item => item.name.toLowerCase() === name.toLowerCase()); if (exact) tagIds.push(exact.id); }
    return { query: request.query, categoryIds: request.categoryIds, tagIds, dccTools: request.dccTools, versions: request.versions, formats: request.formats, licenses: request.licenses, favoriteOnly: request.favoriteOnly, recentOnly: request.recentOnly, healthIssue: request.healthIssue || null, sort: request.sort };
  };
  const saveSmartCollection = async (existing?: SmartCollection) => {
    const name = window.prompt(existing ? "智能集合名称" : "保存当前搜索为智能集合", existing?.name || "新智能集合")?.trim(); if (!name) return;
    try {
      const input: SmartCollectionInput = { name, icon: existing?.icon || "sparkles", color: existing?.color || personalization.global.accentColor, rule: await currentSmartRule() };
      const saved = existing ? await api.updateSmartCollection(existing.id, input) : await api.createSmartCollection(input);
      await refreshSmartCollections(); setSourceSmartId(saved.id); setActiveSmartId(saved.id); setRequest(previous => ({ ...previous, smartCollectionId: saved.id })); notify(existing ? "智能集合已更新" : "智能集合已创建");
    } catch (error) { notify(String(error), true); }
  };
  const renameSmartCollection = async (collection: SmartCollection) => { const name = window.prompt("重命名智能集合", collection.name)?.trim(); if (!name || name === collection.name) return; try { await api.updateSmartCollection(collection.id, { name, icon: collection.icon, color: collection.color, rule: collection.rule }); await refreshSmartCollections(); } catch (error) { notify(String(error), true); } };
  const duplicateSmartCollection = async (collection: SmartCollection) => { try { await api.duplicateSmartCollection(collection.id); await refreshSmartCollections(); notify("智能集合已复制"); } catch (error) { notify(String(error), true); } };
  const deleteSmartCollection = async (collection: SmartCollection) => { if (!window.confirm(`删除智能集合“${collection.name}”？素材不会被删除。`)) return; try { await api.deleteSmartCollection(collection.id); if (sourceSmartId === collection.id) { setSourceSmartId(null); setActiveSmartId(null); } await refreshSmartCollections(); notify("智能集合已删除"); } catch (error) { notify(String(error), true); } };
  const reorderSmartCollections = async (ids: string[]) => { const previous = smartCollections; setSmartCollections(ids.map(id => previous.find(item => item.id === id)!).filter(Boolean)); try { await api.reorderSmartCollections(ids); } catch (error) { setSmartCollections(previous); notify(String(error), true); } };

  const personalizationChanged = (value: PersonalizationState) => {
    setPersonalization(value); applyGlobalPreferences(value.global);
    const disabled = value.library.disabledModules;
    const currentModule = view as string;
    if (disabled.includes(currentModule) || (view === "library" && activeSmartId && disabled.includes("smartCollections"))) changeView("library");
  };
  const collectAssetMedia = async (id: string) => {
    try {
      const result = await api.collectAssetPreviewsToMedia(id);
      const duplicateText = result.skipped ? `，跳过重复 ${result.skipped} 项` : "";
      notify(`已收录 ${result.imported} 项媒体${duplicateText}`, result.failed > 0);
    } catch (error) { notify(String(error), true); }
  };

  const paletteCommands = useMemo<PaletteCommand[]>(() => {
    const enabled = new Set(enabledModules(personalization.library));
    const commands: PaletteCommand[] = [
      { id: "search", label: "聚焦全局搜索", detail: "素材库", shortcut: personalization.global.shortcuts.focusSearch, run: () => { changeView("library"); window.setTimeout(() => searchRef.current?.focus(), 0); } },
      { id: "add", label: "添加素材", detail: "新建一条素材记录", shortcut: personalization.global.shortcuts.addAsset, run: () => { setInitialShare(null); setEditor("new"); } },
      { id: "quick", label: "快速录入", detail: importTask("quick").summary, shortcut: personalization.global.shortcuts.quickAdd, run: () => setShowQuickAdd(true) },
      { id: "import", label: "批量导入", detail: importTask("table").summary, run: () => setShowImport(true) },
      { id: "fabLibrary", label: "同步已购 Fab", detail: importTask("fabLibrary").summary, run: () => setShowFabLibrary(true) },
      { id: "settings", label: "打开设置中心", shortcut: personalization.global.shortcuts.settings, run: () => setShowSettings(true) },
      { id: "backup", label: "导出素材库备份", run: () => void exportBackup() },
    ];
    if (enabled.has("favorites")) commands.push({ id: "favorites", label: "前往我的收藏", run: () => changeView("favorites") });
    if (enabled.has("projects")) commands.push({ id: "projects", label: "打开创作项目", detail: "项目素材、镜头任务与工程入口", run: () => changeView("projects") });
    if (enabled.has("health")) commands.push({ id: "health", label: "素材库检查", detail: "缺失信息与链接检查", run: () => changeView("health") });
    if (enabled.has("tagManager")) commands.push({ id: "tags", label: "标签管理", run: () => changeView("tagManager") });
    if (enabled.has("aiAssistant")) commands.push({ id: "aiAssistant", label: "打开 AI 创作助手", run: () => openAi({ kind: "plan" }) });
    if (enabled.has("reference")) commands.push({ id: "reference", label: "打开参考板", run: () => changeView("reference") });
    if (enabled.has("trash")) commands.push({ id: "trash", label: "打开回收站", run: () => changeView("trash") });
    smartCollections.forEach(collection => commands.push({ id: `smart-${collection.id}`, label: `智能集合：${collection.name}`, detail: collection.invalidConditions.length ? "条件已失效" : "动态搜索", run: () => void openSmartCollection(collection) }));
    return commands;
  }, [personalization, smartCollections, request, view, activeSmartId]);

  const heading = activeSmartId ? smartCollections.find(item => item.id === activeSmartId)?.name || "智能集合" : view === "favorites" ? "我的收藏" : view === "recent" ? "最近查看" : view === "health" ? "素材库检查" : request.categoryIds.length ? meta.categories.find(c => c.id === request.categoryIds[0])?.name || "素材库" : "全部素材";
  const openAi = (launch: AiLaunch) => { setAiLaunch(launch); changeView("aiAssistant"); };
  const mediaKind: MediaKind | null = view === "imageLibrary" ? "image" : view === "modelLibrary" ? "model" : view === "audioLibrary" ? "audio" : view === "videoLibrary" ? "video" : null;
  useEffect(()=>{setCoreScope("normal");},[view,activeLibraryId]);
  const activeFilterText = useMemo(() => [request.tags, request.dccTools, request.versions, request.formats, request.licenses].flat().slice(0, 3), [request]);
  useEffect(()=>{
    let active=true;let dispose:(()=>void)|undefined;
    void listen<BrowserCaptureState>("browser-capture-state",event=>{
      if(!active)return;
      if(event.payload.submitted)window.dispatchEvent(new Event("library-task-started"));
      if(event.payload.open&&enabledModules(personalization.library).includes("browserCapture"))void changeView("browserCapture");
    }).then(fn=>{if(active)dispose=fn;else fn();});
    return()=>{active=false;dispose?.();};
  },[personalization.library,libraryEpoch]);
  const enabledIds = new Set(enabledModules(personalization.library));
  const orderedModules = (group: "content" | "creation" | "manage") => personalization.library.moduleOrder.filter(id => enabledIds.has(id) && moduleRegistry.find(item => item.id === id)?.group === group);
  const groupCollapsed = (group: string) => personalization.library.collapsedModuleGroups.includes(group);
  const mediaModules = orderedModules("content").filter(id => ["imageLibrary","modelLibrary","audioLibrary","videoLibrary"].includes(id));
  // Media libraries remain one collapsible bucket; its first member determines group placement.
  const contentNav=orderedModules("content").filter(id=>!mediaModules.includes(id)||id===mediaModules[0]);
  const renderNavItem = (id: string, nested = false) => {
    const values: Record<string, { label: string; icon: typeof Library; target: ViewMode }> = {
      downloader: { label: "下载中心", icon: Download, target: "downloader" }, browserCapture: { label: "采集收件箱", icon: Inbox, target: "browserCapture" }, library: { label: "素材库", icon: Library, target: "library" }, imageLibrary: { label: "图片", icon: Image, target: "imageLibrary" }, modelLibrary: { label: "三维模型", icon: Box, target: "modelLibrary" }, audioLibrary: { label: "音频", icon: AudioLines, target: "audioLibrary" }, videoLibrary: { label: "视频", icon: Film, target: "videoLibrary" },
      learningLibrary: { label: "学习库", icon: Library, target: "learningLibrary" }, knowledge: { label: "知识库", icon: Library, target: "knowledge" },
      aiAssistant: { label: "AI 创作助手", icon: Sparkles, target: "aiAssistant" }, projects: { label: "创作项目", icon: BriefcaseBusiness, target: "projects" }, reference: { label: "参考板", icon: Grid3X3, target: "reference" }, tagManager: { label: "标签管理", icon: Tags, target: "tagManager" }, health: { label: "素材库检查", icon: ShieldCheck, target: "health" }, trash: { label: "回收站", icon: Trash2, target: "trash" },
    };
    const item = values[id]; if (!item) return null; const Icon = item.icon;
    const hasContextPane = ["library", "imageLibrary", "modelLibrary", "audioLibrary", "videoLibrary"].includes(id);
    return <button key={id} className={`${view === item.target && !(id === "library" && activeSmartId) ? "active" : ""} ${nested ? "nested" : ""}`} onClick={() => {
      if (hasContextPane && view === item.target && !activeSmartId && !learningTarget) setContextPaneCollapsed(id, !personalization.library.collapsedContextPanes.includes(id));
      else changeView(item.target);
    }} title={hasContextPane && view === item.target ? "再次点击可展开或收起导航" : undefined}><Icon size={17} />{item.label}</button>;
  };

  return <ImageCompareProvider key={libraryEpoch}><div className="app-shell">
    <aside className="sidebar">
      <nav className="workspace-history" aria-label="浏览历史"><button aria-label="后退" disabled={historyIndex===0} onClick={()=>traverseHistory(-1)}>← 后退</button><button aria-label="前进" disabled={historyIndex>=history.length-1} onClick={()=>traverseHistory(1)}>前进 →</button></nav>
      <div className="brand"><div className="brand-mark"><ArchiveRestore size={21} /></div><div><strong>栈藏</strong><span>DCC ASSET LIBRARY</span></div></div>
      <div className="nav-section">
        <section className="nav-group"><button className="nav-group-header" onClick={() => toggleModuleGroup("content")}>{groupCollapsed("content") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}<span>内容</span></button>{!groupCollapsed("content") && <div className="nav-group-items">{contentNav.map(id=>id===mediaModules[0]?<div key="mediaLibraries"><button className={`nav-parent ${mediaKind ? "active-parent" : ""}`} onClick={() => toggleModuleGroup("mediaLibraries")}><FolderTree size={17} />媒体库<span className="nav-expander">{groupCollapsed("mediaLibraries") ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</span></button>{!groupCollapsed("mediaLibraries") && mediaModules.map(id => renderNavItem(id, true))}</div>:renderNavItem(id))}</div>}</section>
        <section className="nav-group"><button className="nav-group-header" onClick={() => toggleModuleGroup("creation")}>{groupCollapsed("creation") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}<span>创作</span></button>{!groupCollapsed("creation") && <div className="nav-group-items">{orderedModules("creation").map(id => renderNavItem(id))}</div>}</section>
        <section className="nav-group"><button className="nav-group-header" onClick={() => toggleModuleGroup("manage")}>{groupCollapsed("manage") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}<span>管理</span></button>{!groupCollapsed("manage") && <div className="nav-group-items">{orderedModules("manage").map(id => renderNavItem(id))}</div>}</section>
      </div>
      <div className="sidebar-footer"><TaskCenter notify={notify}/><button onClick={() => setShowSettings(true)}><Settings size={15} />设置中心</button><button onClick={exportBackup}><DatabaseBackup size={15} />导出整库备份</button><button onClick={restoreBackup}><Download size={15} />从备份恢复</button><span>本地素材库 · 离线可用</span></div>
    </aside>

    <main className={`main-content ${selectedId ? "with-detail" : ""} ${view === "reference" ? "reference-mode" : ""}`}><WorkspaceBoundary key={view}>
      {learningTarget?<LearningWorkspace key={`${learningTarget.libraryId}:${learningTarget.courseId}`} libraryId={learningTarget.libraryId} courseId={learningTarget.courseId} projectId={learningTarget.projectId} initialPositionMs={learningTarget.positionMs} sourceFingerprint={learningTarget.sourceFingerprint} backLabel={learningTarget.fromLearningHome?"返回学习库":learningTarget.returnNoteId?"返回笔记":learningTarget.returnBoardId?"返回参考板":undefined} onBack={returnFromLearning} onOpenReceipt={openExcerptReceipt} onOpenSettings={()=>{setSettingsTab("online");setShowSettings(true);}} onOpenNote={enabledIds.has("knowledge")?id=>{setLearningTarget(null);setKnowledgeInitialId(id);changeView("knowledge");}:undefined} notify={notify}/>:<>
      <AssetScope hidden={!isAssetWorkspace(view)} scope={coreScope} onChange={scope=>{setCoreScope(scope);closeAssetDetail();setSelectionMode(false);setSelectedIds(new Set());}} browser={<AssetBrowser key={libraryEpoch} libraryId={activeLibraryId||""} locale={contentLanguage} notify={notify} onOpenOrigin={(origin,type)=>{setCoreScope("normal");if(origin.kind==="legacy"){changeView("library");void selectAsset(origin.id);}else if(origin.kind==="media"){setCoreMediaId(origin.id);void changeView(type==="image"?"imageLibrary":type==="model"?"modelLibrary":type==="audio"?"audioLibrary":"videoLibrary");}}} onOpenNote={enabledIds.has("knowledge")?id=>{setKnowledgeInitialId(id);changeView("knowledge");}:undefined} onOpenProject={enabledIds.has("projects")?id=>{setKnowledgeProjectId(id);changeView("projects");}:undefined}/>}>
      {view === "learningLibrary" ? <LearningLibraryHome key={libraryEpoch} libraryId={activeLibraryId||""} onOrganize={()=>setShowLearningImport(true)} onOpen={(courseId,mediaId,positionMs)=>{if(activeLibraryId)setLearningTarget({libraryId:activeLibraryId,courseId,mediaId,positionMs,fromLearningHome:true});}} onOpenSettings={()=>{setSettingsTab("online");setShowSettings(true);}} onOpenNote={enabledIds.has("knowledge")?id=>{setKnowledgeInitialId(id);changeView("knowledge");}:undefined}/> : view === "downloader" ? <DownloadWorkspace key={libraryEpoch} notify={notify} onSettings={()=>{setSettingsTab("download");setShowSettings(true);}} onOpenLibrary={audio=>void changeView(audio?"audioLibrary":"videoLibrary")}/> : view === "browserCapture" ? <BrowserCaptureInbox key={libraryEpoch} notify={notify} canAddProject={enabledIds.has("projects")} canAddBoard={enabledIds.has("reference")} onOpenNote={enabledIds.has("knowledge")?id=>{setKnowledgeInitialId(id);changeView("knowledge");}:undefined}/> : view === "knowledge" ? <Suspense fallback={<LoadFeedback label="正在加载知识库…" skeleton/>}><KnowledgeWorkspace key={libraryEpoch} initialNoteId={knowledgeInitialId} onLearnReference={openNoteLearning} onOpenProject={enabledIds.has("projects")?id=>{setKnowledgeProjectId(id);changeView("projects");}:undefined} notify={notify}/></Suspense> : view === "aiAssistant" ? <AiAssistant key={libraryEpoch} settingsOpen={showSettings} launch={aiLaunch} language={contentLanguage} notify={notify} onChanged={() => { void refreshAll(); setProjectRefreshKey(v => v + 1); }} onSettings={() => setShowSettings(true)} /> : mediaKind ? <MediaLibraryView onLearn={(id)=>void openLearning(id)} key={`${libraryEpoch}-${mediaKind}`} shortcuts={personalization.global.shortcuts} onAi={enabledIds.has("aiAssistant") ? ids => openAi({ kind: "organize", mediaIds: ids }) : undefined} initialEntryId={coreMediaId} onInitialEntryOpened={()=>setCoreMediaId(null)} kind={mediaKind} onNotify={notify} paneWidth={personalization.library.contextPaneWidths[view] || defaultLibraryPreferences.contextPaneWidths[view] || 220} paneCollapsed={personalization.library.collapsedContextPanes.includes(view)} onPaneWidthChange={width => setContextPaneWidth(view, width)} onPaneCollapsedChange={collapsed => setContextPaneCollapsed(view, collapsed)} /> : view === "projects" ? <ProjectHub initialContext={projectReturnContext} onLearn={(id,projectId,context)=>void openLearning(id,projectId,context)} initialMediaId={projectReturnMediaId} onInitialMediaOpened={()=>setProjectReturnMediaId(null)} libraryId={activeLibraryId||""} initialProjectId={knowledgeProjectId} key={libraryEpoch} onAi={enabledIds.has("aiAssistant") ? id => openAi({ kind: "plan", projectId: id }) : undefined} contentLanguage={contentLanguage} notify={notify} refreshKey={projectRefreshKey} onOpenReference={boardId => { setReferenceInitialBoardId(boardId); setReferenceInitialItemId(null); setView("reference"); }} onOpenNote={enabledIds.has("knowledge")?id=>void setView("knowledge",()=>setKnowledgeInitialId(id)):undefined} onEditAsset={id=>{changeView("library");void api.getAsset(id,contentLanguage).then(asset=>{setSelectedId(id);setDetail(asset);setEditor("edit")}).catch(error=>notify(String(error),true))}} /> : view === "reference" ? <ReferenceBoardHub detachedBoardId={detachedBoardId} onDetached={setDetachedBoardId} notify={notify} initialBoardId={referenceInitialBoardId} initialItemId={referenceInitialItemId} onLearnSource={openBoardLearning} /> : view === "tagManager" ? <TagManager notify={notify} onChanged={refreshAll} /> : view === "trash" ? <TrashPanel notify={notify} onChanged={refreshAll} onOpenNote={id=>{setKnowledgeInitialId(id);changeView("knowledge");}} /> : <div className="asset-workspace-layout">
      <ResizableContextPane ariaLabel="素材分类" className="asset-context-pane" width={personalization.library.contextPaneWidths.library || 248} defaultWidth={248} collapsed={personalization.library.collapsedContextPanes.includes("library")} onResizeEnd={width => setContextPaneWidth("library", width)} onCollapsedChange={collapsed => setContextPaneCollapsed("library", collapsed)}>
        <div className="asset-context-heading"><strong>素材导航</strong><span>{meta.totalAssets.toLocaleString("zh-CN")} 项</span></div>
        <div className="asset-context-shortcuts">{enabledIds.has("favorites") && <button className={view === "favorites" ? "active" : ""} onClick={() => changeView("favorites")}><Heart size={15} />我的收藏</button>}{enabledIds.has("recent") && <button className={view === "recent" ? "active" : ""} onClick={() => changeView("recent")}><Clock3 size={15} />最近查看</button>}</div>
        <div className="context-section-title"><span>分类</span></div><div className="asset-category-scroll"><CategoryTree categories={meta.categories} selected={request.categoryIds} total={meta.totalAssets} activeDrag={activeDrag} defaultExpanded={personalization.library.categoryTreeExpanded} onChange={ids => { setView("library"); patchRequest({ categoryIds: ids, favoriteOnly: false, recentOnly: false, healthIssue: null }); }} onAdd={addCategory} onRename={renameCategory} onDelete={deleteCategory} onPointerDragStart={beginLibraryPointerDrag} /></div>
        {enabledIds.has("smartCollections") && <div className="asset-smart-collections"><SmartCollections items={smartCollections} activeId={activeSmartId} onOpen={openSmartCollection} onCreate={() => void saveSmartCollection()} onRename={item => void renameSmartCollection(item)} onDuplicate={item => void duplicateSmartCollection(item)} onDelete={item => void deleteSmartCollection(item)} onReorder={ids => void reorderSmartCollections(ids)} /></div>}
      </ResizableContextPane>
      <section className="asset-library-main">
      <section className="library-header"><div><span className="eyebrow">{activeSmartId ? "SMART COLLECTION" : "ASSET COLLECTION"}</span><h1>{heading}</h1><p>{loading ? "正在检索…" : `${total.toLocaleString("zh-CN")} 项素材`}{activeFilterText.length ? ` · ${activeFilterText.join(" / ")}` : ""}</p></div><div className="library-header-actions">{sourceSmartId && !activeSmartId && <button className="secondary-button" onClick={() => { const collection = smartCollections.find(item => item.id === sourceSmartId); if (collection) void saveSmartCollection(collection); }}><Sparkles size={15} />更新原集合</button>}<div className="asset-view-switch" role="group" aria-label="素材视图"><button type="button" aria-label="网格视图" aria-pressed={personalization.library.assetView === "grid"} onClick={() => updateLibraryUi(current => ({ ...current, assetView: "grid" }))}><Grid3X3 size={16} /></button><button type="button" aria-label="列表视图" aria-pressed={personalization.library.assetView === "list"} onClick={() => updateLibraryUi(current => ({ ...current, assetView: "list" }))}><ListChecks size={16} /></button></div><button className="secondary-button" onClick={() => void saveSmartCollection()}><Plus size={15} />另存为智能集合</button></div></section>
      <header className="topbar">{personalization.library.collapsedContextPanes.includes("library") && <button className="context-pane-open" onClick={() => setContextPaneCollapsed("library", false)} title="展开素材导航"><PanelLeftOpen size={17} /></button>}<div className="search-box"><Search size={18} /><input data-workspace-search ref={searchRef} value={queryInput} onChange={e => setQueryInput(e.target.value)} placeholder='高级搜索：tag:Nanite -format:FBX' />{queryInput && <button onClick={() => setQueryInput("")}><X size={15} /></button>}<SearchHelpPopover />{enabledIds.has("knowledge")&&<GlobalNoteSearch key={libraryEpoch} query={queryInput} onOpen={id=>{setKnowledgeInitialId(id);changeView("knowledge");}}/>}{enabledIds.has("aiAssistant")&&<button title="使用自然语言检索" onClick={()=>openAi({kind:"search",prompt:queryInput})}><Sparkles size={16}/></button>}<kbd>Ctrl K</kbd></div><div className="topbar-actions"><div className="content-language-toggle" aria-label="素材内容语言"><button className={contentLanguage === "zh-CN" ? "active" : ""} aria-pressed={contentLanguage === "zh-CN"} onClick={() => void changeContentLanguage("zh-CN")}>中文</button><button className={contentLanguage === "en" ? "active" : ""} aria-pressed={contentLanguage === "en"} onClick={() => void changeContentLanguage("en")}>English</button></div><button className={`secondary-button ${selectionMode ? "active" : ""}`} aria-pressed={selectionMode} onClick={() => { setSelectionMode(value => !value); setSelectedIds(new Set()); }}><ListChecks size={16} />多选</button><button className="secondary-button" title={importTask("fabLibrary").summary} onClick={() => setShowFabLibrary(true)}><Library size={16} />同步已购 Fab</button><button className="secondary-button" title={importTask("table").summary} onClick={() => setShowImport(true)}><Upload size={16} />批量导入</button><button className="primary-button" onClick={() => { setInitialShare(null); setEditor("new"); }}><Plus size={17} />添加素材</button></div></header>
      {view === "health" && <HealthPanel summary={health} activeIssue={request.healthIssue || null} loading={healthLoading} linkCheckRunning={linkCheckRunning} linkProgress={linkProgress} onSelect={issue => patchRequest({ healthIssue: issue })} onRefresh={refreshHealth} onCheckLinks={() => void checkAllLinks()} onCancelLinkCheck={() => void cancelLinkCheck()} />}
      <FilterBar request={request} options={meta.filters} onChange={patchRequest} />
      {selectionMode && <><SelectionSummary ids={selectedIds} filterKey={JSON.stringify(request)} getMatchingIds={async()=> (await api.selectAssetIds({...request,offset:0,limit:1})).ids}/><BatchToolbar onAi={enabledIds.has("aiAssistant") ? () => openAi({ kind: "organize", assetIds: [...selectedIds] }) : undefined} count={selectedIds.size} totalCount={total} selectingAll={selectingAll} selectionKey={[...selectedIds].sort().join("\n")} categories={meta.categories} onSelectAll={() => void selectAllResults()} onClear={() => setSelectedIds(new Set())} onExit={() => { setSelectionMode(false); setSelectedIds(new Set()); }} onAddToReference={() => void addSelectionToReference()} onAddToProject={() => selectedIds.size && setProjectPicker([...selectedIds])} onTranslate={() => void translateSelected([...selectedIds])} translating={translatingBatch} onApply={applyBatch} /></>}
      {searchFailed&&<ReadFailure onRetry={()=>void runSearch({...request,offset:0})}/>}
      <div className="grid-scroll" ref={scrollRef}><AssetGrid blocked={searchFailed} items={items} total={total} loading={loading} selectedId={selectedId} onSelect={selectAsset} onFavorite={toggleFavorite} onContextMenu={openAssetContextMenu} onLoadMore={loadMore} scrollRef={scrollRef} selectionMode={selectionMode} selectedIds={selectedIds} onToggleSelect={toggleSelection} onPointerDragStart={beginLibraryPointerDrag} preferences={personalization.library} query={request.query} /></div>
      </section></div>}
      </AssetScope>
      </>}
    </WorkspaceBoundary></main>

    {coreScope === "normal" && !mediaKind && view !== "learningLibrary" && view !== "downloader" && view !== "browserCapture" && view !== "knowledge" && view !== "aiAssistant" && view !== "projects" && view !== "reference" && view !== "tagManager" && view !== "trash" && <>{selectedId && <button className="inspector-backdrop" aria-label="关闭素材详情" onClick={closeAssetDetail} />}<DetailPanel onTranslate={id => void translateSelected([id])} translating={translatingBatch} onAi={enabledIds.has("aiAssistant") ? id => openAi({ kind: "organize", assetIds: [id] }) : undefined} asset={detail} contentLanguage={contentLanguage} loading={detailLoading} onClose={closeAssetDetail} onEdit={() => setEditor("edit")} onDelete={deleteAsset} onFavorite={() => detail && toggleFavorite(detail)} onOpen={async () => { if (!detail) return; try { await api.openShare(detail.id); setDetail(await api.getAsset(detail.id, contentLanguage)); } catch (error) { notify(String(error), true); } }} onSourceOpen={async () => { if (!detail?.sourceUrl) return; try { await api.openExternal(detail.sourceUrl); } catch (error) { notify(String(error), true); } }} onOpenExternal={async url => { try { await api.openExternal(url); } catch (error) { notify(String(error), true); } }} onCopy={async () => { if (!detail) return; try { await api.copyCode(detail.id); notify("提取码已复制"); } catch (error) { notify(String(error), true); } }} onCheckLink={() => void checkDetailLink()} checkingLink={checkingDetailLink} onAddReference={imageIds => setReferencePicker({ imageIds, title: detail?.name || "素材预览图" })} onAddProject={id => setProjectPicker([id])} onCollectMedia={collectAssetMedia} /></>}
    {editor && <AssetEditor expectedLibraryId={activeLibraryId || undefined} asset={editor === "edit" ? detail : null} contentLanguage={contentLanguage} initialShare={editor === "new" ? initialShare : null} categories={meta.categories} saveShortcut={personalization.global.shortcuts.saveAsset || "Ctrl+S"} onClose={() => { setEditor(null); setInitialShare(null); }} onSave={saveAsset} onRetryTranslation={retrySingleTranslation} onMediaImported={async id=>{setDetail(await api.getAsset(id,contentLanguage));await refreshAll();}} onOpenExisting={id => { setEditor(null); setInitialShare(null); void selectAsset(id); }} />}
    {showImport && <ImportDialog expectedLibraryId={activeLibraryId || undefined} onClose={() => setShowImport(false)} onImported={() => { void refreshAll(); if (view === "health") void refreshHealth(); }} notify={notify} />}
    {showFabLibrary && <FabLibraryDialog expectedLibraryId={activeLibraryId || undefined} onClose={() => setShowFabLibrary(false)} onImported={() => { void refreshAll(); if (view === "health") void refreshHealth(); }} notify={notify} />}
    {showLearningImport&&activeLibraryId&&<LearningImportPanel libraryId={activeLibraryId} onClose={()=>setShowLearningImport(false)}/>}
    {batchCost && <TranslationCostDialog estimate={batchCost.estimate} loading={batchCost.loading} error={batchCost.error} onCancel={cancelBatchCost} onRetry={() => void estimateSelected(batchCost.ids, batchCost.libraryId, batchCost.epoch)} onConfirm={async () => {
      if (batchCost.epoch !== librarySaveEpoch.current || batchCost.libraryId !== activeLibraryId || !batchCost.estimate || batchCost.loading || batchCost.error || batchCost.estimate.wouldExceed) return;
      const ids = batchCost.ids; cancelBatchCost(); setTranslationReport(null); await translateSelected(ids, true);
    }} />}
    {(translatingBatch || translationReport) && <TranslationBatchReportDialog report={translationReport} progress={translationProgress} busy={translatingBatch} onClose={() => setTranslationReport(null)} onRetry={ids => void translateSelected(ids)} />}
    {showQuickAdd && <QuickAddDialog onClose={() => setShowQuickAdd(false)} notify={notify} onParsed={parsed => { if (libraryEpoch !== librarySaveEpoch.current) return; setInitialShare(parsed); setShowQuickAdd(false); setEditor("new"); }} onOpenExisting={id => { if (libraryEpoch !== librarySaveEpoch.current) return; setShowQuickAdd(false); selectAsset(id); }} />}
    {categoryEditor && <HierarchyNameDialog kind="分类" editing={Boolean(categoryEditor.id)} name={categoryEditor.name} busy={savingCategory} onNameChange={name => setCategoryEditor(previous => previous && { ...previous, name })} onClose={() => setCategoryEditor(null)} onSubmit={() => void saveCategory()} />}
    {showSettings && <StorageSettings initialTab={settingsTab} onClose={() => {setShowSettings(false);setSettingsTab("appearance");}} onLibraryChanged={libraryChanged} notify={notify} libraryLocked={Boolean(detachedBoardId)} onPersonalizationChanged={personalizationChanged} />}
    {referencePicker && <ReferenceBoardPicker imageIds={referencePicker.imageIds} title={referencePicker.title} lockedBoardId={detachedBoardId} onClose={() => setReferencePicker(null)} notify={notify} />}
    {projectPicker && <ProjectPicker assetIds={projectPicker} onClose={() => setProjectPicker(null)} onAdded={() => setProjectRefreshKey(value => value + 1)} notify={notify} />}
    {activeDrag && dragPoint && <div className="library-drag-preview active" style={{ left: dragPoint.x + 14, top: dragPoint.y + 14 }}>{dragPreviewText(activeDrag)}</div>}
    {showCommandPalette && <CommandPalette commands={paletteCommands} onClose={() => setShowCommandPalette(false)} />}
    {assetContextMenu && <ContextMenu {...assetContextMenu} onClose={() => setAssetContextMenu(null)} />}
    <AutomaticBackupService notify={notify}/>
    <div className="toast-stack">{toasts.map(toast => <div key={toast.id} className={`toast ${toast.error ? "error" : ""}`}><span>{toast.message}</span>{toast.action && <button onClick={() => { setToasts(previous => previous.filter(item => item.id !== toast.id)); void toast.action?.(); }}>{toast.actionLabel}</button>}</div>)}</div>
  </div></ImageCompareProvider>;
}
