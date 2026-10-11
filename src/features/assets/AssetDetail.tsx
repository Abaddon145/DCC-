import {useEffect,useRef,useState} from "react";
import {X} from "lucide-react";
import {LoadFeedback,ReadFailure} from "../../components/LoadFeedback";
import {ImageLightbox} from "../../components/ImageLightbox";
import {ModelViewer} from "../../components/ModelViewer";
import {api,mediaLibraryBundleUrl} from "../../lib/api";
import type {ProjectSummary} from "../../types";
import {coreApi} from "./api";
import {fileUrl,typeNames} from "./AssetCard";
import {TagPanel} from "./TagPanel";
import {SourcePanel} from "./SourcePanel";
import {VersionPanel} from "./VersionPanel";
import {GraphView} from "../intelligence/relations/GraphView";
import {AIAnalysisPanel} from "../intelligence/analysis/AIAnalysisPanel";
import {PromptEditor} from "../intelligence/prompt/PromptEditor";
import {ShotPanel} from "../intelligence/shot/ShotPanel";
import {intelligenceApi} from "../intelligence/api";
import type {AssetFile,CoreAssetDetail} from "./types";
import type {AssetBrowserProps} from "./AssetBrowser";
const detailTabs=["基础信息","文件","Metadata","标签","来源","版本","关系","AI分析","Prompt","项目"] as const;
type DetailTab=typeof detailTabs[number];
export function AssetDetail({assetId,initialTab="基础信息",onClose,onChanged,onSelect,...props}:AssetBrowserProps&{assetId:string;initialTab?:DetailTab;onClose:()=>void;onChanged:()=>void;onSelect:(id:string)=>void}){
 const [tab,setTab]=useState<DetailTab>(initialTab),[promptVisited,setPromptVisited]=useState(initialTab==="Prompt"),[shotVisited,setShotVisited]=useState(initialTab==="项目"),[dirty,setDirty]=useState(false),[pending,setPending]=useState<(()=>void)|null>(null),[deleting,setDeleting]=useState(false);
 const [detail,setDetail]=useState<CoreAssetDetail|null>(null),[error,setError]=useState(""),[revision,setRevision]=useState(0),[busy,setBusy]=useState(false),[project,setProject]=useState(""),[projects,setProjects]=useState<ProjectSummary[]>([]),[preview,setPreview]=useState<string|null>(null);
 const mounted=useRef(false),operation=useRef(false),close=useRef<HTMLButtonElement>(null);
 const navigate=(action:()=>void)=>dirty?setPending(()=>action):action();
 const chooseTab=(next:DetailTab)=>{setTab(next);if(next==="Prompt")setPromptVisited(true);if(next==="项目")setShotVisited(true);};
 useEffect(()=>{mounted.current=true;close.current?.focus();return()=>{mounted.current=false;};},[]);
 useEffect(()=>{let active=true;setError("");void coreApi.get(props.libraryId,assetId,props.locale).then(data=>{if(active)setDetail(data);}).catch(e=>{if(active)setError(String(e));});return()=>{active=false;};},[props.libraryId,assetId,props.locale,revision]);
 useEffect(()=>{let active=true;void api.listProjects().then(p=>{if(active)setProjects(p);}).catch(()=>{});return()=>{active=false;};},[props.libraryId]);
 useEffect(()=>{const handler=(e:KeyboardEvent)=>{if(e.key==="Escape"){if(preview)setPreview(null);else if(pending)setPending(null);else navigate(onClose);}};window.addEventListener("keydown",handler);return()=>window.removeEventListener("keydown",handler);},[onClose,preview,dirty,pending]);
 const run=async(action:()=>Promise<unknown>)=>{if(operation.current)return false;operation.current=true;setBusy(true);setError("");try{await action();if(mounted.current){setRevision(r=>r+1);onChanged();}return mounted.current;}catch(e){if(mounted.current)setError(String(e));return false;}finally{operation.current=false;if(mounted.current)setBusy(false);}};
 const file=detail?.files.find(f=>f.id===preview);
 return <><button className="core-detail-backdrop" aria-label="关闭详情背景" onClick={()=>navigate(onClose)}/><aside className="core-detail" role="dialog" aria-modal="true" aria-label="资产详情" aria-busy={busy}>
 <header><div><span className="eyebrow">ASSET CORE</span><h2>{detail?.asset.name||"资产详情"}</h2></div><button ref={close} aria-label="关闭资产详情" onClick={()=>navigate(onClose)}><X size={18}/></button></header>
 <nav className="core-detail-tabs" role="tablist" aria-label="资产详情栏目">{detailTabs.map((label,index)=><button key={label} id={`asset-tab-${index}`} role="tab" aria-selected={tab===label} aria-controls={`asset-panel-${index}`} tabIndex={tab===label?0:-1} onClick={()=>chooseTab(label)} onKeyDown={e=>{const delta=e.key==="ArrowRight"?1:e.key==="ArrowLeft"?-1:0;let next=delta?(index+delta+detailTabs.length)%detailTabs.length:e.key==="Home"?0:e.key==="End"?detailTabs.length-1:-1;if(next>=0){e.preventDefault();chooseTab(detailTabs[next]);e.currentTarget.parentElement?.querySelector<HTMLButtonElement>(`#asset-tab-${next}`)?.focus();}}}>{label}</button>)}</nav>
 <div className="core-detail-scroll">{!detail&&!error&&<LoadFeedback label="正在读取资产详情…" skeleton/>}{error&&<div role="alert"><p>{error}</p>{!detail&&<ReadFailure onRetry={()=>setRevision(r=>r+1)}/>}</div>}
 {detail&&<>
 <div id="asset-panel-0" role="tabpanel" aria-labelledby="asset-tab-0" hidden={tab!=="基础信息"}><p>{typeNames[detail.asset.assetType]} · {detail.asset.deleted?"已在回收站":"当前素材库"}</p>{detail.asset.origin.kind!=="entity"&&<button onClick={()=>navigate(()=>props.onOpenOrigin(detail.asset.origin,detail.asset.assetType))}>打开原详情</button>}<p className="core-description">{detail.asset.description||"暂无描述"}</p>
 <div className="core-actions"><button onClick={()=>chooseTab("关系")}>查看资产关系</button><button disabled={detail.asset.deleted} onClick={()=>chooseTab("AI分析")}>查看 AI 分析</button>{detail.asset.assetType==="prompt"&&<button onClick={()=>chooseTab("Prompt")}>编辑 Prompt</button>}{detail.asset.origin.kind==="entity"&&!detail.asset.deleted&&<button disabled={busy} onClick={()=>setDeleting(true)}>删除文本资产</button>}</div>
 <section><h3>知识与相关资产</h3>{detail.knowledgeLinks.map(n=><button key={n.id} onClick={()=>navigate(()=>props.onOpenNote?.(n.id))}>{n.name}</button>)}{detail.relatedAssets.map(a=><button key={a.id} onClick={()=>navigate(()=>onSelect(a.id))}>{a.name}</button>)}{!detail.knowledgeLinks.length&&!detail.relatedAssets.length&&<p>暂无关联</p>}</section>
 <section><h3>处理记录</h3>{detail.processingHistory.map(task=><div key={task.id}><strong>{task.label} · {task.status}</strong><p>{task.message||task.phase}</p></div>)}{!detail.processingHistory.length&&<p>暂无关联任务</p>}</section></div>
 <div id="asset-panel-1" role="tabpanel" aria-labelledby="asset-tab-1" hidden={tab!=="文件"}><section><h3>托管文件</h3>{detail.files.map(f=><div key={f.id} className="core-file-row"><div><strong>{f.filename}</strong><small>{f.role} · {f.extension} · {f.size.toLocaleString()} B · {f.available?"可用":"文件缺失"}</small></div>{f.role==="original"&&<button disabled={!f.available} onClick={()=>setPreview(f.id)}>预览</button>}</div>)}{!detail.files.length&&<p>{detail.asset.origin.kind==="entity"?"文本资产不需要本地文件。":"无本地原件，Fab 与网盘功能可在原详情使用。"}</p>}</section></div>
 <div id="asset-panel-2" role="tabpanel" aria-labelledby="asset-tab-2" hidden={tab!=="Metadata"}><section><h3>文件元数据</h3>{detail.metadata.map(m=><p key={m.fileId}>{detail.files.find(f=>f.id===m.fileId)?.filename} · {[m.width&&m.height?`${m.width} × ${m.height}`:null,m.duration!==null?`${m.duration.toFixed(2)} 秒`:null,m.codec,m.fps!==null?`${m.fps.toFixed(2)} FPS`:null].filter(Boolean).join(" · ")}</p>)}{!detail.metadata.length&&<p>暂无文件元数据。</p>}<button disabled={busy||detail.asset.deleted||!detail.files.some(f=>f.role==="original")} onClick={()=>void run(async()=>{await api.startLibraryTask({kind:"assetProcess",request:{assetId,force:false}},props.libraryId);window.dispatchEvent(new Event("library-task-started"));props.notify("已加入元数据/缩略图补齐任务");})}>补齐元数据与缩略图</button><button disabled={busy} onClick={()=>setRevision(r=>r+1)}>重新读取详情</button></section></div>
 <div id="asset-panel-3" role="tabpanel" aria-labelledby="asset-tab-3" hidden={tab!=="标签"}><TagPanel detail={detail} libraryId={props.libraryId} locale={props.locale} busy={busy||detail.asset.deleted} run={run}/></div>
 <div id="asset-panel-4" role="tabpanel" aria-labelledby="asset-tab-4" hidden={tab!=="来源"}><SourcePanel detail={detail} libraryId={props.libraryId} busy={busy||detail.asset.deleted} run={run}/></div>
 <div id="asset-panel-5" role="tabpanel" aria-labelledby="asset-tab-5" hidden={tab!=="版本"}><VersionPanel detail={detail} libraryId={props.libraryId} busy={busy||detail.asset.deleted} run={run} onPreview={setPreview} isCurrent={()=>mounted.current}/></div>
 <div id="asset-panel-6" role="tabpanel" aria-labelledby="asset-tab-6" hidden={tab!=="关系"}>{tab==="关系"&&<GraphView libraryId={props.libraryId} assetId={assetId} locale={props.locale} onOpenAsset={id=>navigate(()=>onSelect(id))} onChanged={onChanged}/>}</div>
 <div id="asset-panel-7" role="tabpanel" aria-labelledby="asset-tab-7" hidden={tab!=="AI分析"}>{tab==="AI分析"&&(detail.asset.deleted?<p>回收站资产请先恢复再分析。</p>:<AIAnalysisPanel libraryId={props.libraryId} assetId={assetId} locale={props.locale} onChanged={()=>{setRevision(r=>r+1);onChanged();}}/>)}</div>
 <div id="asset-panel-8" role="tabpanel" aria-labelledby="asset-tab-8" hidden={tab!=="Prompt"}>{detail.asset.assetType==="prompt"?(detail.asset.deleted?<p>请先恢复资产再编辑。</p>:promptVisited&&<PromptEditor libraryId={props.libraryId} assetId={assetId} onDirty={setDirty} onSaved={()=>{setRevision(r=>r+1);onChanged();}}/>):<p>此资产不是 Prompt，可在资产关系中连接 Prompt。</p>}</div>
 <div id="asset-panel-9" role="tabpanel" aria-labelledby="asset-tab-9" hidden={tab!=="项目"}>{detail.asset.assetType==="shot"&&shotVisited&&(detail.asset.deleted?<p>请先恢复 Shot 再编辑。</p>:<ShotPanel libraryId={props.libraryId} assetId={assetId} onDirty={setDirty} onRelations={()=>chooseTab("关系")} onSaved={()=>{setRevision(r=>r+1);onChanged();}}/>)}<section><h3>关联项目</h3>{detail.projects.map(p=><div key={p.id} className="core-file-row"><button onClick={()=>navigate(()=>props.onOpenProject?.(p.id))}>{p.name}</button><button disabled={busy||detail.asset.deleted} onClick={()=>void run(()=>coreApi.project(props.libraryId,assetId,p.id,false))}>解除关联</button></div>)}<label>加入项目<select value={project} onChange={e=>setProject(e.target.value)}><option value="">不关联项目</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label><button disabled={busy||!project||detail.asset.deleted} onClick={()=>void run(()=>coreApi.project(props.libraryId,assetId,project,true))}>关联所选项目</button></section></div>
 </>}
 {pending&&<div role="alertdialog" aria-label="未保存的资产草稿"><p>当前修改还未保存。关闭前可以继续编辑或复制草稿。</p><button onClick={()=>{const action=pending;setPending(null);setDirty(false);action();}}>放弃修改并继续</button><button onClick={()=>setPending(null)}>继续编辑</button></div>}
 {deleting&&<div role="alertdialog" aria-label="删除文本资产"><p>将此文本资产移入回收站？可在回收站恢复。</p><button disabled={busy} onClick={()=>void run(()=>intelligenceApi.trashTextAssets(props.libraryId,[assetId])).then(ok=>{if(ok)onClose();})}>确认移入回收站</button><button onClick={()=>setDeleting(false)}>取消删除</button></div>}
 </div></aside>{file&&<CoreFilePreview file={file} onClose={()=>setPreview(null)}/>}</>;
}
function CoreFilePreview({file,onClose}:{file:AssetFile;onClose:()=>void}){
 const ref=file.preview||file.sourceRef,ext=file.extension.toLowerCase();
 if(file.sourceRef.kind==="image")return <ImageLightbox images={[{id:file.sourceRef.id,originalName:file.filename}]} initialIndex={0} title={file.filename} onClose={onClose}/>;
 if(["jpg","jpeg","png","webp","gif","bmp","tiff","tif"].includes(ext)&&file.sourceRef.kind==="mediaFile")return <ImageLightbox images={[{id:file.sourceRef.id,originalName:file.filename,mediaFileId:file.sourceRef.id}]} initialIndex={0} title={file.filename} onClose={onClose}/>;
 return <div className="lightbox core-preview" role="dialog" aria-label="文件预览" aria-modal="true"><header><strong>{file.filename}</strong><button onClick={onClose}>关闭预览</button></header><div>{["glb","gltf","fbx","obj","stl"].includes(ext)?<ModelViewer sourceUrl={file.mediaEntryId&&file.logicalPath?mediaLibraryBundleUrl(file.mediaEntryId,file.logicalPath):fileUrl(ref)} sourceName={file.filename}/>: ["mp3","wav","flac","ogg","opus"].includes(ext)?<audio controls src={fileUrl(ref)}/>: ["mp4","mov","mkv","webm"].includes(ext)?<video controls src={fileUrl(ref)}/>:<img src={fileUrl(ref)} alt={file.filename}/>}</div></div>;
}
