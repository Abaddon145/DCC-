import {useState} from "react";
import {coreApi} from "./api";
import type {CoreAssetDetail} from "./types";
export type RunAction=(action:()=>Promise<unknown>)=>Promise<boolean>;
export function TagPanel({detail,libraryId,locale,busy,run}:{detail:CoreAssetDetail;libraryId:string;locale:string;busy:boolean;run:RunAction}){
  const [text,setText]=useState("");
  const names=detail.asset.tags;
  return <section><h3>标签</h3><div className="core-tags">{detail.tagDetails.map(tag=><span key={tag.id}><label title={`设置 ${tag.name} 标签颜色`}><input aria-label={`${tag.name} 标签颜色`} type="color" value={/^#[0-9a-f]{6}$/i.test(tag.color)?tag.color:"#d99a42"} disabled={busy} onChange={e=>void run(()=>coreApi.color(libraryId,tag.id,e.target.value))}/>{tag.name}</label><button disabled={busy} aria-label={`移除标签 ${tag.name}`} onClick={()=>void run(()=>coreApi.tags(libraryId,detail.asset.id,locale,names.filter(n=>n!==tag.name)))}>×</button></span>)}</div><form onSubmit={async e=>{e.preventDefault();if(await run(()=>coreApi.tags(libraryId,detail.asset.id,locale,[...names,...text.split(/[;；\n]/).map(s=>s.trim()).filter(Boolean)])))setText("");}}><label>新增标签<input value={text} maxLength={1000} onChange={e=>setText(e.target.value)} placeholder="用分号分隔"/></label><button disabled={busy||!text.trim()}>添加标签</button></form></section>;
}
