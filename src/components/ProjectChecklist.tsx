import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { beginPointerReorder } from "../lib/drag";
import { LoadFeedback, ReadFailure } from "./LoadFeedback";

const defaults = {
  still: ["构图与视觉重点检查", "模型与材质检查", "灯光与曝光检查", "渲染噪点与色彩检查", "分辨率与输出格式检查"],
  scene: ["场景比例与白盒检查", "模型拓扑与 UV 检查", "材质与贴图路径检查", "灯光与性能检查", "工程与最终输出检查"],
  animation: ["镜头与帧范围检查", "动画与穿插检查", "材质与灯光检查", "序列完整性与帧率检查", "声音同步与最终交付检查"],
};
interface Template { id: string | null; name: string; titles: string[] }
const ue5: Template = { id: null, name: "UE5 场景制作", titles: ["参考收集与构图", "白盒与镜头规划", "素材与 Nanite 检查", "材质与贴图", "Lumen 灯光与曝光", "性能与资源检查", "最终渲染与交付"] };

export function ProjectChecklist({ projectId, kind, onAdded, notify }: { projectId: string; kind: keyof typeof defaults; onAdded: () => Promise<void>; notify: (text: string, error?: boolean) => void }) {
  const [open, setOpen] = useState(false), [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false), [failed, setFailed] = useState(false);
  const [saved, setSaved] = useState<Template[]>([]), [libraryId, setLibraryId] = useState("");
  const [draft, setDraft] = useState<Template>({ id: null, name: "交付检查", titles: defaults[kind] });
  const [selected, setSelected] = useState<number[]>(defaults[kind].map((_, i) => i));
  const operation = useRef(false), generation = useRef(0), cleanup = useRef<(() => void) | null>(null);
  const dialog = useRef<HTMLElement>(null);
  const load = async () => { const version = ++generation.current; setLoading(true); setFailed(false); try { const data = await invoke<{ libraryId: string; templates: Template[] }>("list_workflow_templates"); if (version !== generation.current) return; setLibraryId(data.libraryId); setSaved(data.templates); } catch (e) { if (version === generation.current) { setFailed(true); notify(String(e), true); } } finally { if (version === generation.current) setLoading(false); } };
  useEffect(() => { if (open) void load(); return () => { generation.current++; cleanup.current?.(); }; }, [open]);
  useEffect(() => { if (!open) return; const previous = document.activeElement as HTMLElement | null; dialog.current?.focus(); return () => { if (previous?.isConnected) previous.focus(); }; }, [open]);
  const choose = (value: Template) => { setDraft({ ...value, titles: [...value.titles] }); setSelected(value.titles.map((_, i) => i)); };
  const run = async (work: () => Promise<void>) => { if (operation.current) return; operation.current = true; setBusy(true); try { await work(); } catch (e) { notify(String(e), true); } finally { operation.current = false; setBusy(false); } };
  const reorder = (from: number, to: number) => { const rows = draft.titles.map((title, index) => ({ title, checked: selected.includes(index) })); const [row] = rows.splice(from, 1); rows.splice(to, 0, row); setDraft(value => ({ ...value, titles: rows.map(row => row.title) })); setSelected(rows.flatMap((row, index) => row.checked ? [index] : [])); };
  const valid = draft.name.trim() && draft.titles.length > 0 && draft.titles.every(title => title.trim());
  const close = () => { if (!operation.current) setOpen(false); };
  const save = (copy=false) => void run(async () => { const value=await invoke<Template>("save_workflow_template",{libraryId,input:{...draft,id:copy?null:draft.id}}); choose(value); await load(); notify("流程模板已保存"); });
  return <><button className="secondary-button" onClick={() => setOpen(true)}>工作流程模板</button>{open && <div className="modal-backdrop" onPointerDown={e => { if (e.target === e.currentTarget) close(); }}>
    <section ref={dialog} tabIndex={-1} className="modal-card workflow-dialog" role="dialog" aria-modal="true" aria-label="工作流程模板" onKeyDown={e => { e.stopPropagation(); if (e.key === "Escape") { e.preventDefault(); close(); } else if (e.key === "Tab") { const nodes = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled),input:not(:disabled)")); const first = nodes[0], last = nodes.at(-1); if (e.shiftKey && (document.activeElement === first || document.activeElement === e.currentTarget)) { e.preventDefault(); last?.focus(); } else if (!e.shiftKey && (document.activeElement === last || document.activeElement === e.currentTarget)) { e.preventDefault(); first?.focus(); } } }}>
      <header><div><h2>工作流程模板</h2><p>保存在当前素材库，可复用于本库所有项目。</p></div><button disabled={busy} onClick={close}>关闭</button></header>
      {loading ? <LoadFeedback label="正在读取流程模板…" /> : failed ? <ReadFailure onRetry={() => void load()} /> : <>
        <div className="workflow-presets"><button disabled={busy} onClick={() => choose({ id: null, name: "交付检查", titles: defaults[kind] })}>交付检查</button><button disabled={busy} onClick={() => choose(ue5)}>UE5 场景制作</button>{saved.map(value => <button key={value.id} className={value.id === draft.id ? "active" : ""} disabled={busy} onClick={() => choose(value)}>{value.name}</button>)}</div>
        <label>模板名称<input autoFocus aria-label="模板名称" disabled={busy} value={draft.name} maxLength={100} onChange={e => setDraft(value => ({ ...value, name: e.target.value }))} /></label>
        <p>勾选需要添加的任务。拖动左侧手柄排序，最多 30 项。</p>
        <div className="workflow-titles">{draft.titles.map((title, i) => <div key={i} data-workflow-index={i} className="workflow-row"><button className="workflow-grip" disabled={busy} title="拖动排序" aria-label={`拖动任务 ${i + 1}`} onPointerDown={e => { cleanup.current?.(); cleanup.current = beginPointerReorder(e, { source: e.currentTarget.parentElement!, selector: "[data-workflow-index]", onDrop: (target, after) => { let to = Number(target.dataset.workflowIndex) + (after ? 1 : 0); if (i < to) to--; reorder(i, to); } }); }}>⋮⋮</button><input type="checkbox" aria-label={`选择任务 ${i + 1}`} disabled={busy} checked={selected.includes(i)} onChange={e => setSelected(value => e.target.checked ? [...value, i] : value.filter(index => index !== i))} /><input aria-label={`清单任务 ${i + 1}`} disabled={busy} value={title} maxLength={300} onChange={e => setDraft(value => ({ ...value, titles: value.titles.map((v, index) => index === i ? e.target.value : v) }))} /><button aria-label={`上移任务 ${i + 1}`} disabled={busy || i === 0} onClick={() => reorder(i, i - 1)}>↑</button><button aria-label={`下移任务 ${i + 1}`} disabled={busy || i === draft.titles.length - 1} onClick={() => reorder(i, i + 1)}>↓</button><button aria-label={`删除任务 ${i + 1}`} disabled={busy} onClick={() => { setDraft(value => ({ ...value, titles: value.titles.filter((_, index) => index !== i) })); setSelected(value => value.filter(index => index !== i).map(index => index > i ? index - 1 : index)); }}>删除</button></div>)}</div>
        <button disabled={busy || draft.titles.length >= 30} onClick={() => { setDraft(value => ({ ...value, titles: [...value.titles, ""] })); setSelected(value => [...value, draft.titles.length]); }}>添加任务标题</button>
        <footer><button disabled={busy || !valid || !libraryId} onClick={()=>save()}>保存模板</button><button disabled={busy || !valid || !draft.id || !libraryId} onClick={()=>save(true)}>另存为新模板</button>{draft.id && <button className="danger" disabled={busy} onClick={() => { if (window.confirm("删除此流程模板？已添加到项目的任务不会删除。")) void run(async () => { await invoke("delete_workflow_template", { libraryId, id: draft.id }); choose({ id: null, name: "交付检查", titles: defaults[kind] }); await load(); }); }}>删除模板</button>}<button className="primary-button" disabled={busy || !valid || !selected.length || !libraryId} onClick={() => void run(async () => { const count = await invoke<number>("add_project_checklist", { projectId, libraryId, titles: draft.titles.filter((_, i) => selected.includes(i)) }); notify(`已添加 ${count} 项待办；同名任务已跳过`); setOpen(false); await onAdded(); })}>{busy ? "处理中…" : `添加 ${selected.length} 项到项目`}</button></footer>
      </>}
    </section>
  </div>}</>;
}
