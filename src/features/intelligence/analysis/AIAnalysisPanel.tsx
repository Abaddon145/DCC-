import {useEffect,useRef,useState} from "react";
import {LoadFeedback} from "../../../components/LoadFeedback";
import {api} from "../../../lib/api";
import {intelligenceApi} from "../api";
import type {LibraryTask} from "../../../types";
import type {AnalysisInputPreview,AnalysisMode,AssetAnalysis} from "../types";
import "../intelligence.css";
type Props={libraryId:string;assetId:string;locale:string;onChanged:()=>void};
export function AIAnalysisPanel(props:Props) {return <AnalysisPanel key={`${props.libraryId}:${props.assetId}:${props.locale}`} {...props}/>;}
function AnalysisPanel({libraryId,assetId,locale,onChanged}:Props){
 const [mode,setMode]=useState<AnalysisMode>("text"),[preview,setPreview]=useState<AnalysisInputPreview|null>(null),[saved,setSaved]=useState<AssetAnalysis|null>(null),[task,setTask]=useState<LibraryTask|null>(null);
 const [force,setForce]=useState(false),[selected,setSelected]=useState<string[]>([]),[busy,setBusy]=useState(false),[loading,setLoading]=useState(true),[error,setError]=useState(""),[feedback,setFeedback]=useState("");
 const live=useRef(false),lock=useRef(false),refreshLock=useRef(false);
 const running=!!task&&["running","queued"].includes(task.status);
 useEffect(()=>{
   live.current=true;void intelligenceApi.getAnalysis(libraryId,assetId).then(value=>{if(live.current)setSaved(value)}).catch(e=>{if(live.current)setError(String(e))}).finally(()=>{if(live.current)setLoading(false)});
   return()=>{live.current=false};
 },[libraryId,assetId]);
 useEffect(()=>{
   if(!task||!running)return;
   let active=true;
   const refresh=async()=>{
     if(refreshLock.current)return;refreshLock.current=true;
     try{
       const value=await api.getLibraryTask(task.id);if(!active||!live.current||value.libraryId!==libraryId)return;
       if(value.status==="completed"){
         const resultData=value.result as {analysisId?:string}|null;
         const result=await intelligenceApi.getAnalysis(libraryId,assetId,resultData?.analysisId);if(!active||!live.current)return;setSaved(result);setSelected([]);setPreview(null);setFeedback("分析已保存；标签需勾选后追加。");onChanged();
       }else if(!["running","queued"].includes(value.status))setError(value.message||"分析已停止；已有结果仍保留。");
       setTask(value);
     }catch(e){if(active&&live.current)setError(`读取任务失败：${String(e)}，可在任务中心查看。`)}finally{refreshLock.current=false}
   };
   const completed=(event:Event)=>{const value=(event as CustomEvent<LibraryTask>).detail;if(value?.id===task.id&&value.libraryId===libraryId)void refresh()};
   const timer=window.setInterval(()=>void refresh(),1500);window.addEventListener("library-task-completed",completed);
   return()=>{active=false;window.clearInterval(timer);window.removeEventListener("library-task-completed",completed)};
 },[task?.id,running,libraryId,assetId,onChanged]);
 const action=async(fn:()=>Promise<void>)=>{if(lock.current)return;lock.current=true;setBusy(true);setError("");setFeedback("");try{await fn()}catch(e){if(live.current)setError(String(e))}finally{lock.current=false;if(live.current)setBusy(false)}};
 const prepare=()=>action(async()=>{const value=await intelligenceApi.prepareAnalysis(libraryId,assetId,mode);if(live.current){setPreview(value);setForce(false)}});
 const start=()=>action(async()=>{
   if(!preview)return;if(force&&!window.confirm("强制重新分析会再次调用 DeepSeek 并使用额度，是否继续？"))return;
   const value=await api.startLibraryTask({kind:"aiAnalyze",request:{assetId,previewId:preview.id,force}},libraryId);
   if(live.current){setTask(value);window.dispatchEvent(new Event("library-task-started"));}
 });
 const apply=()=>action(async()=>{if(!saved||!selected.length)return;await intelligenceApi.applyAnalysisTags(libraryId,assetId,saved.id,locale,selected);if(live.current){setSelected([]);setFeedback("所选标签已追加，已有标签保留。");onChanged()}});
 return <section className="intelligence-analysis" aria-busy={busy||loading} aria-label="AI 素材分析">
   <h3>AI 素材分析</h3><p className="intelligence-hint">仅分析当前素材。先核对实际发送资料，再确认使用在线额度；不会自动覆盖描述或标签。</p>
   {loading&&<LoadFeedback label="正在读取本地分析…"/>}{error&&<p role="alert">{error}</p>}{feedback&&<p role="status">{feedback}</p>}
   <div className="core-actions"><label>资料范围<select value={mode} disabled={busy||running} onChange={e=>{setMode(e.target.value as AnalysisMode);setPreview(null)}}><option value="text">名称、描述与标签</option><option value="visual">图片 / 视频采样 / 模型缩略图</option></select></label><button disabled={busy||running} onClick={()=>void prepare()}>预览分析资料</button></div>
   {busy&&<LoadFeedback label="正在处理，请稍候…"/>}
   {preview&&<div className="analysis-preview"><h4>待发送资料 · {preview.model}</h4><pre>{preview.summary}</pre><div className="analysis-preview-images">{preview.images.map((image,i)=><figure key={i}><img src={image.dataUrl} alt={image.label}/><figcaption>{image.label}</figcaption></figure>)}</div>{preview.warnings.map(w=><p key={w} className="intelligence-hint">{w}</p>)}<p>本机保守预留约 {preview.estimatedTokens} Token，实际以服务返回用量为准。</p>{preview.cachedAnalysisId&&<p>相同资料有本地缓存，复用不调用在线服务。</p>}<label><input type="checkbox" checked={force} disabled={busy||running} onChange={e=>setForce(e.target.checked)}/>强制重新分析（再次使用额度）</label><button className="primary-button" disabled={busy||running} onClick={()=>void start()}>{preview.cachedAnalysisId&&!force?"确认并复用缓存":"确认并开始分析"}</button></div>}
   {running&&<div role="status"><LoadFeedback label={task.status==="queued"?"等待分析任务…":"正在分析已确认资料…"}/><button disabled={busy} onClick={()=>void action(async()=>{await api.cancelLibraryTask(task.id);if(live.current)setFeedback("已请求停止；已发送请求可能产生用量。")})}>停止分析</button></div>}
   {saved&&<div className="analysis-result"><h4>已保存分析</h4><p className="intelligence-hint">{saved.mode==="visual"?"预览画面分析":"文字资料分析"} · {saved.createdAt} · {saved.actualTokens===null?"用量未返回":`${saved.actualTokens} Token`}。历史结果不会随素材修改自动更新。</p><dl>{([["description","描述"],["scene","场景"],["style","风格"],["camera","镜头"],["lighting","灯光"],["mood","情绪"]] as const).map(([key,label])=><div key={key}><dt>{label}</dt><dd>{saved[key]||"未知 / 未提供"}</dd></div>)}</dl><h4>建议标签</h4><div className="analysis-keywords">{saved.keywords.map(tag=><label key={tag}><input type="checkbox" checked={selected.includes(tag)} disabled={busy} onChange={e=>setSelected(values=>e.target.checked?[...values,tag]:values.filter(v=>v!==tag))}/>{tag}</label>)}</div><button disabled={busy||!selected.length} onClick={()=>void apply()}>追加所选标签</button></div>}
   {!loading&&!saved&&<p className="intelligence-hint">尚无分析结果。导入素材不会自动调用 AI。</p>}
 </section>;
}
