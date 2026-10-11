import { useEffect, useState,useRef } from "react";
import { ArchiveRestore, Trash2 } from "lucide-react";
import {LoadFeedback,ReadFailure} from "./LoadFeedback";
import { api } from "../lib/api";
import { NoteTrashList } from "./NoteTrashList";
import type { TrashBatch } from "../types";

interface Props { notify: (message: string, error?: boolean) => void; onChanged: () => Promise<void>; onOpenNote?:(id:string)=>void }

export function TrashPanel({ notify, onChanged, onOpenNote }: Props) {
  const [items, setItems] = useState<TrashBatch[]>([]);
  const mutation=useRef(false);const [busy,setBusy]=useState(false),[failed,setFailed]=useState(false);
  const [loading, setLoading] = useState(true);
  const load = async () => { setFailed(false);setLoading(true); try { setItems((await api.listTrash()).items); } catch (error) { setFailed(true);notify(String(error), true); } finally { setLoading(false); } };
  useEffect(() => { void load(); }, []);
  const restore = async (item: TrashBatch) => { if(mutation.current)return;mutation.current=true;setBusy(true);try { await api.restoreTrashBatch(item.id); notify(`已恢复“${item.label}”`); await Promise.all([load(), onChanged()]); } catch (error) { notify(String(error), true); } finally{mutation.current=false;setBusy(false);} };
  const purge = async (item: TrashBatch) => { if (mutation.current||!window.confirm(`永久删除“${item.label}”？相关预览图片也会被移除，且无法撤销。`)) return; mutation.current=true;setBusy(true);try { await api.purgeTrashBatch(item.id); notify("已永久删除"); await Promise.all([load(), onChanged()]); } catch (error) { notify(String(error), true); } finally{mutation.current=false;setBusy(false);} };
  const empty = async () => { if (mutation.current||!items.length || !window.confirm(`永久清空回收站中的 ${items.length} 个删除批次？此操作无法撤销。`)) return; mutation.current=true;setBusy(true);try { const count = await api.emptyTrash(); notify(`已清空 ${count} 个删除批次`); await Promise.all([load(), onChanged()]); } catch (error) { notify(String(error), true); } finally{mutation.current=false;setBusy(false);} };
  return <section className="trash-panel"><header><div><span className="eyebrow">RECYCLE BIN</span><h1>回收站</h1><p>删除内容会一直保留，直到你手动永久清理。</p></div><button className="secondary-button danger" disabled={busy||!items.length} onClick={() => void empty()}><Trash2 size={15} />清空资产回收站</button></header>
    {failed?<ReadFailure onRetry={()=>void load()}/>:loading ? <LoadFeedback label="正在读取回收站…" skeleton/> : !items.length ? <div className="trash-empty"><ArchiveRestore size={40} /><strong>资产回收站为空</strong><span>已删除的素材、Prompt、Shot、分类和媒体会显示在这里。</span></div> : <div className="trash-list">{items.map(item => <article key={item.id}><ArchiveRestore size={22} /><div><strong>{item.label}</strong><span>{item.assetCount} 项资产 · {item.categoryCount} 个分类 · {item.mediaCount} 个媒体 · {item.mediaFolderCount} 个媒体文件夹</span><time>{new Date(item.createdAt).toLocaleString("zh-CN")}</time></div><button className="secondary-button" disabled={busy} onClick={() => void restore(item)}>恢复</button><button className="secondary-button danger" disabled={busy} onClick={() => void purge(item)}>永久删除</button></article>)}</div>}
  <NoteTrashList notify={notify} onOpen={onOpenNote}/></section>;
}
