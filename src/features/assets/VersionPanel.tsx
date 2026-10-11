import {useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";
import {coreApi} from "./api";
import type {CoreAssetDetail} from "./types";
import type {RunAction} from "./TagPanel";
import type {MediaKind,MediaImportReport} from "../../types";
export async function waitCoreTask(id:string,isCurrent:()=>boolean){
  for(;;){if(!isCurrent())throw new Error("页面已关闭，已完成导入保留，未追加版本");const task=await api.getLibraryTask(id);if(!isCurrent())throw new Error("页面已关闭");if(!["queued","running"].includes(task.status))return task;await new Promise(r=>setTimeout(r,500));}
}
const kinds:Record<string,MediaKind>={jpg:"image",jpeg:"image",png:"image",webp:"image",gif:"image",bmp:"image",tif:"image",tiff:"image",mp4:"video",mov:"video",mkv:"video",webm:"video",mp3:"audio",wav:"audio",flac:"audio",ogg:"audio",glb:"model",gltf:"model",fbx:"model",obj:"model",stl:"model"};
export function VersionPanel({detail,libraryId,busy,run,onPreview,isCurrent}:{detail:CoreAssetDetail;libraryId:string;busy:boolean;run:RunAction;onPreview:(id:string)=>void;isCurrent:()=>boolean}){
 const originals=detail.files.filter(f=>f.role==="original"&&f.available);
 const [file,setFile]=useState(originals[0]?.id||""),[label,setLabel]=useState("v001"),[note,setNote]=useState("");
 const append=()=>run(async()=>{
   const selected=await open({multiple:false,title:"追加版本原件",filters:[{name:"媒体原件",extensions:Object.keys(kinds)}]});if(typeof selected!=="string"||!isCurrent())return;
   const kind=kinds[selected.split(".").pop()?.toLowerCase()||""];if(!kind)throw new Error("不支持此原件格式");
   const task=await api.startLibraryTask({kind:"mediaImport",request:{kind,paths:[selected],folderId:null,tags:[],favorite:false}},libraryId);window.dispatchEvent(new Event("library-task-started"));
   const completed=await waitCoreTask(task.id,isCurrent);const report=completed.result as MediaImportReport|null;const row=report?.rows[0];
   const entryId=row?.entryId||row?.duplicateEntryId;if(!entryId)throw new Error(completed.message||row?.message||"导入未完成，未创建版本");
   const entry=await api.getMediaEntry(entryId);if(!isCurrent())throw new Error("页面已关闭，导入原件保留");
   const attached=await coreApi.attach(libraryId,detail.asset.id,{kind:"mediaFile",id:entry.primaryFileId,role:"original"});
   if(!isCurrent())throw new Error("页面已关闭，已附加文件保留，未创建版本");
   await coreApi.createVersion(libraryId,{assetId:detail.asset.id,fileId:attached.id,version:label,note});
 });
 return <section><h3>版本</h3>{detail.versions.map(version=><div key={version.id} className="core-file-row"><div><strong>{version.version}</strong><p>{version.note||"无版本说明"}</p></div><button disabled={busy||!detail.files.some(f=>f.id===version.fileId&&f.available)} onClick={()=>onPreview(version.fileId)}>预览 {version.version}</button><button disabled={busy} aria-label={`删除版本 ${version.version}`} onClick={()=>{if(window.confirm("只删除版本记录，原件文件会保留。确认？"))void run(()=>coreApi.deleteVersion(libraryId,version.id));}}>删除版本</button></div>)}<form onSubmit={e=>{e.preventDefault();void run(()=>coreApi.createVersion(libraryId,{assetId:detail.asset.id,fileId:file,version:label,note}));}}><label>版本标签<input value={label} onChange={e=>setLabel(e.target.value)} maxLength={80} required placeholder="v001 / v002 / Final"/></label><label>版本说明<textarea value={note} onChange={e=>setNote(e.target.value)} maxLength={10000}/></label><label>已登记原件<select value={file} onChange={e=>setFile(e.target.value)}><option value="">选择原件</option>{originals.map(f=><option key={f.id} value={f.id}>{f.filename}</option>)}</select></label><div className="core-actions"><button disabled={busy||!file||!label.trim()}>创建版本</button><button type="button" disabled={busy||!label.trim()} onClick={()=>void append()}>导入原件并创建版本</button></div></form></section>;
}
