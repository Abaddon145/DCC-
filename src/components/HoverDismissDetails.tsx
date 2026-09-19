import { useEffect, useRef, type DetailsHTMLAttributes } from "react";

const LEAVE_GRACE_MS = 180;

export function HoverDismissDetails({ children, onMouseEnter, onMouseLeave, onKeyDown, ...props }: DetailsHTMLAttributes<HTMLDetailsElement>) {
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

  useEffect(() => cancelClose, []);

  return <details
    {...props}
    ref={detailsRef}
    onMouseEnter={event => { cancelClose(); onMouseEnter?.(event); }}
    onMouseLeave={event => { scheduleClose(); onMouseLeave?.(event); }}
    onKeyDown={event => {
      if (event.key === "Escape") {
        cancelClose();
        event.currentTarget.open = false;
      }
      onKeyDown?.(event);
    }}
  >{children}</details>;
}
