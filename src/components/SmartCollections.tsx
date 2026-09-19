import { useState } from "react";
import { ChevronDown, ChevronRight, Copy, MoreHorizontal, Pencil, Plus, Sparkles, Trash2, TriangleAlert } from "lucide-react";
import type { SmartCollection } from "../types";
import { HoverDismissDetails } from "./HoverDismissDetails";

interface Props {
  items: SmartCollection[]; activeId: string | null; onOpen: (item: SmartCollection) => void; onCreate: () => void;
  onRename: (item: SmartCollection) => void; onDuplicate: (item: SmartCollection) => void; onDelete: (item: SmartCollection) => void; onReorder: (ids: string[]) => void;
}
export function SmartCollections({ items, activeId, onOpen, onCreate, onRename, onDuplicate, onDelete, onReorder }: Props) {
  const [expanded, setExpanded] = useState(true); const [dragId, setDragId] = useState<string | null>(null);
  return <div className="smart-collections"><button className="smart-collections-title" onClick={() => setExpanded(value => !value)}>{expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}<Sparkles size={14} />智能集合<span>{items.length}</span></button>{expanded && <div className="smart-collection-list">{items.map(item => <div key={item.id} draggable onDragStart={() => setDragId(item.id)} onDragOver={event => event.preventDefault()} onDrop={() => { if (!dragId || dragId === item.id) return; const ids = items.map(value => value.id).filter(id => id !== dragId); ids.splice(ids.indexOf(item.id), 0, dragId); setDragId(null); onReorder(ids); }} className={activeId === item.id ? "active" : ""}><button onClick={() => onOpen(item)}><i style={{ background: item.color }} />{item.invalidConditions.length ? <TriangleAlert size={12} /> : <Sparkles size={12} />}<span title={item.name}>{item.name}</span></button><HoverDismissDetails><summary><MoreHorizontal size={13} /></summary><div className="menu-popover"><button onClick={() => onRename(item)}><Pencil size={12} />重命名</button><button onClick={() => onDuplicate(item)}><Copy size={12} />复制</button><button className="danger" onClick={() => onDelete(item)}><Trash2 size={12} />删除</button></div></HoverDismissDetails></div>)}<button className="smart-add" onClick={onCreate}><Plus size={12} />保存当前搜索</button></div>}</div>;
}
