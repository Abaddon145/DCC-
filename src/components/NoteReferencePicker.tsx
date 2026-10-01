import { useEffect, useMemo, useState } from "react";
import { knowledgeApi, type KnowledgeReference } from "../lib/knowledge";

export function NoteReferencePicker({libraryId,onPick,onClose}:{libraryId:string;onPick:(value:KnowledgeReference)=>void;onClose:()=>void}) {
  const client=useMemo(()=>knowledgeApi(libraryId),[libraryId]);
  const [kind,setKind]=useState("note");const [query,setQuery]=useState("");const [items,setItems]=useState<KnowledgeReference[]>([]);const [error,setError]=useState("");const [loading,setLoading]=useState(false);
  useEffect(()=>{let active=true;setLoading(true);const timer=setTimeout(()=>{void client.referenceTargets(kind,query).then(v=>{if(active){setItems(v);setError("");}}).catch(e=>{if(active)setError(String(e));}).finally(()=>{if(active)setLoading(false);});},180);return()=>{active=false;clearTimeout(timer);};},[client,kind,query]);
  return <div className="modal-backdrop"><section className="modal-card hierarchy-name-dialog" role="dialog" aria-modal="true" aria-label="插入引用"><header><h2>关联学习与创作内容</h2><button onClick={onClose}>关闭</button></header><select aria-label="引用类型" value={kind} onChange={e=>setKind(e.target.value)}><option value="note">笔记</option><option value="asset">素材</option><option value="media">图片 / 模型 / 音频 / 视频</option><option value="project">创作项目</option></select><input autoFocus aria-label="搜索引用" placeholder="输入名称搜索（最多显示 30 项）" value={query} onChange={e=>setQuery(e.target.value)}/>{error&&<p role="alert">{error}</p>}<div className="knowledge-reference-results" aria-busy={loading}>{items.map(item=><button key={item.targetId} disabled={loading} onClick={()=>onPick(item)}>{item.label}</button>)}</div><p>只保存关联，不复制素材或媒体文件。来源删除后仍保留引用名称。</p></section></div>;
}
