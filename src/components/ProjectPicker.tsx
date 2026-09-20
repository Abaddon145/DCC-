import { BriefcaseBusiness, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { ProjectSummary } from "../types";

interface Props { assetIds: string[]; onClose: () => void; onAdded: () => void; notify: (message: string, error?: boolean) => void }

export function ProjectPicker({ assetIds, onClose, onAdded, notify }: Props) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => { void api.listProjects(false).then(values => { setProjects(values); setSelected(values[0]?.id || ""); }).catch(error => notify(String(error), true)); }, []);
  const add = async () => {
    if (!selected || !assetIds.length) return;
    setBusy(true);
    try { const count = await api.addProjectAssets(selected, assetIds); notify(count ? `已将 ${count} 项素材加入项目` : "这些素材已在项目中"); onAdded(); onClose(); }
    catch (error) { notify(`加入项目失败：${String(error)}`, true); }
    finally { setBusy(false); }
  };
  return <div className="modal-backdrop"><section className="project-picker modal-card">
    <header><div><span className="eyebrow">PROJECT BASKET</span><h2>加入创作项目</h2></div><button className="icon-button" onClick={onClose}><X size={18} /></button></header>
    {projects.length ? <><label>选择项目<select value={selected} onChange={event => setSelected(event.target.value)}>{projects.map(project => <option value={project.id} key={project.id}>{project.name}</option>)}</select></label><p>将 {assetIds.length} 项素材加入项目素材篮，默认标记为“候选”。</p></> : <div className="empty-state compact"><BriefcaseBusiness size={30} /><strong>还没有可用项目</strong><span>请先在“创作项目”模块中新建项目。</span></div>}
    <footer><button className="secondary-button" onClick={onClose}>取消</button><button className="primary-button" disabled={!selected || busy} onClick={() => void add()}>{busy ? "正在加入…" : "加入项目"}</button></footer>
  </section></div>;
}
