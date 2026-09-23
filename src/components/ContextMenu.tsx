import { createPortal } from "react-dom";
import { useLayoutEffect, useRef, useState } from "react";

export interface ContextMenuItem {
  label: string;
  run: () => void;
  disabled?: boolean;
  danger?: boolean;
}

interface Props {
  x: number;
  y: number;
  title?: string;
  items: ContextMenuItem[];
  onClose: () => void;
}

export function menuPosition(x: number, y: number, width: number, height: number, viewportWidth: number, viewportHeight: number) {
  return {
    left: Math.max(8, Math.min(x, viewportWidth - width - 8)),
    top: Math.max(8, Math.min(y, viewportHeight - height - 8)),
  };
}

export function ContextMenu({ x, y, title, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    setPosition(menuPosition(x, y, element.offsetWidth, element.offsetHeight, window.innerWidth, window.innerHeight));
    element.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y, items]);

  useLayoutEffect(() => {
    const pointer = (event: PointerEvent) => { if (!ref.current?.contains(event.target as Node)) onClose(); };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } };
    const scroll = () => onClose();
    window.addEventListener("pointerdown", pointer, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("scroll", scroll, true);
    return () => { window.removeEventListener("pointerdown", pointer, true); window.removeEventListener("keydown", key, true); window.removeEventListener("scroll", scroll, true); };
  }, [onClose]);

  return createPortal(<div ref={ref} className="app-context-menu" role="menu" style={position} onContextMenu={event => event.preventDefault()}>
    {title && <div className="app-context-menu-title">{title}</div>}
    {items.map((item, index) => <button key={`${item.label}-${index}`} type="button" role="menuitem" disabled={item.disabled} className={item.danger ? "danger" : ""} onClick={() => { onClose(); item.run(); }}>{item.label}</button>)}
  </div>, document.body);
}
