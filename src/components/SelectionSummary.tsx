import { useEffect, useState } from "react";
import { api } from "../lib/api";

export function SelectionSummary({ ids, filterKey, getMatchingIds, media = false }: {
  ids: Set<string>; filterKey: string; getMatchingIds: () => Promise<string[]>; media?: boolean;
}) {
  const [matching,setMatching]=useState<Set<string>|null>(null);
  const [open,setOpen]=useState(false); const [page,setPage]=useState(0);
  const [labels,setLabels]=useState<{id:string;name:string}[]>([]);
  const [error,setError]=useState("");
  useEffect(()=>{let active=true;setMatching(null);if(ids.size)void getMatchingIds().then(values=>{if(active)setMatching(new Set(values))}).catch(()=>undefined);return()=>{active=false}},[filterKey,ids]);
  useEffect(()=>{if(!open)return;let active=true;setError("");setLabels([]);const selected=[...ids].slice(page*100,page*100+100);void api.getSelectionLabels(selected,media).then(values=>{if(active)setLabels(values)}).catch(reason=>{if(active)setError(String(reason))});return()=>{active=false}},[open,page,ids,media]);
  const visible=matching?[...ids].filter(id=>matching.has(id)).length:null;
  return <><button className="link-button selection-scope" onClick={()=>{setOpen(true);setPage(0)}}>{visible===null?`查看全部选择 ${ids.size} 项`:`当前条件 ${visible} 项 · 隐藏 ${ids.size-visible} 项 · 查看清单`}</button>
    {open&&<div className="modal-backdrop"><section className="modal-card selection-dialog" role="dialog" aria-modal="true" aria-label="完整选中清单"><header><h2>完整选中清单 · {ids.size} 项</h2></header><p>批量操作会作用于全部明确选中的条目，包括筛选之外的条目。</p>{error&&<p role="alert">{error}</p>}<ol start={page*100+1}>{labels.map(item=><li key={item.id}>{item.name||"条目已不可用"}<small>{item.id}</small></li>)}</ol><footer><button disabled={!page} onClick={()=>setPage(value=>value-1)}>上一页</button><span>{page+1} / {Math.max(1,Math.ceil(ids.size/100))}</span><button disabled={(page+1)*100>=ids.size} onClick={()=>setPage(value=>value+1)}>下一页</button><button className="secondary-button" onClick={()=>setOpen(false)}>关闭</button></footer></section></div>}
  </>;
}
