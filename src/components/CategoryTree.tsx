import { ChevronDown, ChevronRight, Folder, FolderPlus, Layers3, MoreHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Category, LibraryDragPayload, MoveCategoryRequest } from "../types";
import type { BeginLibraryPointerDrag } from "../lib/drag";
import { HoverDismissDetails } from "./HoverDismissDetails";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";

export type DropZone = "before" | "inside" | "after";

interface Props {
  categories: Category[];
  selected: string[];
  total: number;
  activeDrag: LibraryDragPayload | null;
  onChange: (ids: string[]) => void;
  onAdd: (parentId: string | null) => void;
  onRename: (category: Category) => void;
  onDelete: (category: Category) => void;
  onPointerDragStart: BeginLibraryPointerDrag;
  defaultExpanded?: boolean;
}

interface TreeNode extends Category { children: TreeNode[] }
interface DropTarget { id: string | null; zone: DropZone; valid: boolean }

function sorted(categories: Category[]) {
  return [...categories].sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name, "zh-CN"));
}

function buildTree(categories: Category[]): TreeNode[] {
  const map = new Map(categories.map(item => [item.id, { ...item, children: [] as TreeNode[] }]));
  const roots: TreeNode[] = [];
  for (const node of map.values()) {
    const parent = node.parentId ? map.get(node.parentId) : undefined;
    if (parent) parent.children.push(node); else roots.push(node);
  }
  const sort = (nodes: TreeNode[]) => nodes.sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name, "zh-CN")).forEach(node => sort(node.children));
  sort(roots); return roots;
}

function isDescendant(categories: Category[], ancestorId: string, possibleDescendantId: string) {
  const children = new Map<string, string[]>();
  for (const category of categories) {
    if (!category.parentId) continue;
    children.set(category.parentId, [...(children.get(category.parentId) || []), category.id]);
  }
  const pending = [...(children.get(ancestorId) || [])];
  while (pending.length) {
    const id = pending.pop()!;
    if (id === possibleDescendantId) return true;
    pending.push(...(children.get(id) || []));
  }
  return false;
}

export function categoryMoveRequest(categories: Category[], draggedId: string, targetId: string | null, zone: DropZone): MoveCategoryRequest | null {
  const dragged = categories.find(item => item.id === draggedId);
  if (!dragged) return null;
  if (targetId === null) {
    const roots = sorted(categories.filter(item => item.parentId === null && item.id !== draggedId));
    return { id: draggedId, targetParentId: null, targetIndex: roots.length };
  }
  const target = categories.find(item => item.id === targetId);
  if (!target || target.id === draggedId) return null;
  const targetParentId = zone === "inside" ? target.id : target.parentId;
  if (targetParentId === draggedId || (targetParentId && isDescendant(categories, draggedId, targetParentId))) return null;
  const siblings = sorted(categories.filter(item => item.parentId === targetParentId && item.id !== draggedId));
  if (zone === "inside") return { id: draggedId, targetParentId, targetIndex: siblings.length };
  const targetIndex = siblings.findIndex(item => item.id === target.id);
  if (targetIndex < 0) return null;
  return { id: draggedId, targetParentId, targetIndex: targetIndex + (zone === "after" ? 1 : 0) };
}

export function categoryDropZone(rect: Pick<DOMRect, "top" | "height">, clientY: number, payload: LibraryDragPayload, root = false): DropZone {
  if (root || payload.kind === "assets") return "inside";
  const ratio = rect.height ? (clientY - rect.top) / rect.height : .5;
  return ratio < .28 ? "before" : ratio > .72 ? "after" : "inside";
}

export function CategoryTree(props: Props) {
  const tree = useMemo(() => buildTree(props.categories), [props.categories]);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; title: string; items: ContextMenuItem[] } | null>(null);
  const expandTimer = useRef<number | null>(null);
  const expandTarget = useRef<string | null>(null);

  useEffect(() => () => {
    if (expandTimer.current !== null) window.clearTimeout(expandTimer.current);
  }, []);
  useEffect(() => {
    if (props.defaultExpanded !== false) return;
    const parents = new Set(props.categories.filter(category => props.categories.some(child => child.parentId === category.id)).map(category => category.id));
    setCollapsed(parents);
  }, [props.defaultExpanded, tree.length]);

  const clearExpandTimer = () => {
    if (expandTimer.current !== null) window.clearTimeout(expandTimer.current);
    expandTimer.current = null;
    expandTarget.current = null;
  };

  const scheduleExpand = (node: TreeNode, zone: DropZone, valid: boolean) => {
    if (!valid || zone !== "inside" || !collapsed.has(node.id) || !node.children.length) { clearExpandTimer(); return; }
    if (expandTarget.current === node.id) return;
    clearExpandTimer();
    expandTarget.current = node.id;
    expandTimer.current = window.setTimeout(() => {
      setCollapsed(previous => { const next = new Set(previous); next.delete(node.id); return next; });
      clearExpandTimer();
    }, 600);
  };

  useEffect(() => {
    if (props.activeDrag) return;
    clearExpandTimer();
    setDropTarget(null);
  }, [props.activeDrag]);

  const updateDrop = (event: React.PointerEvent<HTMLDivElement>, node: TreeNode | null) => {
    const payload = props.activeDrag;
    if (!payload) return;
    const zone = categoryDropZone(event.currentTarget.getBoundingClientRect(), event.clientY, payload, node === null);
    const request = payload.kind === "category" ? categoryMoveRequest(props.categories, payload.id, node?.id || null, zone) : null;
    const valid = payload.kind === "assets" || request !== null;
    setDropTarget({ id: node?.id || null, zone, valid });
    if (node) scheduleExpand(node, zone, valid); else clearExpandTimer();

    const scroll = event.currentTarget.closest(".category-scroll");
    if (scroll) {
      const rect = scroll.getBoundingClientRect();
      if (event.clientY < rect.top + 28) scroll.scrollBy({ top: -12 });
      else if (event.clientY > rect.bottom - 28) scroll.scrollBy({ top: 12 });
    }
  };

  const dropClasses = (id: string | null) => {
    if (!dropTarget || dropTarget.id !== id) return "";
    return `drag-${dropTarget.valid ? dropTarget.zone : "invalid"}`;
  };

  const nodeView = (node: TreeNode, depth: number): React.ReactNode => {
    const isCollapsed = collapsed.has(node.id);
    const selected = props.selected.includes(node.id);
    return <div key={node.id}>
      <div data-category-drop-id={node.id} className={`category-row ${selected ? "selected" : ""} ${dropClasses(node.id)}`} style={{ paddingLeft: 10 + depth * 14 }} onContextMenu={event => { event.preventDefault(); setContextMenu({ x:event.clientX, y:event.clientY, title:node.name, items:[{label:"打开分类",run:()=>props.onChange([node.id])},{label:"新建子分类",run:()=>props.onAdd(node.id)},{label:"重命名",run:()=>props.onRename(node)},{label:"移入回收站",danger:true,run:()=>props.onDelete(node)}] }); }} onPointerMove={event => updateDrop(event, node)} onPointerLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as globalThis.Node | null)) { clearExpandTimer(); setDropTarget(current => current?.id === node.id ? null : current); } }}>
        <button className="tree-toggle" onClick={() => setCollapsed(previous => {
          const next = new Set(previous); next.has(node.id) ? next.delete(node.id) : next.add(node.id); return next;
        })}>{node.children.length ? (isCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />) : <span />}</button>
        <button className="category-main" onPointerDown={event => {
          if (event.button !== 0) return;
          const payload: LibraryDragPayload = { kind: "category", id: node.id, label: node.name };
          props.onPointerDragStart(payload, event);
        }} onClick={() => props.onChange(selected ? [] : [node.id])}>
          <Folder size={15} /><span>{node.name}</span><small>{node.assetCount}</small>
        </button>
        <HoverDismissDetails className="row-menu">
          <summary><MoreHorizontal size={15} /></summary>
          <div className="menu-popover">
            <button onClick={() => props.onAdd(node.id)}>新建子分类</button>
            <button onClick={() => props.onRename(node)}>重命名</button>
            <button className="danger" onClick={() => props.onDelete(node)}>删除</button>
          </div>
        </HoverDismissDetails>
      </div>
      {!isCollapsed && node.children.map(child => nodeView(child, depth + 1))}
    </div>;
  };

  return <nav className="category-tree">
    <div data-category-drop-id="__root__" className={`category-row root ${props.selected.length === 0 ? "selected" : ""} ${dropClasses(null)}`} onContextMenu={event => { event.preventDefault(); setContextMenu({x:event.clientX,y:event.clientY,title:"全部素材",items:[{label:"显示全部素材",run:()=>props.onChange([])},{label:"新建根分类",run:()=>props.onAdd(null)}]}); }} onPointerMove={event => updateDrop(event, null)} onPointerLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as globalThis.Node | null)) setDropTarget(current => current?.id === null ? null : current); }}>
      <button className="category-main" onClick={() => props.onChange([])}><Layers3 size={16} /><span>全部素材</span><small>{props.total}</small></button>
      <button className="icon-button subtle" title="新建根分类" onClick={() => props.onAdd(null)}><FolderPlus size={15} /></button>
    </div>
    {tree.map(node => nodeView(node, 0))}
    {contextMenu && <ContextMenu {...contextMenu} onClose={() => setContextMenu(null)} />}
  </nav>;
}
