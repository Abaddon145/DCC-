import { useEffect, useState } from "react";
import { api } from "../lib/api";

export function MediaAssociationPicker({ ids, target, onClose, onAdded, notify }: {
  ids: string[]; target: "project" | "board"; onClose: () => void; onAdded: () => Promise<void>;
  notify: (message: string, error?: boolean) => void;
}) {
  const [options, setOptions] = useState<{ id: string; name: string }[]>([]);
  const [selected, setSelected] = useState(""); const [busy, setBusy] = useState(false);
  useEffect(() => { let active=true; const load = target === "project" ? api.listProjects(false) : api.listReferenceBoards();
    void load.then(values => { if(active){setOptions(values);setSelected(values[0]?.id||"");} }).catch(error => notify(String(error),true));
    return()=>{active=false};
  }, [target, notify]);
  const add = async () => {
    if (!selected || busy) return;
    setBusy(true);
    try {
      let added=0, skipped=0; const warnings:string[]=[];
      for (let index=0;index<ids.length;index+=5000) {
        const chunk=ids.slice(index,index+5000);
        if(target==="project")added+=await api.linkProjectMedia(chunk,[selected]);
        else {const report=await api.addMediaImagesToBoard(selected,chunk,{x:index,y:0});added+=report.items.length;skipped+=report.skipped;warnings.push(...report.warnings);}
      }
      notify(`已${target==="project"?"关联":"复制"} ${added} 项${skipped?`，跳过 ${skipped} 项`:""}${warnings.length?`：${warnings.join("；")}`:""}`,warnings.length>0);
      await onAdded();onClose();
    } catch(error){notify(`加入失败：${String(error)}`,true)}finally{setBusy(false)}
  };
  return <div className="modal-backdrop"><section className="modal-card project-picker" role="dialog" aria-modal="true" aria-label={target==="project"?"媒体加入项目":"图片加入参考板"}>
    <header><h2>{target==="project"?"媒体加入项目":"图片加入参考板"}</h2></header>
    <p>操作全部 {ids.length} 项明确选中的媒体。{target==="board"?"仅图片可复制；原件和已有参考板快照不会改变。":"重复关联会跳过，不复制或删除媒体原件。"}</p>
    <label>选择目标<select value={selected} onChange={event=>setSelected(event.target.value)} disabled={busy}>{options.map(option=><option key={option.id} value={option.id}>{option.name}</option>)}</select></label>
    {!options.length&&<p>请先创建{target==="project"?"项目":"参考板"}。</p>}
    <footer><button className="secondary-button" disabled={busy} onClick={onClose}>取消</button><button className="primary-button" disabled={busy||!selected} onClick={()=>void add()}>{busy?"正在处理…":"确认加入"}</button></footer>
  </section></div>;
}
