import type { ReferenceBoardItem } from "../types";

export interface BoardPoint { x: number; y: number }
export interface BoardView { x: number; y: number; scale: number }
export interface BoardBounds { left: number; top: number; right: number; bottom: number }

export const MIN_BOARD_ZOOM = 0.05;
export const MAX_BOARD_ZOOM = 8;

export function screenToBoard(point: BoardPoint, view: BoardView): BoardPoint {
  return { x: (point.x - view.x) / view.scale, y: (point.y - view.y) / view.scale };
}

export function boardToScreen(point: BoardPoint, view: BoardView): BoardPoint {
  return { x: view.x + point.x * view.scale, y: view.y + point.y * view.scale };
}

export function zoomViewAt(view: BoardView, cursor: BoardPoint, factor: number): BoardView {
  const nextScale = Math.max(MIN_BOARD_ZOOM, Math.min(MAX_BOARD_ZOOM, view.scale * factor));
  const world = screenToBoard(cursor, view);
  return { x: cursor.x - world.x * nextScale, y: cursor.y - world.y * nextScale, scale: nextScale };
}

export function itemBounds(item: ReferenceBoardItem): BoardBounds {
  const cx = item.x + item.width / 2;
  const cy = item.y + item.height / 2;
  const angle = item.rotation * Math.PI / 180;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const points = [[item.x, item.y], [item.x + item.width, item.y], [item.x + item.width, item.y + item.height], [item.x, item.y + item.height]].map(([x, y]) => {
    const dx = x - cx; const dy = y - cy;
    return { x: cx + dx * cos - dy * sin, y: cy + dx * sin + dy * cos };
  });
  return {
    left: Math.min(...points.map(point => point.x)), top: Math.min(...points.map(point => point.y)),
    right: Math.max(...points.map(point => point.x)), bottom: Math.max(...points.map(point => point.y))
  };
}

export function selectionBounds(items: ReferenceBoardItem[], ids?: Set<string>): BoardBounds | null {
  const selected = ids ? items.filter(item => ids.has(item.id)) : items;
  if (!selected.length) return null;
  const bounds = selected.map(itemBounds);
  return {
    left: Math.min(...bounds.map(value => value.left)), top: Math.min(...bounds.map(value => value.top)),
    right: Math.max(...bounds.map(value => value.right)), bottom: Math.max(...bounds.map(value => value.bottom))
  };
}

export function intersects(a: BoardBounds, b: BoardBounds) {
  return a.left <= b.right && a.right >= b.left && a.top <= b.bottom && a.bottom >= b.top;
}

export function fitBounds(bounds: BoardBounds, viewport: { width: number; height: number }, padding = 56): BoardView {
  const contentWidth = Math.max(1, bounds.right - bounds.left);
  const contentHeight = Math.max(1, bounds.bottom - bounds.top);
  const scale = Math.max(MIN_BOARD_ZOOM, Math.min(MAX_BOARD_ZOOM, Math.min((viewport.width - padding * 2) / contentWidth, (viewport.height - padding * 2) / contentHeight)));
  return {
    x: viewport.width / 2 - (bounds.left + contentWidth / 2) * scale,
    y: viewport.height / 2 - (bounds.top + contentHeight / 2) * scale,
    scale
  };
}

export function arrangeItems(items: ReferenceBoardItem[], selectedIds: Set<string>, gap = 24): ReferenceBoardItem[] {
  const targets = selectedIds.size ? items.filter(item => selectedIds.has(item.id)) : [...items];
  if (!targets.length) return items;
  const original = selectionBounds(targets)!;
  const targetWidth = Math.max(720, Math.sqrt(targets.reduce((sum, item) => sum + item.width * item.height, 0)) * 1.6);
  let x = original.left; let y = original.top; let rowHeight = 0;
  const updates = new Map<string, BoardPoint>();
  for (const item of [...targets].sort((a, b) => a.zIndex - b.zIndex || a.id.localeCompare(b.id))) {
    if (x > original.left && x + item.width > original.left + targetWidth) { x = original.left; y += rowHeight + gap; rowHeight = 0; }
    updates.set(item.id, { x, y });
    x += item.width + gap;
    rowHeight = Math.max(rowHeight, item.height);
  }
  return items.map(item => updates.has(item.id) ? { ...item, ...updates.get(item.id)! } : item);
}

export function normalizeZ(items: ReferenceBoardItem[]) {
  return [...items].sort((a, b) => a.zIndex - b.zIndex || a.id.localeCompare(b.id)).map((item, zIndex) => ({ ...item, zIndex }));
}

export function reorderItems(items: ReferenceBoardItem[], selectedIds: Set<string>, direction: "front" | "back" | "forward" | "backward") {
  const ordered = normalizeZ(items);
  if (direction === "front") return assignZ([...ordered.filter(item => !selectedIds.has(item.id)), ...ordered.filter(item => selectedIds.has(item.id))]);
  if (direction === "back") return assignZ([...ordered.filter(item => selectedIds.has(item.id)), ...ordered.filter(item => !selectedIds.has(item.id))]);
  const next = [...ordered];
  if (direction === "forward") {
    for (let index = next.length - 2; index >= 0; index--) if (selectedIds.has(next[index].id) && !selectedIds.has(next[index + 1].id)) [next[index], next[index + 1]] = [next[index + 1], next[index]];
  } else {
    for (let index = 1; index < next.length; index++) if (selectedIds.has(next[index].id) && !selectedIds.has(next[index - 1].id)) [next[index], next[index - 1]] = [next[index - 1], next[index]];
  }
  return assignZ(next);
}

function assignZ(items: ReferenceBoardItem[]) { return items.map((item, zIndex) => ({ ...item, zIndex })); }

export function visibleItems(items: ReferenceBoardItem[], view: BoardView, viewport: { width: number; height: number }, overscan = 300) {
  const topLeft = screenToBoard({ x: -overscan, y: -overscan }, view);
  const bottomRight = screenToBoard({ x: viewport.width + overscan, y: viewport.height + overscan }, view);
  const visible = { left: topLeft.x, top: topLeft.y, right: bottomRight.x, bottom: bottomRight.y };
  return items.filter(item => intersects(itemBounds(item), visible));
}

export function cloneItems(items: ReferenceBoardItem[]) { return items.map(item => ({ ...item })); }
