import { useCallback, useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { ListTodo, X } from "lucide-react";
import {LoadFeedback,ReadFailure} from "./LoadFeedback";
import { api } from "../lib/api";
import type { LibraryTask } from "../types";
import { ModelThumbnailTask } from "./ModelThumbnailTask";

const statusLabels:Record<string,string>={queued:"等待执行",running:"处理中",completed:"已完成",stopped:"已停止",interrupted:"上次中断",failed:"失败",paused:"已暂停",cancelled:"已取消"};
function validKeptCourseItem(item:unknown):boolean{
  if(!item||typeof item!=="object")return false;
  const value=item as Record<string,unknown>;
  return typeof value.entryId==="string"&&typeof value.outcome==="string"&&["kept_modified","kept_reused","kept_conflict","kept_missing"].includes(value.outcome)&&typeof value.targetKind==="string"&&["course","series"].includes(value.targetKind)&&(value.targetId===null||typeof value.targetId==="string");
}
function courseReceiptMessage(task:LibraryTask):string|null{
  if(task.payload.kind!=="learningImport")return null;
  const request=task.payload.request;
  if(request.action!=="commitCourse"&&request.action!=="cancelCourse"&&request.action!=="revertCourse")return null;
  if(task.status==="queued"||task.status==="running")return "课程批次任务尚未完成。";
  if(task.status!=="completed")return "课程批次任务未显示完成，请检查批次状态。";
  if(request.action==="commitCourse"){
    const result=task.result as {batchId?:unknown;status?:unknown;receipt?:Record<string,unknown>}|null;
    const receipt=result?.receipt;
    if(!result||typeof result.batchId!=="string"||!result.batchId||(result.status!=="committed"&&result.status!=="cancelled")||!receipt||!["coursesCommitted","courseConflicts","seriesCommitted","seriesConflicts"].every(key=>Number.isSafeInteger(receipt[key])&&(receipt[key] as number)>=0))return "课程提交记录不完整，请检查批次。";
    if(result.status==="cancelled")return `课程迁入已取消，请查看已保存的批次记录：${result.batchId}`;
    return `课程已提交 ${receipt.coursesCommitted}，冲突 ${receipt.courseConflicts}；系列已提交 ${receipt.seriesCommitted}，冲突 ${receipt.seriesConflicts}。批次：${result.batchId}`;
  }
  const value=task.result as {batchId?:unknown;status?:unknown;summary?:{revertedCount?:unknown;keptCount?:unknown;archivesKept?:unknown};report?:{batchId?:unknown;reverted?:unknown[];kept?:unknown[];archivesKept?:unknown}}|null;
  if(request.action==="revertCourse"){
    if(value?.summary!==undefined){
      const summary=value.summary;
      const valid=value.batchId===request.batchId&&value.status==="reverted"&&summary?.archivesKept===true&&Number.isSafeInteger(summary.revertedCount)&&(summary.revertedCount as number)>=0&&Number.isSafeInteger(summary.keptCount)&&(summary.keptCount as number)>=0;
      return valid?`已撤回 ${summary.revertedCount} 项，保留 ${summary.keptCount} 项；原媒体已保留。`:"课程撤回记录不完整，请检查批次。";
    }
    const report=value?.report;
    const valid=value?.batchId===request.batchId&&value.status==="reverted"&&report?.batchId===request.batchId&&report.archivesKept===true&&Array.isArray(report.reverted)&&report.reverted.every(item=>typeof item==="string")&&Array.isArray(report.kept)&&report.kept.every(validKeptCourseItem);
    return valid?`已撤回 ${report.reverted!.length} 项，保留 ${report.kept!.length} 项；原媒体已保留。`:"课程撤回记录不完整，请检查批次。";
  }
  if(value?.batchId===request.batchId&&value.status==="cancelled")return "课程批次已取消。";
  if(value?.batchId===request.batchId&&value.status==="committed")return "课程批次已提交，未取消。";
  return "无法确认课程批次状态。";
}
export function TaskCenter({notify}:{notify:(message:string,error?:boolean)=>void}) {
  const retryAfter=useRef(0);const [readFailed,setReadFailed]=useState(false);
  const [warning,setWarning]=useState<string|null>(null);const [pending,setPending]=useState<string|null>(null);const libraryGeneration=useRef(0);const refreshLock=useRef<number|null>(null);const mutationLock=useRef(false);
  const [resultPage,setResultPage]=useState(0);const [inspected,setInspected]=useState<LibraryTask|null>(null);const [detailRefresh,setDetailRefresh]=useState(0);const [detailLoading,setDetailLoading]=useState(false);const [detailFailed,setDetailFailed]=useState(false);
  const [tasks,setTasks]=useState<LibraryTask[]>([]);const [open,setOpen]=useState(false);const [selected,setSelected]=useState<string|null>(null);
  useEffect(() => { const show = () => setOpen(true); window.addEventListener("open-task-center", show); return () => window.removeEventListener("open-task-center", show); }, []);
  const refresh=useCallback(async()=>{const generation=libraryGeneration.current;if(refreshLock.current===generation||Date.now()<retryAfter.current)return;refreshLock.current=generation;try{const next=await api.listLibraryTasks();if(generation!==libraryGeneration.current)return;setTasks(next);if(api.getTaskWarning){const nextWarning=await api.getTaskWarning();if(generation!==libraryGeneration.current)return;setWarning(nextWarning)}setReadFailed(false)}catch(error){if(generation!==libraryGeneration.current)return;retryAfter.current=Date.now()+30000;setReadFailed(true);notify(`读取任务失败：${String(error)}`,true)}finally{if(refreshLock.current===generation)refreshLock.current=null}},[notify]);
  useEffect(()=>{
    let active=true;const disposes:Array<()=>void>=[];
    void refresh();const timer=window.setInterval(()=>void refresh(),1500);
    void api.queueModelThumbnailTasks().catch(error=>notify(`读取模型缩略图任务失败：${String(error)}`,true));
    const add=(dispose:()=>void)=>{if(active)disposes.push(dispose);else dispose()};
    void listen<string>("learning-close-failed",event=>{if(active)notify(event.payload,true)}).then(add);
    void listen<LibraryTask>("library-task-updated",event=>{void refresh();if(!["queued","running"].includes(event.payload.status)){window.dispatchEvent(new CustomEvent("library-task-completed",{detail:event.payload}));if(event.payload.payload.kind==="mediaImport"||event.payload.payload.kind==="restore")void api.queueModelThumbnailTasks().catch(()=>undefined);}}).then(add);
    void listen("library-task-exit-requested",()=>{setOpen(true);if(window.confirm("仍有任务正在执行。停止可取消的任务，并在数据维护安全完成后退出？已完成结果会保留。"))void (async()=>{
      try{const current=await api.listLibraryTasks();for(const task of current.filter(value=>value.status==="queued"||(value.status==="running"&&value.cancellable)))await api.cancelLibraryTask(task.id);
        while((await api.listLibraryTasks()).some(task=>["queued","running"].includes(task.status)))await new Promise(resolve=>window.setTimeout(resolve,500));
        await api.exitWhenTasksIdle();
      }catch(error){notify(String(error),true)}
    })();}).then(add);
    const libraryChanged=()=>{libraryGeneration.current++;retryAfter.current=0;setTasks([]);setSelected(null);setInspected(null);setDetailFailed(false);setDetailLoading(false);setWarning(null);setReadFailed(false);setResultPage(0);void refresh()};
    window.addEventListener("library-task-started",refresh);window.addEventListener("library-location-changed",libraryChanged);
    return()=>{active=false;window.clearInterval(timer);disposes.forEach(dispose=>dispose());window.removeEventListener("library-task-started",refresh);window.removeEventListener("library-location-changed",libraryChanged)};
  },[refresh,notify]);
  const mutate=async(task:LibraryTask,retry:boolean)=>{if(mutationLock.current)return;mutationLock.current=true;setPending(task.id);try{if(retry){if(task.payload.kind==="translation"||task.payload.kind==="fabImport"||task.payload.kind==="aiAnalyze"){if(!window.confirm("继续可能调用 Fab、在线翻译或 AI 服务并使用额度，是否继续？"))return;}await api.retryLibraryTask(task.id)}else await api.cancelLibraryTask(task.id);await refresh()}catch(error){notify(String(error),true)}finally{mutationLock.current=false;setPending(null)}};
  const pause=async(id:string)=>{if(mutationLock.current)return;mutationLock.current=true;setPending(id);try{await api.pauseDownloadTask(id);await refresh()}catch(error){notify(String(error),true)}finally{mutationLock.current=false;setPending(null)}};
  const running=tasks.filter(task=>["running","queued"].includes(task.status)).length;
  const thumbnail=tasks.find(task=>task.status==="running"&&task.payload.kind==="modelThumbnail");
  const selectedSummary=tasks.find(task=>task.id===selected);
  useEffect(()=>{setInspected(null);setDetailFailed(false);if(!selected){setDetailLoading(false);return}let active=true;setDetailLoading(true);void api.getLibraryTask(selected).then(task=>{if(active)setInspected(task)}).catch(error=>{if(active){setDetailFailed(true);notify(String(error),true)}}).finally(()=>{if(active)setDetailLoading(false)});return()=>{active=false}},[selected,selectedSummary?.updatedAt,selectedSummary?.status,detailRefresh,notify]);
  const result=inspected?.result as {rows?:unknown;outputPath?:unknown;safetyBackupDirectory?:unknown}|null;
  const rows=Array.isArray(result?.rows)&&result.rows.every(row=>row!==null&&typeof row==="object"&&!Array.isArray(row))?result.rows as Array<Record<string,unknown>>:null;
  const rowCount=rows?.length;
  useEffect(()=>{if(rowCount!==undefined)setResultPage(page=>Math.min(page,Math.max(0,Math.ceil(rowCount/100)-1)))},[rowCount]);
  const rowsDamaged=result?.rows!=null&&!rows;
  const courseReceipt=inspected?courseReceiptMessage(inspected):null;
  return <>{thumbnail&&<ModelThumbnailTask key={thumbnail.id} id={thumbnail.id}/>}<button onClick={()=>setOpen(true)}><ListTodo size={15}/>任务中心{running?` · ${running}`:""}</button>
    {open&&<div className="modal-backdrop"><section className="task-center modal-card" role="dialog" aria-modal="true" aria-label="任务中心"><header><div><span className="eyebrow">WORKSPACE TASKS</span><h2>任务中心</h2></div><button className="icon-button" aria-label="关闭任务中心" onClick={()=>setOpen(false)}><X size={18}/></button></header><p>下载按设置并发执行，其他任务按顺序执行。关闭此面板不会停止任务；中断后需手动继续，不自动请求在线服务。</p>
      {warning&&<p role="alert">{warning}</p>}{readFailed&&<ReadFailure onRetry={()=>{retryAfter.current=0;void refresh()}}/>}<div className="task-list" aria-busy={!!pending}>{tasks.map(task=><article key={task.id} className={`task-row ${task.status}`}><button className="task-name" onClick={()=>{if(task.id!==selected)setInspected(null);else setDetailRefresh(value=>value+1);setSelected(task.id);setResultPage(0)}}><strong>{task.label}</strong><small>{task.payload.kind==="ueAssets"?"旧版 UE 任务 · 只读历史":task.currentName||task.phase}</small></button><span>{statusLabels[task.status]}</span>{task.total>0?<progress max={task.total} value={task.current}/>:task.status==="running"?<progress/>:null}<small>{task.total?`${task.current} / ${task.total}`:""}</small>{task.payload.kind==="download"&&["running","queued"].includes(task.status)&&<button disabled={pending===task.id} onClick={()=>void pause(task.id)}>暂停</button>}{task.payload.kind==="ueAssets"?<button disabled title="当前版本仅保留此任务的历史记录">只读历史</button>:["running","queued"].includes(task.status)?<button disabled={pending===task.id||(task.status==="running"&&!task.cancellable)} onClick={()=>void mutate(task,false)}>{task.cancellable||task.status==="queued"?"安全停止":"安全维护中"}</button>:task.payload.kind==="courseAnalysis"?<button disabled aria-describedby={`course-consent-${task.id}`}>需重新预览确认</button>:task.payload.kind==="learningImport"&&task.payload.request.action==="commitCourse"?<button disabled>返回课程来源重新预检</button>:task.payload.kind==="learningImport"&&["revert","revertCourse"].includes(task.payload.request.action)?<button disabled>返回批次重新确认</button>:<button disabled={pending===task.id} onClick={()=>void mutate(task,true)}>{task.status==="completed"?"重新执行":"继续 / 重试失败项"}</button>}{task.payload.kind==="courseAnalysis"&&!["running","queued"].includes(task.status)&&<p id={`course-consent-${task.id}`}>旧批准不能重试；请重新预览材料与费用。已完成阶段会保留。</p>}{task.message&&<p role="alert">{task.message}</p>}</article>)}{!readFailed&&!tasks.length&&<p>暂无任务。导入、翻译和数据维护任务将在这里显示。</p>}</div>
      {selected&&detailLoading&&<LoadFeedback label="正在读取任务结果…"/>}{selected&&detailFailed&&<div role="alert">任务结果读取失败。<button onClick={()=>setDetailRefresh(value=>value+1)}>重新读取任务结果</button></div>}
      {inspected&&<section className="task-result"><h3>{inspected.label} · 结果</h3>{typeof result?.outputPath==="string"&&result.outputPath&&<p>输出位置：{result.outputPath}</p>}{typeof result?.safetyBackupDirectory==="string"&&result.safetyBackupDirectory&&<p>恢复前安全副本：{result.safetyBackupDirectory}</p>}{courseReceipt?<p>{courseReceipt}</p>:rowsDamaged?<p role="alert">任务逐项结果损坏，请检查任务记录。</p>:<div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>条目</th><th>状态</th><th>说明</th></tr></thead><tbody>{rows?.slice(resultPage*100,resultPage*100+100).map((row,index)=><tr key={index}><td>{String(row.name||row.path||row.assetId||index+1)}</td><td>{String(row.status||"")}</td><td>{Array.isArray(row.messages)?row.messages.join("；"):String(row.message||"")}</td></tr>)}</tbody></table></div>}{Boolean(rows?.length)&&<footer><button disabled={!resultPage} onClick={()=>setResultPage(value=>value-1)}>上一页</button><span>第 {resultPage+1} / {Math.ceil((rows?.length||0)/100)} 页 · {rows?.length} 项</span><button disabled={(resultPage+1)*100>=(rows?.length||0)} onClick={()=>setResultPage(value=>value+1)}>下一页</button></footer>}{!courseReceipt&&!rowsDamaged&&!rows?.length&&!inspected.message&&<p>当前步骤尚未产生逐项结果。</p>}</section>}
      <footer><button className="secondary-button" onClick={()=>setOpen(false)}>收起，继续工作</button></footer>
    </section></div>}
  </>;
}
