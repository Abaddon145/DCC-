import {useEffect,useRef,useState} from "react";
import {learningApi,type CourseImportProgress,type CourseSourceBinding,type CourseSqlitePreview,type CourseSourceSeries} from "./api";
import {api} from "../../lib/api";
import type {MediaEntry} from "../../types";

const CONFLICT_LABEL:Record<string,string>={unresolved:"未绑定目标媒体",progress_absent:"来源进度缺失",divergent_progress:"来源和目标进度不一致",source_changed:"来源已变",target_changed:"目标已改",both_changed:"来源和目标均已变化",target_unavailable:"目标不可用",invalid_series:"系列标题或说明不符合要求",membership_may_be_withheld:"若课程未能完整建立或映射，系列将保留为冲突记录",shared_media:"多个来源课程共用同一媒体",repeated_membership:"系列成员重复",different_source_progress:"同一媒体的来源进度不一致",different_target_progress:"来源与目标进度存在差异"};
const UUID=/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const MAX_BATCH_ITEMS=10000;
function progressText(value:CourseImportProgress|null){
  return value?`${value.positionMs/1000} 秒；时长 ${value.durationMs===null?"未知":`${value.durationMs/1000} 秒`}；${value.rate} 倍速；${value.completed?"已完成":"未完成"}`:"未知";
}

export default function CourseSqliteImport({libraryId,source}:{libraryId:string;source:CourseSourceBinding}){
  const [preview,setPreview]=useState<CourseSqlitePreview|null>(null);
  const [bindings,setBindings]=useState<Record<string,string>>({});
  const [fresh,setFresh]=useState(false);
  const [approved,setApproved]=useState(false);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const [status,setStatus]=useState("");
  const [page,setPage]=useState(0);
  const [catalog,setCatalog]=useState<CourseSourceSeries[]|null>(null);const [selectedSeries,setSelectedSeries]=useState<string[]|null>(null);const [catalogPage,setCatalogPage]=useState(0);
  const [picker,setPicker]=useState("");const [query,setQuery]=useState("");const [videos,setVideos]=useState<MediaEntry[]>([]);const [videoOffset,setVideoOffset]=useState(0);const [videoTotal,setVideoTotal]=useState(0);const [videoLoading,setVideoLoading]=useState(false);const [videoError,setVideoError]=useState("");
  const videoRequest=useRef(0);
  const active=useRef(true);const pending=useRef(false);
  useEffect(()=>{active.current=true;return()=>{active.current=false;videoRequest.current++}},[]);

  async function searchVideos(offset:number){
    const request=++videoRequest.current;setVideoLoading(true);setVideoError("");
    try{const result=await api.searchMediaEntries({kind:"video",query,folderId:null,includeChildFolders:true,tags:[],formats:[],favoriteOnly:false,sort:"updated",offset,limit:50});if(active.current&&request===videoRequest.current){setVideos(result.items);setVideoOffset(offset);setVideoTotal(result.total)}}
    catch(reason){if(active.current&&request===videoRequest.current){setVideos([]);setVideoTotal(0);setVideoError(String(reason))}}
    finally{if(active.current&&request===videoRequest.current)setVideoLoading(false)}
  }

  async function preflight(){
    if(pending.current)return;
    setApproved(false);setFresh(false);setError("");setStatus("");
    const choices=Object.entries(bindings).filter(([,value])=>value.trim()).map(([sourceItemId,value])=>({sourceItemId,mediaId:value.trim()}));
    if(choices.some(item=>!UUID.test(item.mediaId))){setError("目标媒体必须填写已登记本地媒体的 UUID，不能填写文件路径。");return;}
    pending.current=true;setBusy(true);
    try{
      const next=await learningApi.previewCourseImport(libraryId,source.selectionId,choices,selectedSeries??undefined);
      if(!active.current)return;
      if(next.selectionId!==source.selectionId||!Number.isFinite(next.expiresAt)||next.expiresAt*1000<=Date.now())throw new Error("预检已失效，请重新预检课程数据库。");
      setPreview(next);setFresh(true);setPage(0);
    }catch(reason){if(active.current)setError(String(reason));}finally{pending.current=false;if(active.current)setBusy(false);}
  }
  async function loadCatalog(){
    if(pending.current)return;pending.current=true;setBusy(true);setError("");setFresh(false);setApproved(false);
    try{const result=await learningApi.listCourseSourceSeries(libraryId,source.selectionId);if(active.current){setCatalog(result);setSelectedSeries([]);setCatalogPage(0);setPreview(null);setBindings({})}}
    catch(reason){if(active.current)setError(String(reason))}finally{pending.current=false;if(active.current)setBusy(false)}
  }
  async function queue(){
    if(pending.current||!preview||!fresh||!approved)return;
    setApproved(false);setFresh(false);setError("");
    if(preview.expiresAt*1000<=Date.now()){setError("预检已过期，请重新预检课程数据库。");return;}
    pending.current=true;setBusy(true);
    try{
      const task=await learningApi.queueCourseImport(libraryId,source.selectionId,preview.previewId);
      if(!active.current)return;
      setStatus(`课程迁入任务 ${task.id} 已排队，请在任务中心查看结果。`);
      window.dispatchEvent(new Event("library-task-started"));
    }catch(reason){if(active.current)setError(String(reason));}finally{pending.current=false;if(active.current)setBusy(false);}
  }
  const rows=preview?.series.flatMap(series=>series.items.map(item=>({series,item})))||[];
  return <section className="learning-import-preview" aria-label="课程数据库预检" aria-busy={busy}>
    <p>请先关闭映流。预检仅复制所选数据库，复制期间短暂拒绝来源写入；不会用 SQLite 打开来源数据库。只在本机处理，不调用付费服务。</p>
    <p>可搜索并绑定当前库登记的视频，不会根据来源文件路径自动匹配。未绑定与进度冲突会保留在回执中。</p>
    <button type="button" disabled={busy} onClick={()=>void loadCatalog()}>选择系列分批迁入</button>
    {catalog&&<section aria-label="选择来源系列"><p>按完整系列分批，单批系列与课程合计最多 {MAX_BATCH_ITEMS} 项。预检还受来源及决策数据容量限制，超限时不会自动截断。</p>
      <ul>{catalog.slice(catalogPage*50,catalogPage*50+50).map(series=><li key={series.id}><label><input type="checkbox" checked={selectedSeries?.includes(series.id)||false} disabled={busy||series.courseCount+1>MAX_BATCH_ITEMS} onChange={event=>{setSelectedSeries(current=>event.target.checked?[...(current||[]),series.id]:(current||[]).filter(id=>id!==series.id));setPreview(null);setBindings({});setFresh(false);setApproved(false)}}/>{series.title} · {series.courseCount} 门 · {series.id}{series.courseCount+1>MAX_BATCH_ITEMS?"（系列超限）":""}</label></li>)}</ul>
      {catalog.length>50&&<div><button type="button" disabled={busy||catalogPage===0} onClick={()=>setCatalogPage(value=>value-1)}>上一页系列</button><span>第 {catalogPage+1} / {Math.ceil(catalog.length/50)} 页</span><button type="button" disabled={busy||(catalogPage+1)*50>=catalog.length} onClick={()=>setCatalogPage(value=>value+1)}>下一页系列</button></div>}
      <p>已选择 {selectedSeries?.length||0} 个系列，共 {catalog.filter(s=>selectedSeries?.includes(s.id)).reduce((sum,s)=>sum+s.courseCount+1,0)} 项。</p><button type="button" disabled={busy} onClick={()=>{setCatalog(null);setSelectedSeries(null);setPreview(null);setBindings({});setFresh(false);setApproved(false)}}>改为预检全部</button>
    </section>}
    <button type="button" disabled={busy||(selectedSeries!==null&&(selectedSeries.length===0||catalog!.filter(s=>selectedSeries.includes(s.id)).reduce((sum,s)=>sum+s.courseCount+1,0)>MAX_BATCH_ITEMS))} onClick={()=>void preflight()}>{preview?"重新预检课程数据库":"预检课程数据库"}</button>
    {busy&&<p role="status">正在处理课程迁入请求…</p>}
    {preview&&<>
      <p>预检有效至：{new Date(preview.expiresAt*1000).toLocaleString()}。修改绑定后必须重新预检。</p>
      <p>系列：{preview.series.map(series=>series.title).join("、")||"无"}</p>
      {preview.series.filter(series=>series.conflictCodes?.length).map(series=><p role="alert" key={series.id}>系列 {series.title}：{series.conflictCodes!.map(code=>CONFLICT_LABEL[code]||code).join("；")}</p>)}
      <div className="preview-table-wrap"><table className="preview-table" aria-label="课程绑定与进度"><thead><tr><th>系列 / 课程</th><th>目标媒体</th><th>进度对照与冲突</th></tr></thead><tbody>{rows.slice(page*100,page*100+100).map(({series,item})=><tr key={item.id}>
        <td><small>{series.title}</small><strong>{item.title}</strong></td>
        <td><label className="learning-import-path">{item.title} · 目标媒体 UUID<input value={bindings[item.id]??item.mediaId??""} disabled={busy} onChange={event=>{setBindings(current=>({...current,[item.id]:event.target.value}));setApproved(false);setFresh(false);setStatus("");}}/></label><button type="button" disabled={busy} aria-label={`为${item.title}选择本地视频`} onClick={()=>{videoRequest.current++;setPicker(item.id);setQuery("");setVideos([]);setVideoOffset(0);setVideoTotal(0);setVideoError("");setVideoLoading(false)}}>选择视频…</button></td>
        <td><p>来源进度：{progressText(item.sourceProgress)}</p><p>目标进度：{progressText(item.targetProgress)}</p><p>冲突：{item.conflictCodes.map(code=>CONFLICT_LABEL[code]||code).join("；")||"无"}</p></td>
      </tr>)}</tbody></table></div>
      {picker&&<section aria-label="选择本地视频"><label>搜索本地视频<input aria-label="搜索本地视频" disabled={busy} value={query} onChange={event=>setQuery(event.target.value)}/></label><button type="button" disabled={busy||videoLoading} onClick={()=>void searchVideos(0)}>搜索视频</button><button type="button" onClick={()=>{videoRequest.current++;setPicker("")}}>取消选择</button>{videoLoading&&<p role="status">正在搜索视频…</p>}{videoError&&<p role="alert">视频搜索失败：{videoError}</p>}<ul>{videos.map(video=><li key={video.id}><button type="button" aria-label={`选择 ${video.name} ${video.id}`} disabled={busy} onClick={()=>{if(pending.current)return;setBindings(current=>({...current,[picker]:video.id}));setApproved(false);setFresh(false);setStatus("");videoRequest.current++;setPicker("")}}>{video.name} · {video.id}</button></li>)}</ul>{videoTotal>50&&<div><button type="button" disabled={busy||videoLoading||videoOffset===0} onClick={()=>void searchVideos(Math.max(0,videoOffset-50))}>上一页视频</button><span>第 {videoOffset/50+1} 页</span><button type="button" disabled={busy||videoLoading||videoOffset+50>=videoTotal} onClick={()=>void searchVideos(videoOffset+50)}>下一页视频</button></div>}</section>}
      {!rows.length&&<p>{preview.series.length?"没有课程，可迁入空系列。":"没有可迁入系列。"}</p>}
      {rows.length>100&&<div className="learning-import-actions"><button type="button" aria-label="上一页课程" disabled={page===0||busy} onClick={()=>setPage(value=>value-1)}>上一页</button><span>第 {page+1} / {Math.ceil(rows.length/100)} 页 · {rows.length} 门课程</span><button type="button" aria-label="下一页课程" disabled={(page+1)*100>=rows.length||busy} onClick={()=>setPage(value=>value+1)}>下一页</button></div>}
      <label><input type="checkbox" checked={approved} disabled={busy||!fresh} onChange={event=>setApproved(event.target.checked)}/>我已逐项核对课程、媒体绑定、来源与目标进度及冲突，确认按本次预检决策迁入</label>
      <button type="button" disabled={busy||!fresh||!approved||!preview.series.length} onClick={()=>void queue()}>排队迁入课程</button>
    </>}
    {status&&<p role="status">{status}</p>}{error&&<p role="alert">{error}</p>}
  </section>;
}
