import {useState} from "react";
import {coreApi} from "./api";
import {api} from "../../lib/api";
import type {CoreAssetDetail} from "./types";
import type {RunAction} from "./TagPanel";
export function SourcePanel({detail,libraryId,busy,run}:{detail:CoreAssetDetail;libraryId:string;busy:boolean;run:RunAction}){
 const [type,setType]=useState("web"),[url,setUrl]=useState(""),[author,setAuthor]=useState("");
 return <section><h3>来源</h3>{detail.sources.map(source=><div className="core-file-row" key={source.id}><div><strong>{source.sourceType}</strong><p>{source.url||"本地导入"}</p><small>{source.author}{!source.editable?" · 在原详情中修改":""}</small></div>{source.url&&<button onClick={()=>void run(()=>api.openExternal(source.url))} disabled={busy}>打开链接</button>}{source.editable&&<button onClick={()=>void run(()=>coreApi.removeSource(libraryId,detail.asset.id,source.id))} disabled={busy}>移除来源</button>}</div>)}<details><summary>添加来源</summary><form onSubmit={async e=>{e.preventDefault();if(await run(()=>coreApi.addSource(libraryId,{assetId:detail.asset.id,sourceType:type,url,author}))) {setUrl("");setAuthor("");}}}><label>来源类型<select value={type} onChange={e=>setType(e.target.value)}>{["local","fab","youtube","pinterest","web","other"].map(value=><option key={value}>{value}</option>)}</select></label><label>来源链接<input value={url} onChange={e=>setUrl(e.target.value)} maxLength={8192} placeholder="https://…"/></label><label>来源作者<input value={author} onChange={e=>setAuthor(e.target.value)} maxLength={300}/></label><button disabled={busy||(!url.trim()&&type!=="local")}>保存来源</button></form></details></section>;
}
