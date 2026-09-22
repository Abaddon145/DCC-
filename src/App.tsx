import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { ArchiveRestore, AudioLines, BriefcaseBusiness, Box, Clock3, DatabaseBackup, Download, Film, Grid3X3, Heart, Image, Library, ListChecks, Plus, Search, Settings, ShieldCheck, Sparkles, Tags, Trash2, Upload, X } from "lucide-react";
import { api } from "./lib/api";
import type { AssetCard, AssetDetail, AssetInput, BatchAssetUpdate, ContentLanguage, DeleteRequest, HealthSummary, LibraryDragPayload, LibraryMeta, LinkCheckProgress, MediaKind, MoveCategoryRequest, MoveResult, ParsedShareText, PersonalizationState, SearchRequest, SmartCollection, SmartCollectionInput, SmartCollectionRule, ViewMode } from "./types";
import { CategoryTree, categoryDropZone, categoryMoveRequest } from "./components/CategoryTree";
import { FilterBar } from "./components/FilterBar";
import { AssetGrid } from "./components/AssetGrid";
import { DetailPanel } from "./components/DetailPanel";
import { AssetEditor } from "./components/AssetEditor";
import { ImportDialog } from "./components/ImportDialog";
import { QuickAddDialog } from "./components/QuickAddDialog";
import { BatchToolbar } from "./components/BatchToolbar";
import { HealthPanel } from "./components/HealthPanel";
import { StorageSettings } from "./components/StorageSettings";
import { dragPreviewText, type BeginLibraryPointerDrag, type LibraryDragPoint } from "./lib/drag";
import { ReferenceBoardHub } from "./components/ReferenceBoardHub";
import { ReferenceBoardPicker } from "./components/ReferenceBoardPicker";
import { TagManager } from "./components/TagManager";
import { CommandPalette, type PaletteCommand } from "./components/CommandPalette";
import { SmartCollections } from "./components/SmartCollections";
import { TrashPanel } from "./components/TrashPanel";
import { ProjectHub } from "./components/ProjectHub";
import { ProjectPicker } from "./components/ProjectPicker";
import { MediaLibraryView } from "./components/MediaLibraryView";
import { applyGlobalPreferences, defaultGlobalPreferences, defaultLibraryPreferences, enabledModules, shortcutMatches } from "./lib/personalization";

const emptyMeta: LibraryMeta = { categories: [], filters: { tags: [], dccTools: [], versions: [], formats: [], licenses: [] }, totalAssets: 0 };
const initialRequest: SearchRequest = {
  query: "", categoryIds: [], tags: [], tagIds: [], dccTools: [], versions: [], formats: [], licenses: [],
  favoriteOnly: false, recentOnly: false, healthIssue: null, smartCollectionId: null, projectId: null, projectAssetStatus: null, sort: "updated", offset: 0, limit: 80, contentLanguage: "zh-CN"
};

interface Toast { id: number; message: string; error: boolean; kind?: "move" | "delete"; actionLabel?: string; action?: () => Promise<void> }

export default function App() {
  const [meta, setMeta] = useState<LibraryMeta>(emptyMeta);
  const [request, setRequest] = useState<SearchRequest>(initialRequest);
  const [queryInput, setQueryInput] = useState("");
  const [items, setItems] = useState<AssetCard[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<AssetDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [editor, setEditor] = useState<"new" | "edit" | null>(null);
  const [showImport, setShowImport] = useState(false);
  const [showQuickAdd, setShowQuickAdd] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [initialShare, setInitialShare] = useState<ParsedShareText | null>(null);
  const [view, setView] = useState<ViewMode>("library");
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
  const [dragPoint, setDragPoint] = useState<LibraryDragPoint | null>(null);
  const [personalization, setPersonalization] = useState<PersonalizationState>({ global: defaultGlobalPreferences, library: defaultLibraryPreferences });
  const [smartCollections, setSmartCollections] = useState<SmartCollection[]>([]);
  const [activeSmartId, setActiveSmartId] = useState<string | null>(null);
  const [sourceSmartId, setSourceSmartId] = useState<string | null>(null);
  const [showCommandPalette, setShowCommandPalette] = useState(false);
  const [selectingAll, setSelectingAll] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const requestVersion = useRef(0);
  const moveBusy = useRef(false);
  const pointerDragCleanup = useRef<(() => void) | null>(null);

  const notify = useCallback((message: string, error = false, action?: { label: string; run: () => Promise<void> }, kind?: "move" | "delete") => {
    const id = Date.now() + Math.random();
    setToasts(previous => [...(kind ? previous.filter(item => item.kind !== kind) : previous), { id, message, error, kind, actionLabel: action?.label, action: action?.run }]);
    window.setTimeout(() => setToasts(previous => previous.filter(item => item.id !== id)), kind ? 8000 : 3500);
  }, []);

  const refreshMeta = useCallback(async () => {
    try { setMeta(await api.getMeta(contentLanguage)); } catch (error) { notify(`读取素材库失败：${String(error)}`, true); }
  }, [notify, contentLanguage]);
  const refreshSmartCollections = useCallback(async () => { try { setSmartCollections(await api.listSmartCollections()); } catch (error) { notify(`读取智能集合失败：${String(error)}`, true); } }, [notify]);

  const runSearch = useCallback(async (next: SearchRequest, append = false) => {
    const version = ++requestVersion.current; setLoading(true);
    try {
      const page = await api.search(next);
      if (version !== requestVersion.current) return;
      setItems(prev => append ? [...prev, ...page.items.filter(item => !prev.some(old => old.id === item.id))] : page.items);
      setTotal(page.total);
    } catch (error) { if (version === requestVersion.current) notify(`搜索失败：${String(error)}`, true); }
    finally { if (version === requestVersion.current) setLoading(false); }
  }, [notify]);

  useEffect(() => {
    void api.getTranslationSettings().then(settings => { setContentLanguageState(settings.contentLanguage); setRequest(previous => ({ ...previous, contentLanguage: settings.contentLanguage, offset: 0 })); }).catch(() => undefined);
    void api.getPersonalization().then(value => { setPersonalization(value); applyGlobalPreferences(value.global); const savedContext = value.library.lastContext; const startup = value.library.rememberLastContext && savedContext ? savedContext.view : value.library.startupModule; const startupView = ["imageLibrary","modelLibrary","audioLibrary","videoLibrary","projects","favorites","recent","tagManager","health","reference","trash"].includes(startup) ? startup as ViewMode : "library"; setView(startupView); if (value.library.rememberSearch && savedContext) { setQueryInput(savedContext.request.query); setRequest(previous => ({ ...initialRequest, ...savedContext.request, contentLanguage: previous.contentLanguage, offset: 0 })); } else setRequest(previous => ({ ...previous, sort: startupView === "recent" ? "recent" : value.library.defaultSort, favoriteOnly: startupView === "favorites", recentOnly: startupView === "recent" })); }).catch(() => undefined);
    void refreshSmartCollections();
  }, [refreshSmartCollections]);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<LinkCheckProgress>("share-link-check-progress", event => setLinkProgress(event.payload)).then(unlisten => { dispose = unlisten; });
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
    if (shortcutMatches(event, personalization.global.shortcuts.commandPalette || "Ctrl+P")) { event.preventDefault(); setShowCommandPalette(true); return; }
    if (typing) return;
    if (shortcutMatches(event, personalization.global.shortcuts.focusSearch || "Ctrl+K")) { event.preventDefault(); searchRef.current?.focus(); }
    else if (shortcutMatches(event, personalization.global.shortcuts.addAsset || "Ctrl+N")) { event.preventDefault(); setInitialShare(null); setEditor("new"); }
    else if (shortcutMatches(event, personalization.global.shortcuts.quickAdd || "Ctrl+Shift+N")) { event.preventDefault(); setShowQuickAdd(true); }
    else if (shortcutMatches(event, personalization.global.shortcuts.settings || "Ctrl+,")) { event.preventDefault(); setShowSettings(true); }
    else if (shortcutMatches(event, personalization.global.shortcuts.toggleSelection || "Ctrl+M")) { event.preventDefault(); setSelectionMode(value => !value); setSelectedIds(new Set()); }
  }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, [personalization.global.shortcuts]);
  useEffect(() => () => pointerDragCleanup.current?.(), []);
  useEffect(() => { const timer = window.setTimeout(() => {
    const changed = request.query !== queryInput;
    if (changed) setActiveSmartId(null);
    setRequest(prev => ({ ...prev, query: queryInput, smartCollectionId: changed ? null : prev.smartCollectionId, offset: 0, sort: queryInput.trim() && prev.sort === "updated" ? "relevance" : prev.sort }));
  }, 180); return () => clearTimeout(timer); }, [queryInput, request.query]);
  useEffect(() => { runSearch(request); scrollRef.current?.scrollTo({ top: 0 }); }, [request, runSearch]);
  useEffect(() => {
    if (!personalization.library.rememberSearch && !personalization.library.rememberLastContext) return;
    const timer = window.setTimeout(() => {
      const next = { ...personalization.library, lastContext: { view, request: { ...request, offset: 0 } } };
      void api.saveLibraryPreferences(next).catch(() => undefined);
    }, 650);
    return () => window.clearTimeout(timer);
  }, [request, view, personalization.library]);

  const patchRequest = (patch: Partial<SearchRequest>) => { setActiveSmartId(null); setRequest(prev => ({ ...prev, ...patch, ...(Object.hasOwn(patch, "tags") ? { tagIds: [] } : {}), smartCollectionId: null, offset: 0 })); };
  const changeView = (next: ViewMode) => {
    setView(next);
    setActiveSmartId(null); setSourceSmartId(null);
    setSelectionMode(false); setSelectedIds(new Set());
    if (["imageLibrary","modelLibrary","audioLibrary","videoLibrary","projects","reference","tagManager","trash"].includes(next)) { setSelectedId(null); setDetail(null); return; }
    patchRequest({ favoriteOnly: next === "favorites", recentOnly: next === "recent", healthIssue: next === "health" ? request.healthIssue || null : null, sort: next === "recent" ? "recent" : request.query ? "relevance" : personalization.library.defaultSort });
    if (next === "health") refreshHealth();
  };
  const loadMore = useCallback(() => {
    if (loading || items.length >= total) return;
    runSearch({ ...request, offset: items.length }, true);
  }, [loading, items.length, total, request, runSearch]);

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

  const selectAsset = async (id: string) => {
    setSelectedId(id); setDetailLoading(true); setDetail(null);
    try { setDetail(await api.getAsset(id, contentLanguage)); } catch (error) { notify(String(error), true); setSelectedId(null); }
    finally { setDetailLoading(false); }
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
    if (!row) return;
    const rawTargetId = row.dataset.categoryDropId;
    if (!rawTargetId) return;
    const targetId = rawTargetId === "__root__" ? null : rawTargetId;
    if (payload.kind === "assets") {
      void moveAssetsByDrag(payload.ids, targetId);
      return;
    }
    const zone = categoryDropZone(row.getBoundingClientRect(), y, payload, targetId === null);
    const moveRequest = categoryMoveRequest(meta.categories, payload.id, targetId, zone);
    if (moveRequest) void moveCategoryByDrag(moveRequest);
  };
  const beginLibraryPointerDrag: BeginLibraryPointerDrag = (payload, event) => {
    if (event.button !== 0 || moveBusy.current) return;
    pointerDragCleanup.current?.();
    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    let dragging = false;
    let clickBlocker: ((event: MouseEvent) => void) | null = null;

    const cleanup = () => {
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
      if (commit) dropLibraryPayloadAt(payload, x, y);
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
  const saveAsset = async (input: AssetInput) => {
    try { const saved = await api.saveAsset(input); setSelectedId(saved.id); setDetail(saved); notify(input.id ? "素材已更新" : "素材已添加"); await refreshAll(); if (view === "health") refreshHealth(); return saved; }
    catch (error) { notify(`保存失败：${String(error)}`, true); throw error; }
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
    try {
      const report = await api.batchUpdate({ ...update, ids: [...selectedIds], contentLanguage });
      notify(`已更新 ${report.updated} 项素材`); setSelectedIds(new Set()); await refreshAll(); if (view === "health") refreshHealth();
    } catch (error) { notify(String(error), true); throw error; }
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
      if (typing || document.querySelector(".modal-backdrop") || view === "reference" || view === "tagManager" || view === "trash") return;
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
  }, [view, selectionMode, selectedIds, request.categoryIds, detail, personalization.global.shortcuts, selectAllResults, deleteLibraryItems]);

  useEffect(() => {
    if (!selectedId || editor) return;
    const closeOnOutside = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target || target.closest(".detail-panel,.asset-card,.asset-list-item,.modal-backdrop,.reference-portal-menu,.popover-dismiss-layer")) return;
      setSelectedId(null); setDetail(null);
    };
    window.addEventListener("pointerdown", closeOnOutside);
    return () => window.removeEventListener("pointerdown", closeOnOutside);
  }, [selectedId, editor]);

  const changeContentLanguage = async (language: ContentLanguage) => {
    setContentLanguageState(language);
    setRequest(previous => ({ ...previous, contentLanguage: language, tags: [], tagIds: [], licenses: [], smartCollectionId: null, offset: 0 }));
    try {
      await api.setContentLanguage(language);
      if (selectedId) setDetail(await api.getAsset(selectedId, language));
    } catch (error) { notify(`保存语言偏好失败：${String(error)}`, true); }
  };

  const libraryChanged = async () => {
    pointerDragCleanup.current?.(); setSelectedId(null); setDetail(null); setSelectedIds(new Set()); setSelectionMode(false); setActiveDrag(null); setDragPoint(null); setToasts(previous => previous.filter(item => item.kind !== "move")); setView("library"); setQueryInput("");
    const nextPersonalization = await api.getPersonalization(); setPersonalization(nextPersonalization); applyGlobalPreferences(nextPersonalization.global);
    const context = nextPersonalization.library.lastContext;
    const startup = nextPersonalization.library.rememberLastContext && context ? context.view : nextPersonalization.library.startupModule;
    const startupView = ["imageLibrary","modelLibrary","audioLibrary","videoLibrary","projects","favorites","recent","tagManager","health","reference","trash"].includes(startup) ? startup as ViewMode : "library";
    setView(startupView);
    const nextRequest = nextPersonalization.library.rememberSearch && context ? { ...context.request, contentLanguage, offset: 0 } : { ...initialRequest, contentLanguage, sort: startupView === "recent" ? "recent" as const : nextPersonalization.library.defaultSort, favoriteOnly: startupView === "favorites", recentOnly: startupView === "recent" };
    setQueryInput(nextRequest.query);
    setRequest(nextRequest); await Promise.all([refreshMeta(), refreshSmartCollections(), runSearch(nextRequest)]);
  };
  const addCategory = async (parentId: string | null) => {
    const name = window.prompt("分类名称"); if (!name?.trim()) return;
    try { await api.addCategory(name.trim(), parentId); await refreshMeta(); } catch (error) { notify(String(error), true); }
  };
  const renameCategory = async (category: LibraryMeta["categories"][number]) => {
    const name = window.prompt("新的分类名称", category.name); if (!name?.trim() || name.trim() === category.name) return;
    try { await api.renameCategory(category.id, name.trim()); await refreshAll(); } catch (error) { notify(String(error), true); }
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
    try { await api.restoreBackup(path); setSelectedId(null); setDetail(null); await refreshAll(); notify("素材库已恢复"); } catch (error) { notify(`恢复失败：${String(error)}`, true); }
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
      { id: "quick", label: "快速录入", detail: "解析百度网盘分享文本", shortcut: personalization.global.shortcuts.quickAdd, run: () => setShowQuickAdd(true) },
      { id: "import", label: "批量导入", detail: "CSV / XLSX", run: () => setShowImport(true) },
      { id: "settings", label: "打开设置中心", shortcut: personalization.global.shortcuts.settings, run: () => setShowSettings(true) },
      { id: "backup", label: "导出素材库备份", run: () => void exportBackup() },
    ];
    if (enabled.has("favorites")) commands.push({ id: "favorites", label: "前往我的收藏", run: () => changeView("favorites") });
    if (enabled.has("projects")) commands.push({ id: "projects", label: "打开创作项目", detail: "项目素材、镜头任务与工程入口", run: () => changeView("projects") });
    if (enabled.has("health")) commands.push({ id: "health", label: "素材库检查", detail: "缺失信息与链接检查", run: () => changeView("health") });
    if (enabled.has("tagManager")) commands.push({ id: "tags", label: "标签管理", run: () => changeView("tagManager") });
    if (enabled.has("reference")) commands.push({ id: "reference", label: "打开参考板", run: () => changeView("reference") });
    if (enabled.has("trash")) commands.push({ id: "trash", label: "打开回收站", run: () => changeView("trash") });
    smartCollections.forEach(collection => commands.push({ id: `smart-${collection.id}`, label: `智能集合：${collection.name}`, detail: collection.invalidConditions.length ? "条件已失效" : "动态搜索", run: () => void openSmartCollection(collection) }));
    return commands;
  }, [personalization, smartCollections, request, view, activeSmartId]);

  const heading = activeSmartId ? smartCollections.find(item => item.id === activeSmartId)?.name || "智能集合" : view === "favorites" ? "我的收藏" : view === "recent" ? "最近查看" : view === "health" ? "素材库检查" : request.categoryIds.length ? meta.categories.find(c => c.id === request.categoryIds[0])?.name || "素材库" : "全部素材";
  const mediaKind: MediaKind | null = view === "imageLibrary" ? "image" : view === "modelLibrary" ? "model" : view === "audioLibrary" ? "audio" : view === "videoLibrary" ? "video" : null;
  const activeFilterText = useMemo(() => [request.tags, request.dccTools, request.versions, request.formats, request.licenses].flat().slice(0, 3), [request]);

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><ArchiveRestore size={21} /></div><div><strong>栈藏</strong><span>DCC ASSET LIBRARY</span></div></div>
      <div className="nav-section">
        {enabledModules(personalization.library).map(id => {
          if (id === "smartCollections") return <SmartCollections key={id} items={smartCollections} activeId={activeSmartId} onOpen={openSmartCollection} onCreate={() => void saveSmartCollection()} onRename={item => void renameSmartCollection(item)} onDuplicate={item => void duplicateSmartCollection(item)} onDelete={item => void deleteSmartCollection(item)} onReorder={ids => void reorderSmartCollections(ids)} />;
          if (id === "library") return <button key={id} className={view === "library" && !activeSmartId ? "active" : ""} onClick={() => changeView("library")}><Library size={17} />素材库</button>;
          if (id === "imageLibrary") return <button key={id} className={view === id ? "active" : ""} onClick={() => changeView(id)}><Image size={17} />图片库</button>;
          if (id === "modelLibrary") return <button key={id} className={view === id ? "active" : ""} onClick={() => changeView(id)}><Box size={17} />三维模型库</button>;
          if (id === "audioLibrary") return <button key={id} className={view === id ? "active" : ""} onClick={() => changeView(id)}><AudioLines size={17} />音频库</button>;
          if (id === "videoLibrary") return <button key={id} className={view === id ? "active" : ""} onClick={() => changeView(id)}><Film size={17} />视频库</button>;
          if (id === "projects") return <button key={id} className={view === "projects" ? "active" : ""} onClick={() => changeView("projects")}><BriefcaseBusiness size={17} />创作项目</button>;
          if (id === "favorites") return <button key={id} className={view === "favorites" ? "active" : ""} onClick={() => changeView("favorites")}><Heart size={17} />我的收藏</button>;
          if (id === "recent") return <button key={id} className={view === "recent" ? "active" : ""} onClick={() => changeView("recent")}><Clock3 size={17} />最近查看</button>;
          if (id === "tagManager") return <button key={id} className={view === "tagManager" ? "active" : ""} onClick={() => changeView("tagManager")}><Tags size={17} />标签管理</button>;
          if (id === "health") return <button key={id} className={view === "health" ? "active" : ""} onClick={() => changeView("health")}><ShieldCheck size={17} />素材库检查</button>;
          if (id === "reference") return <button key={id} className={view === "reference" ? "active" : ""} onClick={() => changeView("reference")}><Grid3X3 size={17} />参考板</button>;
          if (id === "trash") return <button key={id} className={view === "trash" ? "active" : ""} onClick={() => changeView("trash")}><Trash2 size={17} />回收站</button>;
          return null;
        })}
      </div>
      {!mediaKind&&<><div className="sidebar-label"><span>分类</span></div><div className="category-scroll"><CategoryTree categories={meta.categories} selected={request.categoryIds} total={meta.totalAssets} activeDrag={activeDrag} defaultExpanded={personalization.library.categoryTreeExpanded} onChange={ids => { setView("library"); patchRequest({ categoryIds: ids, favoriteOnly: false, recentOnly: false, healthIssue: null }); }} onAdd={addCategory} onRename={renameCategory} onDelete={deleteCategory} onPointerDragStart={beginLibraryPointerDrag} /></div></>}
      <div className="sidebar-footer"><button onClick={() => setShowSettings(true)}><Settings size={15} />设置中心</button><button onClick={exportBackup}><DatabaseBackup size={15} />导出整库备份</button><button onClick={restoreBackup}><Download size={15} />从备份恢复</button><span>本地素材库 · 离线可用</span></div>
    </aside>

    <main className={`main-content ${selectedId ? "with-detail" : ""} ${view === "reference" ? "reference-mode" : ""}`}>
      {mediaKind ? <MediaLibraryView kind={mediaKind} onNotify={notify}/> : view === "projects" ? <ProjectHub contentLanguage={contentLanguage} notify={notify} refreshKey={projectRefreshKey} onOpenReference={boardId => { setReferenceInitialBoardId(boardId); setView("reference"); }} /> : view === "reference" ? <ReferenceBoardHub detachedBoardId={detachedBoardId} onDetached={setDetachedBoardId} notify={notify} initialBoardId={referenceInitialBoardId} /> : view === "tagManager" ? <TagManager notify={notify} onChanged={refreshAll} /> : view === "trash" ? <TrashPanel notify={notify} onChanged={refreshAll} /> : <>
      <header className="topbar"><div className="search-box"><Search size={18} /><input ref={searchRef} value={queryInput} onChange={e => setQueryInput(e.target.value)} placeholder='高级搜索：tag:Nanite -format:FBX' />{queryInput && <button onClick={() => setQueryInput("")}><X size={15} /></button>}<details className="search-help"><summary>?</summary><div><strong>高级搜索</strong><span>空格表示 AND，| 表示 OR，- 表示排除，双引号匹配完整短语。</span><code>name:"desert dune" tag:Nanite</code><code>software:Unreal -format:FBX</code><span>字段：name、tag、desc、category、author、software、version、format、license、source</span></div></details><kbd>Ctrl K</kbd></div><div className="content-language-toggle" aria-label="素材内容语言"><button className={contentLanguage === "zh-CN" ? "active" : ""} onClick={() => void changeContentLanguage("zh-CN")}>中文</button><button className={contentLanguage === "en" ? "active" : ""} onClick={() => void changeContentLanguage("en")}>English</button></div><button className={`secondary-button ${selectionMode ? "active" : ""}`} onClick={() => { setSelectionMode(value => !value); setSelectedIds(new Set()); }}><ListChecks size={16} />多选</button><button className="secondary-button" onClick={() => setShowImport(true)}><Upload size={16} />批量导入</button><button className="primary-button" onClick={() => { setInitialShare(null); setEditor("new"); }}><Plus size={17} />添加素材</button></header>
      <section className="library-header"><div><span className="eyebrow">{activeSmartId ? "SMART COLLECTION" : "ASSET COLLECTION"}</span><h1>{heading}</h1><p>{loading ? "正在检索…" : `${total.toLocaleString("zh-CN")} 项素材`}{activeFilterText.length ? ` · ${activeFilterText.join(" / ")}` : ""}</p></div>{sourceSmartId && !activeSmartId && <button className="secondary-button" onClick={() => { const collection = smartCollections.find(item => item.id === sourceSmartId); if (collection) void saveSmartCollection(collection); }}><Sparkles size={15} />更新原集合</button>}<button className="secondary-button" onClick={() => void saveSmartCollection()}><Plus size={15} />另存为智能集合</button></section>
      {view === "health" && <HealthPanel summary={health} activeIssue={request.healthIssue || null} loading={healthLoading} linkCheckRunning={linkCheckRunning} linkProgress={linkProgress} onSelect={issue => patchRequest({ healthIssue: issue })} onRefresh={refreshHealth} onCheckLinks={() => void checkAllLinks()} onCancelLinkCheck={() => void cancelLinkCheck()} />}
      <FilterBar request={request} options={meta.filters} onChange={patchRequest} />
      {selectionMode && <BatchToolbar count={selectedIds.size} totalCount={total} selectingAll={selectingAll} categories={meta.categories} onSelectAll={() => void selectAllResults()} onClear={() => setSelectedIds(new Set())} onExit={() => { setSelectionMode(false); setSelectedIds(new Set()); }} onAddToReference={() => void addSelectionToReference()} onAddToProject={() => selectedIds.size && setProjectPicker([...selectedIds])} onApply={applyBatch} />}
      <div className="grid-scroll" ref={scrollRef}><AssetGrid items={items} total={total} loading={loading} selectedId={selectedId} onSelect={selectAsset} onFavorite={toggleFavorite} onLoadMore={loadMore} scrollRef={scrollRef} selectionMode={selectionMode} selectedIds={selectedIds} onToggleSelect={toggleSelection} onPointerDragStart={beginLibraryPointerDrag} preferences={personalization.library} query={request.query} /></div>
      </>}
    </main>

    {!mediaKind && view !== "projects" && view !== "reference" && view !== "tagManager" && view !== "trash" && <DetailPanel asset={detail} contentLanguage={contentLanguage} loading={detailLoading} onClose={() => { setSelectedId(null); setDetail(null); }} onEdit={() => setEditor("edit")} onDelete={deleteAsset} onFavorite={() => detail && toggleFavorite(detail)} onOpen={async () => { if (!detail) return; try { await api.openShare(detail.id); setDetail(await api.getAsset(detail.id, contentLanguage)); } catch (error) { notify(String(error), true); } }} onSourceOpen={async () => { if (!detail?.sourceUrl) return; try { await api.openExternal(detail.sourceUrl); } catch (error) { notify(String(error), true); } }} onOpenExternal={async url => { try { await api.openExternal(url); } catch (error) { notify(String(error), true); } }} onCopy={async () => { if (!detail) return; try { await api.copyCode(detail.id); notify("提取码已复制"); } catch (error) { notify(String(error), true); } }} onCheckLink={() => void checkDetailLink()} checkingLink={checkingDetailLink} onAddReference={imageIds => setReferencePicker({ imageIds, title: detail?.name || "素材预览图" })} onAddProject={id => setProjectPicker([id])} onCollectMedia={collectAssetMedia} />}
    {editor && <AssetEditor asset={editor === "edit" ? detail : null} contentLanguage={contentLanguage} initialShare={editor === "new" ? initialShare : null} categories={meta.categories} saveShortcut={personalization.global.shortcuts.saveAsset || "Ctrl+S"} onClose={() => { setEditor(null); setInitialShare(null); }} onSave={saveAsset} onMediaImported={async id=>{setDetail(await api.getAsset(id,contentLanguage));await refreshAll();}} onOpenExisting={id => { setEditor(null); setInitialShare(null); void selectAsset(id); }} />}
    {showImport && <ImportDialog onClose={() => setShowImport(false)} onImported={refreshAll} notify={notify} />}
    {showQuickAdd && <QuickAddDialog onClose={() => setShowQuickAdd(false)} notify={notify} onParsed={parsed => { setInitialShare(parsed); setShowQuickAdd(false); setEditor("new"); }} onOpenExisting={id => { setShowQuickAdd(false); selectAsset(id); }} />}
    {showSettings && <StorageSettings initialTab="appearance" onClose={() => setShowSettings(false)} onLibraryChanged={libraryChanged} notify={notify} libraryLocked={Boolean(detachedBoardId)} onPersonalizationChanged={personalizationChanged} />}
    {referencePicker && <ReferenceBoardPicker imageIds={referencePicker.imageIds} title={referencePicker.title} lockedBoardId={detachedBoardId} onClose={() => setReferencePicker(null)} notify={notify} />}
    {projectPicker && <ProjectPicker assetIds={projectPicker} onClose={() => setProjectPicker(null)} onAdded={() => setProjectRefreshKey(value => value + 1)} notify={notify} />}
    {activeDrag && dragPoint && <div className="library-drag-preview active" style={{ left: dragPoint.x + 14, top: dragPoint.y + 14 }}>{dragPreviewText(activeDrag)}</div>}
    {showCommandPalette && <CommandPalette commands={paletteCommands} onClose={() => setShowCommandPalette(false)} />}
    <div className="toast-stack">{toasts.map(toast => <div key={toast.id} className={`toast ${toast.error ? "error" : ""}`}><span>{toast.message}</span>{toast.action && <button onClick={() => { setToasts(previous => previous.filter(item => item.id !== toast.id)); void toast.action?.(); }}>{toast.actionLabel}</button>}</div>)}</div>
  </div>;
}
