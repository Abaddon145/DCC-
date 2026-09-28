import { invoke } from "@tauri-apps/api/core";
import type { ContentLanguage, MediaKind } from "../types";

export type AiKind = "plan" | "organize" | "search" | "recommend";
export type AiEntryKind = "asset" | MediaKind;
export interface AiRequest {
  kind: AiKind; prompt: string; locale: ContentLanguage; mode: "fast" | "deep";
  assetIds: string[]; mediaIds: string[]; projectId: string | null; recallQuery: string; scope: "all" | AiEntryKind;
}
export interface AiEntry { id: string; kind: AiEntryKind; name: string; description: string; tags: string[]; category: string | null; software: string[]; versions: string[]; formats: string[]; license: string }
export interface AiChange { id: string; kind: AiEntryKind; field: "name" | "description" | "tags" | "category"; value: string | string[]; reason: string }
export interface AiOutput {
  title: string; summary: string; directions: string[]; needs: string[];
  tasks: Array<{ title: string; description: string; scene: string; shot: string }>;
  changes: AiChange[]; recommendations: Array<{ id: string; kind: AiEntryKind; reason: string; unknowns: string }>;
  searches: Array<{ kind: AiEntryKind; query: string }>;
}
export interface AiSession {
  id: string; name: string; kind: AiKind; status: string; message: string; updatedAt: string;
  request: AiRequest; context: { entries: AiEntry[]; project: unknown; categories: unknown; folders: unknown; existingTags: unknown };
  output: AiOutput | null; estimatedTokens: number;
}
export interface AiLaunch { kind?: AiKind; assetIds?: string[]; mediaIds?: string[]; projectId?: string; prompt?: string }
export const aiDefaults = (locale: ContentLanguage, launch?: AiLaunch): AiRequest => ({ kind: launch?.kind ?? "plan", prompt: launch?.prompt ?? "", locale, mode: "fast", assetIds: launch?.assetIds ?? [], mediaIds: launch?.mediaIds ?? [], projectId: launch?.projectId ?? null, recallQuery: "", scope: "all" });
export const aiSelectionKeys = (output: AiOutput) => [...output.tasks.map((_, i) => `task:${i}`), ...output.changes.map((_, i) => `change:${i}`), ...output.recommendations.map((_, i) => `recommend:${i}`)];
export const aiApi = {
  prepare: (request: AiRequest) => invoke<AiSession>("prepare_ai_request", { request }),
  get: (id: string) => invoke<AiSession>("get_ai_session", { id }),
  list: (offset = 0) => invoke<{ items: Pick<AiSession, "id" | "name" | "kind" | "status" | "updatedAt">[]; total: number }>("list_ai_sessions", { offset }),
  rename: (id: string, name: string) => invoke<void>("rename_ai_session", { id, name }),
  remove: (id: string) => invoke<void>("delete_ai_session", { id }),
  generate: (id: string) => invoke<AiSession>("generate_ai_session", { id }),
  cancel: (id: string) => invoke<void>("cancel_ai_session", { id }),
  apply: (id: string, selected: string[]) => invoke<string>("apply_ai_session", { id, selected }),
  undo: () => invoke<void>("undo_ai_application"),
  candidates: (kind: AiEntryKind, query: string, locale: string, offset = 0) => invoke<{ items: AiEntry[]; total: number; offset: number }>("search_ai_candidates", { kind, query, locale, offset }),
  search: (id: string, index: number, offset = 0) => invoke<{ items: AiEntry[]; total: number; offset: number }>("run_ai_search", { id, index, offset }),
};
