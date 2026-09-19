import type { LibraryDragPayload } from "../types";

export interface LibraryDragPoint { x: number; y: number }

export type BeginLibraryPointerDrag = (
  payload: LibraryDragPayload,
  event: React.PointerEvent<HTMLElement>
) => void;

export function dragPreviewText(payload: LibraryDragPayload) {
  return payload.kind === "assets" && payload.ids.length > 1
    ? `${payload.ids.length} 项素材`
    : payload.label;
}
