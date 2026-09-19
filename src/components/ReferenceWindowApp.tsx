import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { ReferenceBoardDetail } from "../types";
import { api } from "../lib/api";
import { ReferenceBoardWorkspace } from "./ReferenceBoardWorkspace";

export function ReferenceWindowApp() {
  const [board, setBoard] = useState<ReferenceBoardDetail | null>(null);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const loadBoard = useCallback(async (id: string) => {
    setError("");
    try {
      const loaded = await api.getReferenceBoard(id);
      setBoard(loaded);
      await api.referenceWindowReady();
    } catch (value) {
      setBoard(null);
      setError(String(value));
    }
  }, []);
  const refresh = useCallback(() => {
    void api.referenceWindowBoard().then(loadBoard).catch(() => undefined);
  }, [loadBoard]);
  useEffect(() => {
    const disposes: Array<() => void> = [];
    refresh();
    void listen<string>("reference-board-load", event => void loadBoard(event.payload)).then(dispose => disposes.push(dispose));
    void listen<string>("reference-board-unload", () => { setBoard(null); setError(""); }).then(dispose => disposes.push(dispose));
    void getCurrentWebviewWindow().onFocusChanged(event => { if (event.payload) refresh(); }).then(dispose => disposes.push(dispose));
    return () => disposes.forEach(dispose => dispose());
  }, [loadBoard, refresh]);
  const notify = (message: string, isError = false) => { setToast(`${isError ? "错误：" : ""}${message}`); window.setTimeout(() => setToast(""), 3500); };
  if (error) return <div className="reference-window-error"><strong>无法打开参考板</strong><span>{error}</span></div>;
  if (!board) return <div className="reference-window-loading">悬浮参考板已就绪…</div>;
  return <><ReferenceBoardWorkspace initialBoard={board} floating notify={notify} />{toast && <div className="reference-window-toast">{toast}</div>}</>;
}
