import { useEffect, useState } from "react";
import { Grid3X3, Plus, X } from "lucide-react";
import type { ReferenceBoardSummary } from "../types";
import { api } from "../lib/api";

interface Props { imageIds: string[]; title: string; lockedBoardId?: string | null; onClose: () => void; notify: (message: string, error?: boolean) => void }

export function ReferenceBoardPicker({ imageIds, title, lockedBoardId, onClose, notify }: Props) {
  const [boards, setBoards] = useState<ReferenceBoardSummary[]>([]);
  const [busy, setBusy] = useState(false);
  const load = () => api.listReferenceBoards().then(setBoards).catch(error => notify(String(error), true));
  useEffect(() => { void load(); }, []);
  const add = async (board: ReferenceBoardSummary) => {
    if (!imageIds.length) return;
    if (lockedBoardId === board.id) { notify("该参考板正在悬浮窗口中编辑，请先收回", true); return; }
    setBusy(true);
    try {
      const detail = await api.getReferenceBoard(board.id);
      const placement = { x: (600 - detail.viewX) / detail.viewScale, y: (380 - detail.viewY) / detail.viewScale };
      const report = await api.addAssetImagesToBoard(board.id, imageIds, placement);
      notify(`已将 ${report.items.length} 张图片加入“${board.name}”${report.skipped ? `，跳过 ${report.skipped} 张` : ""}`, !report.items.length);
      if (report.items.length) onClose();
    } catch (error) { notify(`加入参考板失败：${String(error)}`, true); }
    finally { setBusy(false); }
  };
  const create = async () => {
    const name = window.prompt("参考板名称", "新参考板")?.trim(); if (!name) return;
    setBusy(true);
    try { const board = await api.createReferenceBoard(name); await load(); await add({ id: board.id, name: board.name, background: board.background, itemCount: 0, updatedAt: board.updatedAt, lastOpenedAt: board.updatedAt }); }
    catch (error) { notify(String(error), true); setBusy(false); }
  };
  return <div className="modal-backdrop"><section className="reference-picker" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">REFERENCE BOARD</span><h2>加入参考板</h2><p>{title} · {imageIds.length} 张图片</p></div><button className="icon-button" onClick={onClose}><X size={18} /></button></header>
    <div className="reference-picker-list">{boards.map(board => <button key={board.id} disabled={busy || lockedBoardId === board.id} onClick={() => void add(board)}><span style={{ background: board.background }}><Grid3X3 size={20} /></span><div><strong>{board.name}</strong><small>{lockedBoardId === board.id ? "正在悬浮窗口中编辑" : `${board.itemCount} 张图片`}</small></div></button>)}{!boards.length && <p>还没有参考板，可以先创建一块。</p>}</div>
    <footer className="modal-footer"><button className="secondary-button" disabled={busy} onClick={() => void create()}><Plus size={15} />新建参考板并加入</button><button className="secondary-button" onClick={onClose}>取消</button></footer>
  </section></div>;
}
