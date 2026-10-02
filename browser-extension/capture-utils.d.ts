export const IMAGE_LIMIT: number;
export const VIDEO_LIMIT: number;
export const CHUNK_SIZE: number;
export function publicUrl(raw: string): string;
export function sourceUrl(raw: string): string;
export function folderPath(folder: { id: string; name: string; kind: string; parentId?: string | null }, folders: { id: string; name: string; kind: string; parentId?: string | null }[]): string;
export function extensionFor(mime: string, kind: "image" | "video"): string | null;
export function base64(bytes: Uint8Array): string;
export function resultText(result: { status: string; message?: string; result?: { status: string } }): string;
