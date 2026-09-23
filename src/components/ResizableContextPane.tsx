import { PanelLeftClose } from "lucide-react";
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";

const MIN_WIDTH = 180;
const MAX_WIDTH = 420;
const COLLAPSE_THRESHOLD = 196;

interface Props {
  ariaLabel: string;
  width: number;
  defaultWidth: number;
  collapsed: boolean;
  className?: string;
  children: ReactNode;
  onResizeEnd: (width: number) => void;
  onCollapsedChange: (collapsed: boolean) => void;
}

export function ResizableContextPane({ ariaLabel, width, defaultWidth, collapsed, className = "", children, onResizeEnd, onCollapsedChange }: Props) {
  const [draftWidth, setDraftWidth] = useState(width);
  const dragRef = useRef<{ startX: number; startWidth: number; rawWidth: number } | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => { if (!dragRef.current) setDraftWidth(width); }, [width]);
  useEffect(() => () => { dragRef.current = null; cleanupRef.current?.(); }, []);

  const startResize = (event: ReactPointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    dragRef.current = { startX: event.clientX, startWidth: draftWidth, rawWidth: draftWidth };
    const move = (pointerEvent: PointerEvent) => {
      if (!dragRef.current) return;
      const rawWidth = dragRef.current.startWidth + pointerEvent.clientX - dragRef.current.startX;
      dragRef.current.rawWidth = rawWidth;
      setDraftWidth(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, rawWidth)));
    };
    const finish = () => {
      const drag = dragRef.current;
      dragRef.current = null;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      cleanupRef.current = null;
      if (!drag) return;
      if (drag.rawWidth <= COLLAPSE_THRESHOLD) onCollapsedChange(true);
      else onResizeEnd(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, drag.rawWidth)));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
    cleanupRef.current = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", finish); };
  };

  if (collapsed) return null;
  return <aside className={`context-pane ${className}`} style={{ width: draftWidth }} aria-label={ariaLabel}>
    <button className="context-pane-collapse" type="button" onClick={() => onCollapsedChange(true)} title={`收起${ariaLabel}`} aria-label={`收起${ariaLabel}`}><PanelLeftClose size={15} /></button>
    {children}
    <button
      className="context-pane-resizer"
      type="button"
      aria-label={`调整${ariaLabel}宽度`}
      title="拖动调整宽度，双击恢复默认"
      onPointerDown={startResize}
      onDoubleClick={() => { setDraftWidth(defaultWidth); onResizeEnd(defaultWidth); onCollapsedChange(false); }}
    />
  </aside>;
}
