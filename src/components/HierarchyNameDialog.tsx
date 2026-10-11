import { X } from "lucide-react";

interface Props {
  kind: "分类" | "文件夹";
  editing: boolean;
  name: string;
  busy: boolean;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function HierarchyNameDialog({ kind, editing, name, busy, onNameChange, onClose, onSubmit }: Props) {
  const title = editing ? `重命名${kind}` : `新建${kind}`;
  return <div className="modal-backdrop" onMouseDown={event => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <form className="modal-card hierarchy-name-dialog" role="dialog" aria-modal="true" aria-label={title} onSubmit={event => { event.preventDefault(); if (name.trim() && !busy) onSubmit(); }}>
      <header><div><span className="eyebrow">LIBRARY ORGANIZATION</span><h2>{title}</h2></div><button className="icon-button" type="button" aria-label="关闭" disabled={busy} onClick={onClose}><X size={18} /></button></header>
      <label htmlFor="hierarchy-name">{kind}名称<input id="hierarchy-name" autoFocus maxLength={100} value={name} onChange={event => onNameChange(event.target.value)} onKeyDown={event => { if (event.key === "Escape" && !busy) onClose(); }} placeholder={`输入${kind}名称`} /></label>
      <footer><button className="secondary-button" type="button" disabled={busy} onClick={onClose}>取消</button><button className="primary-button" type="submit" disabled={!name.trim() || busy}>{busy ? "保存中…" : editing ? "保存名称" : `创建${kind}`}</button></footer>
    </form>
  </div>;
}
