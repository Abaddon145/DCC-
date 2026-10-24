import { useCallback, useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { ListTodo, X } from "lucide-react";
import {ReadFailure} from "./LoadFeedback";
import { api } from "../lib/api";
import type { LibraryTask } from "../types";
import { ModelThumbnailTask } from "./ModelThumbnailTask";

const statusLabels:Record<string,string>={queued:"等待执行",running:"处理中",completed:"已完成",stopped:"已停止",interrupted:"上次中断",failed:"失败",paused:"已暂停",cancelled:"已取消"};
export function TaskCenter({notify}:{notify:(message:string,error?:boolean)=>void}) {
  const retryAfter=useRef(0);const [readFailed,setReadFailed]=useState(false);
  const [warning,setWarning]=useState<string|null>(null);const [pending,setPending]=useState<string|null>(null);const refreshLock=useRef(false);const mutationLock=useRef(false);
  const [resultPage,setResultPage]=useState(0);const [inspected,setInspected]=useState<LibraryTask|null>(null);
  const [tasks,setTasks]=useState<LibraryTask[]>([]);const [open,setOpen]=useState(false);const [selected,setSelected]=useState<string|null>(null);
  useEffect(() => { const show = () => setOpen(true); window.addEventListener("open-task-center", show); return () => window.removeEventListener("open-task-center", show); }, []);
  const refresh=useCallback(async()=>{if(refreshLock.current||Date.now()<retryAfter.current)return;refreshLock.current=true;try{setTasks(await api.listLibraryTasks());if(api.getTaskWarning)setWarning(await api.getTaskWarning());setReadFailed(false)}catch(error){retryAfter.current=Date.now()+30000;setReadFailed(true);notify(`读取任务失败：${String(error)}`,true)}finally{refreshLock.current=false}},[notify]);
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
    window.addEventListener("library-task-started",refresh);window.addEventListener("library-location-changed",refresh);
    return()=>{active=false;window.clearInterval(timer);disposes.forEach(dispose=>dispose());window.removeEventListener("library-task-started",refresh);window.removeEventListener("library-location-changed",refresh)};
  },[refresh,notify]);
  const mutate=async(task:LibraryTask,retry:boolean)=>{if(mutationLock.current)return;mutationLock.current=true;setPending(task.id);try{if(retry){if(task.payload.kind==="translation"||task.payload.kind==="fabImport"||task.payload.kind==="aiAnalyze"){if(!window.confirm("继续可能调用 Fab、在线翻译或 AI 服务并使用额度，是否继续？"))return;}await api.retryLibraryTask(task.id)}else await api.cancelLibraryTask(task.id);await refresh()}catch(error){notify(String(error),true)}finally{mutationLock.current=false;setPending(null)}};
  const pause=async(id:string)=>{if(mutationLock.current)return;mutationLock.current=true;setPending(id);try{await api.pauseDownloadTask(id);await refresh()}catch(error){notify(String(error),true)}finally{mutationLock.current=false;setPending(null)}};
  const running=tasks.filter(task=>["running","queued"].includes(task.status)).length;
  const thumbnail=tasks.find(task=>task.status==="running"&&task.payload.kind==="modelThumbnail");
  const selectedSummary=tasks.find(task=>task.id===selected);
  useEffect(()=>{if(!selected){setInspected(null);return}let active=true;void api.getLibraryTask(selected).then(task=>{if(active)setInspected(task)}).catch(error=>notify(String(error),true));return()=>{active=false}},[selected,selectedSummary?.updatedAt,notify]);
  const result=inspected?.result as {rows?:Array<Record<string,unknown>>;outputPath?:string;safetyBackupDirectory?:string}|null;
  return <>{thumbnail&&<ModelThumbnailTask key={thumbnail.id} id={thumbnail.id}/>}<button onClick={()=>setOpen(true)}><ListTodo size={15}/>任务中心{running?` · ${running}`:""}</button>
    {open&&<div className="modal-backdrop"><section className="task-center modal-card" role="dialog" aria-modal="true" aria-label="任务中心"><header><div><span className="eyebrow">WORKSPACE TASKS</span><h2>任务中心</h2></div><button className="icon-button" aria-label="关闭任务中心" onClick={()=>setOpen(false)}><X size={18}/></button></header><p>下载按设置并发执行，其他任务按顺序执行。关闭此面板不会停止任务；中断后需手动继续，不自动请求在线服务。</p>
      {warning&&<p role="alert">{warning}</p>}{readFailed&&<ReadFailure onRetry={()=>{retryAfter.current=0;void refresh()}}/>}<div className="task-list" aria-busy={!!pending}>{tasks.map(task=><article key={task.id} className={`task-row ${task.status}`}><button className="task-name" onClick={()=>{setSelected(task.id);setResultPage(0)}}><strong>{task.label}</strong><small>{task.payload.kind==="ueAssets"?"旧版 UE 任务 · 只读历史":task.currentName||task.phase}</small></button><span>{statusLabels[task.status]}</span>{task.total>0?<progress max={task.total} value={task.current}/>:task.status==="running"?<progress/>:null}<small>{task.total?`${task.current} / ${task.total}`:""}</small>{task.payload.kind==="download"&&["running","queued"].includes(task.status)&&<button disabled={pending===task.id} onClick={()=>void pause(task.id)}>暂停</button>}{task.payload.kind==="ueAssets"?<button disabled title="当前版本仅保留此任务的历史记录">只读历史</button>:["running","queued"].includes(task.status)?<button disabled={pending===task.id||(task.status==="running"&&!task.cancellable)} onClick={()=>void mutate(task,false)}>{task.cancellable||task.status==="queued"?"安全停止":"安全维护中"}</button>:task.payload.kind==="courseAnalysis"?<button disabled aria-describedby={`course-consent-${task.id}`}>需重新预览确认</button>:<button disabled={pending===task.id} onClick={()=>void mutate(task,true)}>{task.status==="completed"?"重新执行":"继续 / 重试失败项"}</button>}{task.payload.kind==="courseAnalysis"&&!["running","queued"].includes(task.status)&&<p id={`course-consent-${task.id}`}>旧批准不能重试；请重新预览材料与费用。已完成阶段会保留。</p>}{task.message&&<p role="alert">{task.message}</p>}</article>)}{!readFailed&&!tasks.length&&<p>暂无任务。导入、翻译和数据维护任务将在这里显示。</p>}</div>
      {inspected&&<section className="task-result"><h3>{inspected.label} · 结果</h3>{result?.outputPath&&<p>输出位置：{result.outputPath}</p>}{result?.safetyBackupDirectory&&<p>恢复前安全副本：{result.safetyBackupDirectory}</p>}<div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>条目</th><th>状态</th><th>说明</th></tr></thead><tbody>{result?.rows?.slice(resultPage*100,resultPage*100+100).map((row,index)=><tr key={index}><td>{String(row.name||row.path||row.assetId||index+1)}</td><td>{String(row.status||"")}</td><td>{Array.isArray(row.messages)?row.messages.join("；"):String(row.message||"")}</td></tr>)}</tbody></table></div>{Boolean(result?.rows?.length)&&<footer><button disabled={!resultPage} onClick={()=>setResultPage(value=>value-1)}>上一页</button><span>第 {resultPage+1} / {Math.ceil((result?.rows?.length||0)/100)} 页 · {result?.rows?.length} 项</span><button disabled={(resultPage+1)*100>=(result?.rows?.length||0)} onClick={()=>setResultPage(value=>value+1)}>下一页</button></footer>}{!result?.rows?.length&&!inspected.message&&<p>当前步骤尚未产生逐项结果。</p>}</section>}
      <footer><button className="secondary-button" onClick={()=>setOpen(false)}>收起，继续工作</button></footer>
    </section></div>}
  </>;
}
