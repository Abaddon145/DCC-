import { useCallback, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ArrowDownToLine, ArrowLeft, ArrowUpToLine, BringToFront, ClipboardPaste, Copy, Download, FolderOpen, Grid3X3, ImagePlus, Maximize2, Pin, PinOff, Redo2, RotateCcw, SendToBack, Trash2, Undo2, Unplug, X } from "lucide-react";
import type { ReferenceBoardDetail, ReferenceBoardItem, ReferenceBoardItemTransform } from "../types";
import { api } from "../lib/api";
import { arrangeItems, cloneItems, reorderItems, type BoardView } from "../lib/referenceBoard";
import { ReferenceCanvas, type ReferenceCanvasHandle } from "./ReferenceCanvas";

interface Props {
  initialBoard: ReferenceBoardDetail;
  floating?: boolean;
  onBack?: () => void;
  onDetach?: (boardId: string) => Promise<void>;
  onChanged?: () => void;
  notify: (message: string, error?: boolean) => void;
}

export function ReferenceBoardWorkspace({ initialBoard, floating = false, onBack, onDetach, onChanged, notify }: Props) {
  const [board, setBoard] = useState(initialBoard);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [toolbarVisible, setToolbarVisible] = useState(true);
  const [pinned, setPinned] = useState(true);
  const canvas = useRef<ReferenceCanvasHandle>(null);
  const boardRef = useRef(board);
  const persistedIds = useRef(new Set(initialBoard.items.map(item => item.id)));
  const undoStack = useRef<ReferenceBoardItem[][]>([]);
  const redoStack = useRef<ReferenceBoardItem[][]>([]);
  const copiedIds = useRef<string[]>([]);
  const savingPromise = useRef<Promise<void> | null>(null);
  const revision = useRef(0);
  const savedRevision = useRef(0);
  const closeArmed = useRef(false);
  boardRef.current = board;

  const markDirty = () => { revision.current += 1; setDirty(true); };
  const pushUndo = (items: ReferenceBoardItem[]) => {
    undoStack.current.push(cloneItems(items));
    if (undoStack.current.length > 50) undoStack.current.shift();
    redoStack.current = [];
  };
  const commitItems = (before: ReferenceBoardItem[], after: ReferenceBoardItem[]) => {
    if (sameTransforms(before, after)) return;
    pushUndo(before); setBoard(current => ({ ...current, items: cloneItems(after) })); markDirty();
  };
  const mutateItems = (change: (items: ReferenceBoardItem[]) => ReferenceBoardItem[]) => {
    const before = cloneItems(boardRef.current.items); const after = change(cloneItems(before)); commitItems(before, after);
  };

  const saveNow = useCallback(async () => {
    if (savingPromise.current) await savingPromise.current;
    if (revision.current === savedRevision.current) { setDirty(false); return; }
    const savingRevision = revision.current;
    const current = boardRef.current;
    const currentIds = new Set(current.items.map(item => item.id));
    const deletedIds = [...persistedIds.current].filter(id => !currentIds.has(id));
    const restoredIds = [...currentIds].filter(id => !persistedIds.current.has(id));
    const task = api.saveReferenceBoardChanges({
      boardId: current.id, background: current.background, viewX: current.viewX, viewY: current.viewY, viewScale: current.viewScale,
      items: current.items.map(toTransform), deletedIds, restoredIds
    }).then(() => {
      persistedIds.current = currentIds;
      savedRevision.current = savingRevision;
      setDirty(savedRevision.current !== revision.current);
      onChanged?.();
    }).finally(() => { savingPromise.current = null; setSaving(false); });
    savingPromise.current = task; setSaving(true); await task;
  }, [onChanged]);

  useEffect(() => {
    if (!dirty) return;
    const timer = window.setTimeout(() => { void saveNow().catch(error => notify(`参考板自动保存失败：${String(error)}`, true)); }, 500);
    return () => window.clearTimeout(timer);
  }, [dirty, board, saveNow, notify]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebviewWindow().onDragDropEvent(event => {
      if (event.payload.type !== "drop") return;
      const paths = event.payload.paths.filter(path => /\.(png|jpe?g|webp|bmp|gif|tiff?)$/i.test(path));
      if (paths.length) {
        const ratio = window.devicePixelRatio || 1;
        const screenPoint = { x: event.payload.position.x / ratio, y: event.payload.position.y / ratio };
        const placement = canvas.current?.pointAt(screenPoint) || center();
        void addPaths(paths, placement);
      }
    }).then(dispose => { unlisten = dispose; }).catch(() => undefined);
    return () => unlisten?.();
  }, [board.id]);

  useEffect(() => {
    if (!floating || !("__TAURI_INTERNALS__" in window)) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebviewWindow().onCloseRequested(async event => {
      if (closeArmed.current) return;
      event.preventDefault();
      try { await saveNow(); closeArmed.current = true; await api.attachReferenceWindow(boardRef.current.id); }
      catch (error) { notify(`关闭悬浮参考板失败：${String(error)}`, true); }
    }).then(dispose => { unlisten = dispose; });
    return () => unlisten?.();
  }, [floating, saveNow, notify]);

  const center = () => canvas.current?.centerPoint() || { x: 0, y: 0 };
  const addReport = (items: ReferenceBoardItem[], skipped: number, warnings: string[]) => {
    if (items.length) {
      pushUndo(boardRef.current.items); items.forEach(item => persistedIds.current.add(item.id));
      setBoard(current => ({ ...current, items: [...current.items, ...items] }));
      setSelectedIds(new Set(items.map(item => item.id))); onChanged?.();
    }
    const suffix = skipped ? `，跳过 ${skipped} 张` : "";
    notify(items.length ? `已加入 ${items.length} 张参考图${suffix}` : warnings[0] || "没有可加入的图片", !items.length);
  };
  const addPaths = async (paths: string[], placement = center()) => {
    try { const report = await api.importReferenceImages(boardRef.current.id, paths, placement); addReport(report.items, report.skipped, report.warnings); }
    catch (error) { notify(`添加参考图失败：${String(error)}`, true); }
  };
  const chooseImages = async () => {
    const result = await open({ multiple: true, filters: [{ name: "参考图片", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"] }] });
    if (result) await addPaths(Array.isArray(result) ? result : [result]);
  };
  const pasteImage = async () => {
    try { const item = await api.pasteReferenceClipboardImage(boardRef.current.id, center()); addReport([item], 0, []); }
    catch (error) { notify(String(error), true); }
  };
  const duplicateSelected = async () => {
    if (!selectedIds.size) return;
    try {
      const before = cloneItems(boardRef.current.items); const items = await api.duplicateReferenceItems(boardRef.current.id, [...selectedIds]);
      pushUndo(before); items.forEach(item => persistedIds.current.add(item.id)); setBoard(current => ({ ...current, items: [...current.items, ...items] })); setSelectedIds(new Set(items.map(item => item.id))); onChanged?.();
    } catch (error) { notify(`复制参考图失败：${String(error)}`, true); }
  };
  const deleteSelected = () => { if (!selectedIds.size) return; mutateItems(items => items.filter(item => !selectedIds.has(item.id))); setSelectedIds(new Set()); };
  const undo = () => {
    const previous = undoStack.current.pop(); if (!previous) return;
    redoStack.current.push(cloneItems(boardRef.current.items)); setBoard(current => ({ ...current, items: previous })); setSelectedIds(new Set()); markDirty();
  };
  const redo = () => {
    const next = redoStack.current.pop(); if (!next) return;
    undoStack.current.push(cloneItems(boardRef.current.items)); setBoard(current => ({ ...current, items: next })); setSelectedIds(new Set()); markDirty();
  };
  const reorder = (direction: "front" | "back" | "forward" | "backward") => mutateItems(items => reorderItems(items, selectedIds, direction));
  const arrange = () => mutateItems(items => arrangeItems(items, selectedIds));
  const exportPng = async () => {
    const path = await save({ defaultPath: `${safeFilename(boardRef.current.name)}.png`, filters: [{ name: "PNG 图片", extensions: ["png"] }] }); if (!path) return;
    try { await saveNow(); await api.exportReferenceBoard(boardRef.current.id, path, { scale: 1 }); notify("参考板 PNG 已导出"); }
    catch (error) { notify(`导出失败：${String(error)}`, true); }
  };
  const detach = async () => {
    if (!onDetach) return;
    try { await saveNow(); await onDetach(boardRef.current.id); }
    catch (error) { notify(String(error), true); }
  };
  const attach = async () => {
    try { await saveNow(); closeArmed.current = true; await api.attachReferenceWindow(boardRef.current.id); }
    catch (error) { closeArmed.current = false; notify(String(error), true); }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (isTyping(event.target)) return;
      const control = event.ctrlKey || event.metaKey;
      if (control && event.key.toLowerCase() === "z") { event.preventDefault(); event.shiftKey ? redo() : undo(); }
      else if (control && event.key.toLowerCase() === "y") { event.preventDefault(); redo(); }
      else if (control && event.key.toLowerCase() === "d") { event.preventDefault(); void duplicateSelected(); }
      else if (control && event.key.toLowerCase() === "c") { copiedIds.current = [...selectedIds]; }
      else if (control && event.key.toLowerCase() === "v") { event.preventDefault(); if (copiedIds.current.length) { setSelectedIds(new Set(copiedIds.current)); void api.duplicateReferenceItems(boardRef.current.id, copiedIds.current).then(items => addReport(items, 0, [])).catch(error => notify(String(error), true)); } else void pasteImage(); }
      else if (event.key === "Delete" || event.key === "Backspace") { event.preventDefault(); deleteSelected(); }
      else if (event.key.toLowerCase() === "f") { event.preventDefault(); event.shiftKey ? canvas.current?.fitSelection() : canvas.current?.fitAll(); }
      else if (event.key === "0") { canvas.current?.resetView(); }
      else if (event.key === "Escape") setSelectedIds(new Set());
      else if (event.key === "Tab" && floating) { event.preventDefault(); setToolbarVisible(value => !value); }
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [selectedIds, floating]);

  const toolbars = <div className={`reference-toolbar ${floating ? "floating" : ""}`} data-no-drag>
    {!floating && <button title="返回参考板列表" onClick={async () => { await saveNow(); onBack?.(); }}><ArrowLeft size={16} /></button>}
    <strong title={board.name}>{board.name}</strong><span className="reference-save-state">{saving ? "保存中…" : dirty ? "等待保存" : "已保存"}</span>
    <i />
    <button title="添加本地图片" onClick={() => void chooseImages()}><ImagePlus size={16} /></button>
    <button title="粘贴剪贴板图片" onClick={() => void pasteImage()}><ClipboardPaste size={16} /></button>
    <button title="撤销 Ctrl+Z" disabled={!undoStack.current.length} onClick={undo}><Undo2 size={16} /></button>
    <button title="重做 Ctrl+Y" disabled={!redoStack.current.length} onClick={redo}><Redo2 size={16} /></button>
    <button title="自动排列" disabled={!board.items.length} onClick={arrange}><Grid3X3 size={16} /></button>
    <button title="复制选择 Ctrl+D" disabled={!selectedIds.size} onClick={() => void duplicateSelected()}><Copy size={16} /></button>
    <button title="向前一层" disabled={!selectedIds.size} onClick={() => reorder("forward")}><ArrowUpToLine size={16} /></button>
    <button title="向后一层" disabled={!selectedIds.size} onClick={() => reorder("backward")}><ArrowDownToLine size={16} /></button>
    <button title="置于顶层" disabled={!selectedIds.size} onClick={() => reorder("front")}><BringToFront size={16} /></button>
    <button title="置于底层" disabled={!selectedIds.size} onClick={() => reorder("back")}><SendToBack size={16} /></button>
    <button title="删除选择" className="danger" disabled={!selectedIds.size} onClick={deleteSelected}><Trash2 size={16} /></button>
    <button title="适应全部 F" onClick={() => canvas.current?.fitAll()}><Maximize2 size={16} /></button>
    <label className="reference-background" title="背景颜色"><input type="color" value={board.background} onChange={event => { setBoard(current => ({ ...current, background: event.target.value })); markDirty(); }} /></label>
    <button title="导出 PNG" onClick={() => void exportPng()}><Download size={16} /></button>
    {!floating && <button title="分离为悬浮窗口" onClick={() => void detach()}><Unplug size={16} /></button>}
    {floating && <><button title={pinned ? "取消置顶" : "始终置顶"} onClick={async () => { const next = !pinned; try { await api.setReferenceWindowAlwaysOnTop(next); setPinned(next); } catch (error) { notify(String(error), true); } }}>{pinned ? <Pin size={16} /> : <PinOff size={16} />}</button><button title="收回主窗口" onClick={() => void attach()}><RotateCcw size={16} /></button><button title="关闭并收回" className="danger" onClick={() => void attach()}><X size={16} /></button></>}
  </div>;

  return <div className={`reference-workspace ${floating ? "floating" : ""}`}>
    {floating && <div className="reference-window-drag-strip" data-tauri-drag-region>{toolbarVisible && toolbars}<span data-tauri-drag-region /></div>}
    {!floating && toolbars}
    <ReferenceCanvas ref={canvas} items={board.items} view={{ x: board.viewX, y: board.viewY, scale: board.viewScale }} background={board.background} selectedIds={selectedIds} onSelectionChange={setSelectedIds} onItemsLive={items => setBoard(current => ({ ...current, items }))} onItemsCommit={commitItems} onViewChange={(view: BoardView, commit) => { setBoard(current => ({ ...current, viewX: view.x, viewY: view.y, viewScale: view.scale })); if (commit) markDirty(); }} />
    {!board.items.length && <div className="reference-empty-overlay"><FolderOpen size={34} /><strong>把参考图片放到这里</strong><span>从素材库加入、拖入本地图片，或粘贴剪贴板截图</span><button className="primary-button" onClick={() => void chooseImages()}><ImagePlus size={16} />选择图片</button></div>}
  </div>;
}

function toTransform(item: ReferenceBoardItem): ReferenceBoardItemTransform { return { id: item.id, x: item.x, y: item.y, width: item.width, height: item.height, rotation: item.rotation, zIndex: item.zIndex }; }
function sameTransforms(a: ReferenceBoardItem[], b: ReferenceBoardItem[]) { return a.length === b.length && a.every((item, index) => { const next = b[index]; return next && item.id === next.id && item.x === next.x && item.y === next.y && item.width === next.width && item.height === next.height && item.rotation === next.rotation && item.zIndex === next.zIndex; }); }
function safeFilename(value: string) { return value.replace(/[<>:"/\\|?*]/g, "_").trim() || "参考板"; }
function isTyping(target: EventTarget | null) { return Boolean((target as HTMLElement | null)?.closest("input,textarea,[contenteditable='true']")); }
