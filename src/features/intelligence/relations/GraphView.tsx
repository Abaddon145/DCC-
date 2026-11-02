import {useEffect,useId,useRef,useState} from "react";
import {LoadFeedback,ReadFailure} from "../../../components/LoadFeedback";
import {coreApi} from "../../assets/api";
import type {Asset} from "../../assets/types";
import {intelligenceApi} from "../api";
import {relationNames,type AssetGraph,type RelationType} from "../types";
import "../intelligence.css";

export interface GraphViewProps {libraryId:string;assetId:string;locale:string;onOpenAsset:(id:string)=>void;onChanged:()=>void}
export function GraphView({libraryId,assetId,locale,onOpenAsset,onChanged}:GraphViewProps) {
  const identity=`${libraryId}:${assetId}:${locale}`;
  const scope=useRef(identity);scope.current=identity;
  const operation=useRef<symbol|null>(null);
  const [result,setResult]=useState<{scope:string;value:AssetGraph}|null>(null);
  const [offset,setOffset]=useState(0),[refresh,setRefresh]=useState(0),[error,setError]=useState(""),[reading,setReading]=useState(true),[busy,setBusy]=useState(false);
  const [query,setQuery]=useState(""),[candidates,setCandidates]=useState<Asset[]>([]),[searching,setSearching]=useState(false),[searchError,setSearchError]=useState("");
  const [target,setTarget]=useState<Asset|null>(null),[kind,setKind]=useState<RelationType>("related_to"),[reverse,setReverse]=useState(false);
  const marker=useId().replace(/:/g,"");
  const graph=result?.scope===identity?result.value:null;
  useEffect(()=>{setOffset(0);setQuery("");setCandidates([]);setTarget(null);setError("");setBusy(false);operation.current=null;},[identity]);
  useEffect(()=>{
    let active=true;setReading(true);setError("");
    void intelligenceApi.graph(libraryId,assetId,locale,offset).then(value=>{if(active)setResult({scope:identity,value});}).catch(e=>{if(active)setError(String(e));}).finally(()=>{if(active)setReading(false);});
    return()=>{active=false;};
  },[libraryId,assetId,locale,identity,offset,refresh]);
  useEffect(()=>{
    let active=true;setCandidates([]);setSearchError("");
    if(!query.trim()){setSearching(false);return;}
    setSearching(true);
    const timer=setTimeout(()=>{void coreApi.search(libraryId,{query:query.trim(),assetType:null,source:null,locale,offset:0,limit:40}).then(page=>{if(active)setCandidates(page.items.filter(a=>a.id!==assetId&&!a.deleted));}).catch(e=>{if(active)setSearchError(String(e));}).finally(()=>{if(active)setSearching(false);});},180);
    return()=>{active=false;clearTimeout(timer);};
  },[libraryId,assetId,locale,query]);
  useEffect(()=>()=>{scope.current="";},[]);
  const mutate=async(action:()=>Promise<unknown>)=>{
    if(operation.current)return;
    const token=Symbol();operation.current=token;setBusy(true);setError("");
    try {await action();if(scope.current===identity){setTarget(null);setQuery("");setRefresh(r=>r+1);onChanged();}}
    catch(e){if(scope.current===identity)setError(String(e));}
    finally{if(operation.current===token){operation.current=null;if(scope.current===identity)setBusy(false);}}
  };
  const nodes=new Map(graph?.nodes.map(a=>[a.id,a])||[]);
  const neighbors=graph?.nodes.filter(a=>a.id!==assetId)||[];
  const positions=new Map(neighbors.map((a,i)=>[a.id,{x:i%2===0?95:385,y:122+Math.floor(i/2)*76}]));
  positions.set(assetId,{x:240,y:42});
  const symmetric=kind==="related_to"||kind==="variant_of";
  return <section className="intelligence-relations" aria-label="资产关系管理" aria-busy={busy||reading}>
    <div className="core-actions"><h3>资产关系</h3><button disabled={reading||busy} onClick={()=>setRefresh(r=>r+1)}>刷新关系</button></div>
    <p className="intelligence-hint">箭头从资产指向它的来源或用途；变体与相关为双向关系。只显示当前资产的一跳关联。</p>
    {error&&<div role="alert"><p>{error}</p><ReadFailure onRetry={()=>setRefresh(r=>r+1)}/></div>}
    {reading&&<LoadFeedback label="正在读取资产关系…"/>}
    {graph&&<>
      <p role="status">关联资产 {graph.total} 项 · 当前显示 {neighbors.length} 项</p>
      {!graph.relations.length&&<p>暂无资产关系</p>}
      {!!neighbors.length&&<div className="intelligence-graph-scroll"><svg viewBox={`0 0 480 ${Math.max(200,Math.ceil(neighbors.length/2)*76+110)}`} role="group" aria-label="局部资产关系图">
        <defs><marker id={marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z"/></marker></defs>
        {graph.relations.map(r=>{const from=positions.get(r.sourceAssetId),to=positions.get(r.targetAssetId);if(!from||!to)return null;const bidirectional=r.relationType==="variant_of"||r.relationType==="related_to";const shift=(Object.keys(relationNames).indexOf(r.relationType)-3)*3;return <path key={r.id} data-relation-id={r.id} data-source={r.sourceAssetId} data-target={r.targetAssetId} d={`M ${from.x} ${from.y+20} C ${from.x+shift} ${from.y+48}, ${to.x+shift} ${to.y-48}, ${to.x} ${to.y-20}`} fill="none" markerEnd={`url(#${marker})`} markerStart={bidirectional?`url(#${marker})`:undefined}><title>{nodes.get(r.sourceAssetId)?.name} — {relationNames[r.relationType]} {bidirectional?"↔":"→"} {nodes.get(r.targetAssetId)?.name}</title></path>;})}
        {graph.nodes.map(a=>{const p=positions.get(a.id);if(!p)return null;return <g key={a.id} transform={`translate(${p.x},${p.y})`} role="button" aria-label={`在关系图打开 ${a.name}`} aria-disabled={a.deleted} tabIndex={a.deleted?-1:0} className={a.deleted?"graph-node deleted":"graph-node"} onClick={()=>{if(!a.deleted)onOpenAsset(a.id);}} onKeyDown={e=>{if(!a.deleted&&(e.key==="Enter"||e.key===" ")){e.preventDefault();onOpenAsset(a.id);}}}><title>{a.name}{a.deleted?" · 已在回收站":""}</title><rect x="-70" y="-20" width="140" height="40" rx="8"/><text textAnchor="middle" dominantBaseline="central">{a.name.length>15?`${a.name.slice(0,14)}…`:a.name}</text></g>;})}
      </svg></div>}
      <ul className="intelligence-relation-list" aria-label="资产关系列表">{graph.relations.map(r=>{const neighbor=nodes.get(r.sourceAssetId===assetId?r.targetAssetId:r.sourceAssetId);const bidirectional=r.relationType==="variant_of"||r.relationType==="related_to";return <li key={r.id}><div><span>{nodes.get(r.sourceAssetId)?.name}</span><span className="relation-label">{relationNames[r.relationType]} {bidirectional?"↔":"→"}</span><span>{nodes.get(r.targetAssetId)?.name}</span>{neighbor?.deleted&&<small>暂不可用 · 已在回收站</small>}</div><div className="core-actions"><button disabled={neighbor?.deleted} onClick={()=>neighbor&&onOpenAsset(neighbor.id)} aria-label={`打开关联资产 ${neighbor?.name}`}>打开</button><button disabled={busy||graph.center.deleted} aria-label={`移除关系 ${relationNames[r.relationType]} ${neighbor?.name}`} onClick={()=>void mutate(()=>intelligenceApi.deleteRelation(libraryId,assetId,r.id))}>移除关系</button></div></li>;})}</ul>
      {graph.total>graph.limit&&<div className="core-pagination"><button disabled={reading||busy||offset===0} onClick={()=>setOffset(o=>Math.max(0,o-80))}>关系上一页</button><span>{Math.floor(offset/80)+1} / {Math.ceil(graph.total/80)}</span><button disabled={reading||busy||offset+80>=graph.total} onClick={()=>setOffset(o=>o+80)}>关系下一页</button></div>}
      {!graph.center.deleted&&<form onSubmit={e=>{e.preventDefault();if(!target)return;void mutate(()=>intelligenceApi.createRelation(libraryId,{sourceAssetId:reverse&&!symmetric?target.id:assetId,targetAssetId:reverse&&!symmetric?assetId:target.id,relationType:kind}));}}>
        <h4>添加关联</h4><label>搜索关联资产<input value={query} onChange={e=>{setQuery(e.target.value);setTarget(null);}} placeholder="输入名称或标签，选择当前库的资产"/></label>
        {searching&&<LoadFeedback label="正在搜索关联资产…"/>}{searchError&&<p role="alert">{searchError}</p>}
        {!!query.trim()&&!searching&&!searchError&&!candidates.length&&<p>没有匹配的可用资产</p>}
        <div className="intelligence-candidates">{candidates.map(a=><button key={a.id} type="button" aria-label={`选择关联资产 ${a.name}`} aria-pressed={target?.id===a.id} onClick={()=>setTarget(a)}>{a.name}</button>)}</div>
        <label>关系类型<select value={kind} onChange={e=>setKind(e.target.value as RelationType)}>{Object.entries(relationNames).map(([value,name])=><option key={value} value={value}>{name}</option>)}</select></label>
        {!symmetric&&<label>关系方向<select value={reverse?"reverse":"forward"} onChange={e=>setReverse(e.target.value==="reverse")}><option value="forward">当前资产 → 所选资产</option><option value="reverse">所选资产 → 当前资产</option></select></label>}
        <p>{target?`${reverse&&!symmetric?target.name:graph.center.name} — ${relationNames[kind]} ${symmetric?"↔":"→"} ${reverse&&!symmetric?graph.center.name:target.name}`:"先选择一个关联资产"}</p>
        <button disabled={busy||!target} type="submit">{busy?"正在保存…":"添加关系"}</button>
      </form>}
    </>}
  </section>;
}
