import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import type { UeLink } from "../types";
import { NoteReferencePicker } from "./NoteReferencePicker";
import { ReadFailure } from "./LoadFeedback";
import { useRequestGate } from "../lib/workspace";

export function UeLinkedAssets({kind,entityId,notify,onOpen}:{kind:UeLink["kind"];entityId:string;notify:(message:string,error?:boolean)=>void;onOpen:(id:string)=>void}) {
  const [links,setLinks]=useState<UeLink[]>([]),[libraryId,setLibraryId]=useState(""),[picking,setPicking]=useState(false),[failed,setFailed]=useState(false),[busy,setBusy]=useState(false);
  const gate=useRequestGate();
  const load=useCallback(async()=>{if(!libraryId)return;const revision=gate.begin();setFailed(false);try{const values=await api.getUeLinks(libraryId,null,kind,entityId);if(!Array.isArray(values))throw new Error("关联资产响应无效，请更新软件后重试");if(gate.isCurrent(revision))setLinks(values);}catch(error){if(gate.isCurrent(revision)){setFailed(true);notify(String(error),true);}}},[libraryId,kind,entityId,notify,gate]);
  useEffect(()=>{let active=true;void api.getWorkspaceContext("ueLibrary").then(value=>{if(active)setLibraryId(value.libraryId);}).catch(()=>{});return()=>{active=false;};},[]);
  useEffect(()=>{void load();},[load]);
  const [shown,setShown]=useState(50);
  const mutate=async(id:string,remove:boolean)=>{if(busy||!libraryId)return;setBusy(true);try{await api.linkUeEntity(libraryId,[id],kind,entityId,remove);setPicking(false);await load();}catch(error){notify(String(error),true);}finally{setBusy(false);}};
  return <section className="knowledge-backlinks ue-linked-assets"><header><h3>关联 UE 原生资产 · {links.length}</h3><button className="secondary-button compact" disabled={busy||!libraryId} onClick={()=>setPicking(true)}>关联资产</button></header>{failed&&<ReadFailure onRetry={()=>void load()}/>}<ul>{links.slice(0,shown).map(link=><li key={link.assetId}><button onClick={()=>onOpen(link.assetId)}>{link.assetName}</button><button disabled={busy} aria-label={`解除关联 ${link.assetName}`} onClick={()=>void mutate(link.assetId,true)}>解除</button></li>)}</ul>{shown<links.length&&<button onClick={()=>setShown(value=>value+50)}>显示更多</button>}{!failed&&!links.length&&<p>只关联原生条目，不复制或移动 UE 工程包。</p>}{picking&&<NoteReferencePicker libraryId={libraryId} initialKind="ue" kinds={["ue"]} title="关联 UE 原生资产" onClose={()=>setPicking(false)} onPick={value=>void mutate(value.targetId,false)}/>}</section>;
}
