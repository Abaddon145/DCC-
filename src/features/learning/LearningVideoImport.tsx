import {useRef,useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";
import {ImportTaskMap,importTask} from "../../components/ImportTaskMap";
import type {Series} from "./types";

export function LearningVideoImport({libraryId,series=[]}:{libraryId:string;series?:Series[]}) {
  const [busy,setBusy]=useState(false),[status,setStatus]=useState(""),[error,setError]=useState("");
  const [seriesId,setSeriesId]=useState("");
  const running=useRef(false);
  const choose=async(folder=false)=>{
    if(running.current)return;running.current=true;setBusy(true);setStatus("");setError("");
    try{
      const selected=folder?await open({title:"选择教程文件夹",directory:true,multiple:false}):await open({title:"选择本地教程视频",multiple:true,filters:[{name:"视频",extensions:["mp4","webm","mov","mkv"]}]});
      if(!selected)return;
      const paths=Array.isArray(selected)?selected:[selected];
      if(paths.length>500)throw new Error("一次最多选择 500 个视频");
      const task=await api.startLibraryTask({kind:"learningMediaImport",request:{kind:"video",paths,folderId:null,tags:[],favorite:false,...(seriesId?{seriesId}:{})}},libraryId);
      window.dispatchEvent(new Event("library-task-started"));
      setStatus(`已提交导入任务 ${task.id}；请在任务中心查看每个文件的结果。`);
    }catch(reason){setError(`未能提交导入：${String(reason)}`);}
    finally{running.current=false;setBusy(false);}
  };
  return <section aria-label="导入学习视频"><h3>导入本地教程</h3><ImportTaskMap current="learningVideo"/><p>选择的视频会作为新课程加入学习库，原文件不会被删除；已有素材需先确认归属。文件夹会包含子文件夹中的视频，一次最多 500 个；链接和其他素材库目录会跳过。</p><label>导入到系列<select aria-label="导入到系列" value={seriesId} disabled={busy} onChange={event=>setSeriesId(event.target.value)}><option value="">暂不加入系列</option>{series.map(item=><option key={item.id} value={item.id}>{item.name}</option>)}</select></label><p>要将多个视频组成系列，请先新建系列，再在这里选择。</p><button type="button" className="primary-button" disabled={busy} title={importTask("learningVideo").summary} onClick={()=>void choose()}>导入本地教程…</button> <button type="button" className="secondary-button" disabled={busy} title={importTask("learningVideo").summary} onClick={()=>void choose(true)}>导入教程文件夹…</button>{status&&<p role="status">{status}</p>}{error&&<p role="alert">{error}</p>}</section>;
}
