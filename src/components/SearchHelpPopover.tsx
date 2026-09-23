import { createPortal } from "react-dom";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

const LEAVE_GRACE_MS = 180;

export function SearchHelpPopover() {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 8, top: 48, width: 390 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<number | null>(null);
  const panelId = useId();

  const cancelClose = () => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
  };
  const scheduleClose = () => {
    cancelClose();
    timerRef.current = window.setTimeout(() => { setOpen(false); timerRef.current = null; }, LEAVE_GRACE_MS);
  };
  const place = () => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const width = Math.min(390, Math.max(260, window.innerWidth - 16));
    const left = Math.max(8, Math.min(window.innerWidth - width - 8, rect.right - width));
    const estimatedHeight = Math.min(300, window.innerHeight - 16);
    const below = rect.bottom + 8;
    const top = below + estimatedHeight <= window.innerHeight ? below : Math.max(8, rect.top - estimatedHeight - 8);
    setPosition({ left, top, width });
  };

  useLayoutEffect(() => { if (open) place(); }, [open]);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || panelRef.current?.contains(target)) return;
      setOpen(false);
    };
    const keydown = (event: KeyboardEvent) => { if (event.key === "Escape") { setOpen(false); triggerRef.current?.focus(); } };
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", keydown);
    window.addEventListener("resize", place);
    return () => { window.removeEventListener("pointerdown", outside); window.removeEventListener("keydown", keydown); window.removeEventListener("resize", place); };
  }, [open]);
  useEffect(() => () => cancelClose(), []);

  return <>
    <button
      ref={triggerRef}
      type="button"
      className="search-help-trigger"
      aria-label="查看高级搜索帮助"
      aria-expanded={open}
      aria-controls={panelId}
      onClick={() => { cancelClose(); setOpen(true); }}
      onMouseEnter={() => { cancelClose(); setOpen(true); }}
      onMouseLeave={scheduleClose}
      onBlur={() => window.setTimeout(() => { if (!panelRef.current?.contains(document.activeElement)) scheduleClose(); }, 0)}
    >?</button>
    {open && createPortal(<div
      ref={panelRef}
      id={panelId}
      role="dialog"
      aria-label="高级搜索帮助"
      className="search-help-popover"
      style={position}
      onMouseEnter={cancelClose}
      onMouseLeave={scheduleClose}
      onFocus={cancelClose}
      onBlur={() => window.setTimeout(() => { if (!triggerRef.current?.contains(document.activeElement) && !panelRef.current?.contains(document.activeElement)) scheduleClose(); }, 0)}
    ><strong>高级搜索</strong><span>空格表示 AND，| 表示 OR，- 表示排除，双引号匹配完整短语。</span><code>name:&quot;desert dune&quot; tag:Nanite</code><code>software:Unreal -format:FBX</code><span>字段：name、tag、desc、category、author、software、version、format、license、source</span></div>, document.body)}
  </>;
}
