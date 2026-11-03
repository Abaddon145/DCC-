import {useEffect,useState} from "react";
import {learningApi} from "./api";
import type {MembershipChange,MembershipPreview} from "./api";
import type {Page} from "../../types";

type Choice="keep"|"materials"|"learning"|"both";

export function MembershipMigration({libraryId}:{libraryId:string}) {
  const [page,setPage]=useState<Page<MembershipPreview>|null>(null);
  const [offset,setOffset]=useState(0),[refresh,setRefresh]=useState(0);
  const [choices,setChoices]=useState<Record<string,Choice>>({});
  const [busy,setBusy]=useState(false),[error,setError]=useState(""),[notice,setNotice]=useState("");
  useEffect(()=>{let active=true;setPage(null);setError("");void learningApi.listMembershipPreview(libraryId,offset,50).then(value=>{if(active)setPage(value)}).catch(reason=>{if(active)setError(`读取失败：${String(reason)}`)});return()=>{active=false};},[libraryId,offset,refresh]);
  const changes:MembershipChange[]=(page?.items||[]).flatMap(item=>{
    const choice=choices[item.mediaId]||"keep";
    if(choice==="keep")return [];
    const materials=choice!=="learning",learning=choice!=="materials";
    if(materials===item.materials&&learning===item.learning)return [];
    return [{mediaId:item.mediaId,expectedMaterials:item.materials,expectedLearning:item.learning,expectedSnapshot:item.snapshot,materials,learning}];
  });
  const apply=async()=>{
    if(busy||!changes.length||!window.confirm(`确认调整 ${changes.length} 个视频的归属？不会移动或删除视频文件。`))return;
    setBusy(true);setError("");setNotice("");
    try{await learningApi.applyMembershipChanges(libraryId,changes);window.dispatchEvent(new CustomEvent("learning-memberships-changed",{detail:{libraryId}}));setChoices({});setNotice(`已调整 ${changes.length} 个视频的归属。`);setRefresh(value=>value+1);}
    catch(reason){setChoices({});setPage(null);setError(`未能确认归属：${String(reason)}。请重新读取并检查。`);}
    finally{setBusy(false);}
  };
  return <section aria-label="整理已有视频归属">
    <h3>整理已有视频归属</h3>
    <p>选择视频属于素材库、学习库或两者。调整只改变归属，不移动或删除文件；有课程的视频必须保留学习归属。</p>
    {error&&<p role="alert">{error}<button type="button" disabled={busy} onClick={()=>setRefresh(value=>value+1)}>重新读取</button></p>}
    {notice&&<p role="status">{notice}</p>}
    {!page&&!error&&<p role="status">正在读取视频…</p>}
    {page&&<><p>共 {page.total} 个视频；项目与素材关联仅供核对，不会自动删除。</p>
      {page.items.length===0?<p>没有可整理的视频。</p>:<ul>{page.items.map(item=><li key={item.mediaId}>
        <strong>{item.name}</strong> · 当前：{item.materials?"素材库":""}{item.materials&&item.learning?" / ":""}{item.learning?"学习库":""}{!item.materials&&!item.learning?"未归属":""} · 项目 {item.projectCount} · 素材关联 {item.assetCount}{item.hasCourse?" · 已有关联课程":""}
        <label>目标归属<select aria-label={`${item.name}的归属`} value={choices[item.mediaId]||"keep"} disabled={busy} onChange={event=>setChoices(old=>({...old,[item.mediaId]:event.target.value as Choice}))}>
          <option value="keep">保持不变</option><option value="learning">仅学习库</option><option value="materials" disabled={item.hasCourse}>仅素材库</option><option value="both">两个库</option>
        </select></label>
      </li>)}</ul>}
      <button type="button" disabled={busy||!changes.length} onClick={()=>void apply()}>确认调整归属</button>
      <button type="button" disabled={busy||!changes.length} onClick={()=>setChoices({})}>清除本页选择</button>
      <nav aria-label="视频归属分页"><button type="button" disabled={busy||!!changes.length||offset===0} onClick={()=>setOffset(value=>Math.max(0,value-50))}>上一页</button><span>{offset+1}—{Math.min(offset+page.items.length,page.total)} / {page.total}</span><button type="button" disabled={busy||!!changes.length||offset+page.items.length>=page.total} onClick={()=>setOffset(value=>value+50)}>下一页</button></nav>
    </>}
  </section>;
}
