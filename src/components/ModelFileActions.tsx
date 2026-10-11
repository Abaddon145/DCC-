import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Download } from "lucide-react";
import { api } from "../lib/api";

export async function exportModel(id: string, notify: (message: string, error?: boolean) => void) {
  const destination = await open({ directory: true, multiple: false, title: "选择模型包导出的工作目录" });
  if (typeof destination !== "string") return;
  const path = await api.exportModelPackage(id, destination);
  notify(`模型与依赖已完整导出：${path}`);
}
export function ModelFileActions({ id, notify }: { id: string; notify: (message: string, error?: boolean) => void }) {
  const [busy, setBusy] = useState(false);
  const run = async (operation: () => Promise<void>) => { setBusy(true); try { await operation(); } catch (error) { notify(String(error), true); } finally { setBusy(false); } };
  return <div className="model-file-actions">
    <button className="secondary-button" disabled={busy} onClick={() => void run(() => api.openModelLocation(id))}><FolderOpen size={16} />在资源管理器中显示</button>
    <button className="primary-button" disabled={busy} onClick={() => void run(() => exportModel(id, notify))}><Download size={16} />{busy ? "处理中…" : "导出模型包"}</button>
    <small>导出保留模型、贴图和已托管依赖，可用于你的 DCC 工程。</small>
  </div>;
}
