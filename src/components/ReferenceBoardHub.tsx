import { useEffect, useState } from "react";
import { Copy, Grid3X3, MoreHorizontal, Plus, Trash2 } from "lucide-react";
import type { ReferenceBoardDetail, ReferenceBoardSummary } from "../types";
import { api } from "../lib/api";
import { HoverDismissDetails } from "./HoverDismissDetails";
import { ReferenceBoardWorkspace } from "./ReferenceBoardWorkspace";

interface Props {
  detachedBoardId: string | null;
  onDetached: (boardId: string) => void;
  notify: (message: string, error?: boolean) => void;
}

export function ReferenceBoardHub({ detachedBoardId, onDetached, notify }: Props) {
  const [boards, setBoards] = useState<ReferenceBoardSummary[]>([]);
  const [active, setActive] = useState<ReferenceBoardDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const loadBoards = async () => { try { setBoards(await api.listReferenceBoards()); } catch (error) { notify(`读取参考板失败：${String(error)}`, true); } finally { setLoading(false); } };
  useEffect(() => { void loadBoards(); }, []);
  useEffect(() => {
    if (!active || detachedBoardId === active.id) return;
    void api.getReferenceBoard(active.id).then(setActive).catch(() => setActive(null));
  }, [detachedBoardId]);

  const create = async () => {
    const name = window.prompt("参考板名称", `参考板 ${boards.length + 1}`)?.trim(); if (!name) return;
    try { const board = await api.createReferenceBoard(name); setActive(board); await loadBoards(); }
    catch (error) { notify(String(error), true); }
  };
  const openBoard = async (id: string) => { try { setLoading(true); setActive(await api.getReferenceBoard(id)); } catch (error) { notify(String(error), true); } finally { setLoading(false); } };
  const rename = async (board: ReferenceBoardSummary) => {
    if (detachedBoardId === board.id) { notify("请先收回悬浮参考板", true); return; }
    const name = window.prompt("新的参考板名称", board.name)?.trim(); if (!name || name === board.name) return;
    try { await api.renameReferenceBoard(board.id, name); if (active?.id === board.id) setActive(current => current ? { ...current, name } : current); await loadBoards(); }
    catch (error) { notify(String(error), true); }
  };
  const duplicate = async (board: ReferenceBoardSummary) => { if (detachedBoardId === board.id) { notify("请先收回悬浮参考板", true); return; } try { const copied = await api.duplicateReferenceBoard(board.id); await loadBoards(); setActive(copied); notify("参考板已复制"); } catch (error) { notify(String(error), true); } };
  const remove = async (board: ReferenceBoardSummary) => {
    if (detachedBoardId === board.id) { notify("请先收回悬浮参考板", true); return; }
    if (!window.confirm(`确定删除参考板“${board.name}”及其中的独立图片副本吗？`)) return;
    try { await api.deleteReferenceBoard(board.id); if (active?.id === board.id) setActive(null); await loadBoards(); notify("参考板已删除"); } catch (error) { notify(String(error), true); }
  };
  const detach = async (boardId: string) => { await api.openReferenceWindow(boardId); onDetached(boardId); };

  if (active) {
    if (detachedBoardId === active.id) return <div className="reference-detached-placeholder"><Grid3X3 size={42} /><h2>{active.name}</h2><p>该参考板正在独立悬浮窗口中编辑。</p><button className="secondary-button" onClick={() => setActive(null)}>返回参考板列表</button></div>;
    return <ReferenceBoardWorkspace key={active.id} initialBoard={active} onBack={() => { setActive(null); void loadBoards(); }} onDetach={detach} onChanged={loadBoards} notify={notify} />;
  }

  return <section className="reference-hub">
    <header><div><span className="eyebrow">REFERENCE BOARDS</span><h1>参考板</h1><p>{boards.length} 个参考板 · 自动保存在当前素材库</p></div><button className="primary-button" onClick={() => void create()}><Plus size={17} />新建参考板</button></header>
    {loading ? <div className="reference-hub-loading">正在读取参考板…</div> : boards.length ? <div className="reference-board-list">{boards.map(board => <article key={board.id} onDoubleClick={() => void openBoard(board.id)}>
      <button className="reference-board-open" onClick={() => void openBoard(board.id)}><div style={{ background: board.background }}><Grid3X3 size={29} /></div><strong>{board.name}</strong><span>{board.itemCount} 张图片</span><small>{new Date(board.updatedAt).toLocaleString("zh-CN")}</small></button>
      <HoverDismissDetails className="reference-board-menu"><summary><MoreHorizontal size={17} /></summary><div className="menu-popover"><button onClick={() => void rename(board)}>重命名</button><button onClick={() => void duplicate(board)}><Copy size={13} />复制参考板</button><button className="danger" onClick={() => void remove(board)}><Trash2 size={13} />删除</button></div></HoverDismissDetails>
    </article>)}</div> : <div className="reference-hub-empty"><Grid3X3 size={40} /><h2>创建第一块参考板</h2><p>把素材预览图、本地图片和剪贴板截图放到同一个无限画布中。</p><button className="primary-button" onClick={() => void create()}><Plus size={17} />新建参考板</button></div>}
  </section>;
}
