import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import type { ReferenceBoardItem } from "../types";
import { boardToScreen, fitBounds, intersects, itemBounds, screenToBoard, selectionBounds, visibleItems, zoomViewAt, type BoardBounds, type BoardPoint, type BoardView } from "../lib/referenceBoard";
import { ReferenceImage } from "./ReferenceImage";

export interface ReferenceCanvasHandle {
  centerPoint: () => BoardPoint;
  pointAt: (screenPoint: BoardPoint) => BoardPoint;
  fitAll: () => void;
  fitSelection: () => void;
  resetView: () => void;
}

interface Props {
  items: ReferenceBoardItem[];
  view: BoardView;
  background: string;
  selectedIds: Set<string>;
  onSelectionChange: (ids: Set<string>) => void;
  onItemsLive: (items: ReferenceBoardItem[]) => void;
  onItemsCommit: (before: ReferenceBoardItem[], after: ReferenceBoardItem[]) => void;
  onViewChange: (view: BoardView, commit: boolean) => void;
}

type Gesture =
  | { mode: "move"; pointerId: number; start: BoardPoint; before: ReferenceBoardItem[]; ids: Set<string> }
  | { mode: "resize"; pointerId: number; anchor: BoardPoint; distance: number; before: ReferenceBoardItem[]; ids: Set<string> }
  | { mode: "rotate"; pointerId: number; center: BoardPoint; angle: number; before: ReferenceBoardItem[]; ids: Set<string> }
  | { mode: "pan"; pointerId: number; start: BoardPoint; before: BoardView }
  | { mode: "marquee"; pointerId: number; start: BoardPoint; base: Set<string> };

export const ReferenceCanvas = forwardRef<ReferenceCanvasHandle, Props>(function ReferenceCanvas(props, ref) {
  const container = useRef<HTMLDivElement>(null);
  const gesture = useRef<Gesture | null>(null);
  const itemsRef = useRef(props.items);
  const viewRef = useRef(props.view);
  const [viewport, setViewport] = useState({ width: 1, height: 1 });
  const [spacePressed, setSpacePressed] = useState(false);
  const [marquee, setMarquee] = useState<{ start: BoardPoint; end: BoardPoint } | null>(null);
  itemsRef.current = props.items; viewRef.current = props.view;

  useEffect(() => {
    const node = container.current; if (!node) return;
    const observer = new ResizeObserver(([entry]) => setViewport({ width: entry.contentRect.width, height: entry.contentRect.height }));
    observer.observe(node); return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const keyDown = (event: KeyboardEvent) => { if (event.code === "Space" && !isTyping(event.target)) { event.preventDefault(); setSpacePressed(true); } };
    const keyUp = (event: KeyboardEvent) => { if (event.code === "Space") setSpacePressed(false); };
    window.addEventListener("keydown", keyDown); window.addEventListener("keyup", keyUp);
    return () => { window.removeEventListener("keydown", keyDown); window.removeEventListener("keyup", keyUp); };
  }, []);

  const localPoint = (event: { clientX: number; clientY: number }) => {
    const rect = container.current?.getBoundingClientRect();
    return { x: event.clientX - (rect?.left || 0), y: event.clientY - (rect?.top || 0) };
  };
  const fit = (ids?: Set<string>) => {
    const bounds = selectionBounds(itemsRef.current, ids);
    if (bounds) props.onViewChange(fitBounds(bounds, viewport), true);
  };
  useImperativeHandle(ref, () => ({
    centerPoint: () => screenToBoard({ x: viewport.width / 2, y: viewport.height / 2 }, viewRef.current),
    pointAt: (screenPoint: BoardPoint) => screenToBoard(screenPoint, viewRef.current),
    fitAll: () => fit(), fitSelection: () => fit(props.selectedIds),
    resetView: () => props.onViewChange({ x: viewport.width / 2, y: viewport.height / 2, scale: 1 }, true)
  }), [viewport, props.selectedIds]);

  const visible = useMemo(() => visibleItems(props.items, props.view, viewport), [props.items, props.view, viewport]);
  const selectedBounds = useMemo(() => selectionBounds(props.items, props.selectedIds), [props.items, props.selectedIds]);

  const capture = (pointerId: number) => { try { container.current?.setPointerCapture(pointerId); } catch { /* WebView may already own capture. */ } };
  const startMove = (event: React.PointerEvent, item: ReferenceBoardItem) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    const nextSelection = new Set(props.selectedIds);
    if (event.ctrlKey || event.shiftKey) {
      if (nextSelection.has(item.id)) nextSelection.delete(item.id); else nextSelection.add(item.id);
    } else if (!nextSelection.has(item.id)) { nextSelection.clear(); nextSelection.add(item.id); }
    props.onSelectionChange(nextSelection);
    if (!nextSelection.has(item.id)) return;
    gesture.current = { mode: "move", pointerId: event.pointerId, start: screenToBoard(localPoint(event), props.view), before: props.items.map(value => ({ ...value })), ids: nextSelection };
    capture(event.pointerId);
  };

  const startResize = (event: React.PointerEvent, corner: string) => {
    if (!selectedBounds) return; event.preventDefault(); event.stopPropagation();
    const anchor = { x: corner.includes("w") ? selectedBounds.right : selectedBounds.left, y: corner.includes("n") ? selectedBounds.bottom : selectedBounds.top };
    const moving = { x: corner.includes("w") ? selectedBounds.left : selectedBounds.right, y: corner.includes("n") ? selectedBounds.top : selectedBounds.bottom };
    gesture.current = { mode: "resize", pointerId: event.pointerId, anchor, distance: Math.max(1, Math.hypot(moving.x - anchor.x, moving.y - anchor.y)), before: props.items.map(value => ({ ...value })), ids: new Set(props.selectedIds) };
    capture(event.pointerId);
  };

  const startRotate = (event: React.PointerEvent) => {
    if (!selectedBounds) return; event.preventDefault(); event.stopPropagation();
    const center = { x: (selectedBounds.left + selectedBounds.right) / 2, y: (selectedBounds.top + selectedBounds.bottom) / 2 };
    const point = screenToBoard(localPoint(event), props.view);
    gesture.current = { mode: "rotate", pointerId: event.pointerId, center, angle: Math.atan2(point.y - center.y, point.x - center.x), before: props.items.map(value => ({ ...value })), ids: new Set(props.selectedIds) };
    capture(event.pointerId);
  };

  const pointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest(".reference-board-item,.selection-handle")) return;
    const point = localPoint(event);
    if (event.button === 1 || (event.button === 0 && spacePressed)) {
      event.preventDefault(); gesture.current = { mode: "pan", pointerId: event.pointerId, start: point, before: { ...props.view } }; capture(event.pointerId); return;
    }
    if (event.button !== 0) return;
    const base = event.ctrlKey || event.shiftKey ? new Set(props.selectedIds) : new Set<string>();
    if (!event.ctrlKey && !event.shiftKey) props.onSelectionChange(new Set());
    gesture.current = { mode: "marquee", pointerId: event.pointerId, start: screenToBoard(point, props.view), base };
    setMarquee({ start: screenToBoard(point, props.view), end: screenToBoard(point, props.view) }); capture(event.pointerId);
  };

  const pointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const active = gesture.current; if (!active || active.pointerId !== event.pointerId) return;
    const screen = localPoint(event);
    if (active.mode === "pan") { props.onViewChange({ ...active.before, x: active.before.x + screen.x - active.start.x, y: active.before.y + screen.y - active.start.y }, false); return; }
    const point = screenToBoard(screen, viewRef.current);
    if (active.mode === "move") {
      const dx = point.x - active.start.x; const dy = point.y - active.start.y;
      props.onItemsLive(active.before.map(item => active.ids.has(item.id) ? { ...item, x: item.x + dx, y: item.y + dy } : item));
    } else if (active.mode === "resize") {
      const factor = Math.max(0.02, Math.hypot(point.x - active.anchor.x, point.y - active.anchor.y) / active.distance);
      props.onItemsLive(active.before.map(item => {
        if (!active.ids.has(item.id)) return item;
        const cx = item.x + item.width / 2; const cy = item.y + item.height / 2;
        const nextCx = active.anchor.x + (cx - active.anchor.x) * factor; const nextCy = active.anchor.y + (cy - active.anchor.y) * factor;
        const width = Math.max(1, item.width * factor); const height = Math.max(1, item.height * factor);
        return { ...item, width, height, x: nextCx - width / 2, y: nextCy - height / 2 };
      }));
    } else if (active.mode === "rotate") {
      let delta = (Math.atan2(point.y - active.center.y, point.x - active.center.x) - active.angle) * 180 / Math.PI;
      if (event.shiftKey) delta = Math.round(delta / 15) * 15;
      const radians = delta * Math.PI / 180; const cos = Math.cos(radians); const sin = Math.sin(radians);
      props.onItemsLive(active.before.map(item => {
        if (!active.ids.has(item.id)) return item;
        const cx = item.x + item.width / 2; const cy = item.y + item.height / 2; const dx = cx - active.center.x; const dy = cy - active.center.y;
        const nextCx = active.center.x + dx * cos - dy * sin; const nextCy = active.center.y + dx * sin + dy * cos;
        return { ...item, x: nextCx - item.width / 2, y: nextCy - item.height / 2, rotation: item.rotation + delta };
      }));
    } else if (active.mode === "marquee") {
      const rectangle = normalizedBounds(active.start, point); setMarquee({ start: active.start, end: point });
      const next = new Set(active.base); props.items.forEach(item => { if (intersects(itemBounds(item), rectangle)) next.add(item.id); }); props.onSelectionChange(next);
    }
  };

  const pointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    const active = gesture.current; if (!active || active.pointerId !== event.pointerId) return;
    gesture.current = null; setMarquee(null);
    if (active.mode === "pan") props.onViewChange(viewRef.current, true);
    else if (active.mode !== "marquee") props.onItemsCommit(active.before, itemsRef.current.map(item => ({ ...item })));
  };

  const worldStyle = { transform: `translate(${props.view.x}px, ${props.view.y}px) scale(${props.view.scale})` };
  const handleSize = 10 / props.view.scale;
  return <div ref={container} className={`reference-canvas ${spacePressed ? "space-pan" : ""}`} style={{ backgroundColor: props.background }} onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={pointerUp} onPointerCancel={pointerUp} onWheel={event => {
    event.preventDefault(); const factor = Math.exp(-event.deltaY * .0015); props.onViewChange(zoomViewAt(props.view, localPoint(event), factor), true);
  }}>
    <div className="reference-grid" style={{ backgroundSize: `${24 * props.view.scale}px ${24 * props.view.scale}px`, backgroundPosition: `${props.view.x}px ${props.view.y}px` }} />
    <div className="reference-world" style={worldStyle}>
      {visible.map(item => <div key={item.id} className={`reference-board-item ${props.selectedIds.has(item.id) ? "selected" : ""}`} style={{ left: item.x, top: item.y, width: item.width, height: item.height, zIndex: item.zIndex, transform: `rotate(${item.rotation}deg)` }} onPointerDown={event => startMove(event, item)} title={item.originalName}>
        <ReferenceImage itemId={item.id} name={item.originalName} thumbnail={props.view.scale < 1.35} />
      </div>)}
      {selectedBounds && <div className="reference-selection-box" style={{ left: selectedBounds.left, top: selectedBounds.top, width: selectedBounds.right - selectedBounds.left, height: selectedBounds.bottom - selectedBounds.top, zIndex: Math.max(1, ...props.items.map(item => item.zIndex)) + 2, ["--board-scale" as string]: props.view.scale, ["--handle-size" as string]: `${handleSize}px`, ["--handle-offset" as string]: `${-handleSize / 2}px` }}>
        {(["nw", "ne", "sw", "se"] as const).map(corner => <button key={corner} aria-label={`缩放 ${corner}`} className={`selection-handle resize ${corner}`} onPointerDown={event => startResize(event, corner)} />)}
        <button aria-label="旋转选择" className="selection-handle rotate" onPointerDown={startRotate} />
      </div>}
    </div>
    {marquee && <div className="reference-marquee" style={screenBounds(marquee.start, marquee.end, props.view)} />}
    <div className="reference-zoom-indicator">{Math.round(props.view.scale * 100)}%</div>
  </div>;
});

function normalizedBounds(a: BoardPoint, b: BoardPoint): BoardBounds { return { left: Math.min(a.x, b.x), top: Math.min(a.y, b.y), right: Math.max(a.x, b.x), bottom: Math.max(a.y, b.y) }; }
function screenBounds(a: BoardPoint, b: BoardPoint, view: BoardView) { const first = boardToScreen(a, view); const second = boardToScreen(b, view); return { left: Math.min(first.x, second.x), top: Math.min(first.y, second.y), width: Math.abs(first.x - second.x), height: Math.abs(first.y - second.y) }; }
function isTyping(target: EventTarget | null) { const element = target as HTMLElement | null; return Boolean(element?.closest("input,textarea,[contenteditable='true']")); }
