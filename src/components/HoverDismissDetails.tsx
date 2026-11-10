import { useEffect, useRef, type DetailsHTMLAttributes } from "react";

const LEAVE_GRACE_MS = 180;

export function HoverDismissDetails({ children, onMouseEnter, onMouseLeave, onKeyDown, closeOnLeave = true, ...props }: DetailsHTMLAttributes<HTMLDetailsElement> & {closeOnLeave?:boolean}) {
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const timerRef = useRef<number | null>(null);

  const cancelClose = () => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const scheduleClose = () => {
    cancelClose();
    timerRef.current = window.setTimeout(() => {
      if (detailsRef.current) detailsRef.current.open = false;
      timerRef.current = null;
    }, LEAVE_GRACE_MS);
  };

  useEffect(() => {
    const doc = detailsRef.current?.ownerDocument;
    const dismiss = (event: PointerEvent) => {
      const details = detailsRef.current;
      if (details?.open && event.target instanceof Node && !details.contains(event.target)) {
        cancelClose();
        details.open = false;
      }
    };
    doc?.addEventListener("pointerdown", dismiss, true);
    return () => { doc?.removeEventListener("pointerdown", dismiss, true); cancelClose(); };
  }, []);

  return <details
    {...props}
    ref={detailsRef}
    onMouseEnter={event => { cancelClose(); onMouseEnter?.(event); }}
    onMouseLeave={event => { if (closeOnLeave) scheduleClose(); onMouseLeave?.(event); }}
    onKeyDown={event => {
      if (event.key === "Escape") {
        cancelClose();
        if (event.currentTarget.open) {
          event.currentTarget.open = false;
          event.currentTarget.querySelector("summary")?.focus();
          event.stopPropagation();
        }
      }
      onKeyDown?.(event);
    }}
  >{children}</details>;
}
