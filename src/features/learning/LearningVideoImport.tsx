import {useRef,useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {api} from "../../lib/api";

export function LearningVideoImport({libraryId}:{libraryId:string}) {
  const [busy,setBusy]=useState(false),[status,setStatus]=useState(""),[error,setError]=useState("");
  const running=useRef(false);
  const choose=async()=>{
    if(running.current)return;running.current=true;setBusy(true);setStatus("");setError("");
    try{
      const selected=await open({title:"选择本地教程视频",multiple:true,filters:[{name:"视频",extensions:["mp4","webm","mov","mkv"]}]});
      if(!selected)return;
      const paths=Array.isArray(selected)?selected:[selected];
      if(paths.length>500)throw new Error("一次最多选择 500 个视频");
      const task=await api.startLibraryTask({kind:"learningMediaImport",request:{kind:"video",paths,folderId:null,tags:[],favorite:false}},libraryId);
      window.dispatchEvent(new Event("library-task-started"));
      setStatus(`已提交导入任务 ${task.id}；请在任务中心查看每个文件的结果。`);
    }catch(reason){setError(`未能提交导入：${String(reason)}`);}
    finally{running.current=false;setBusy(false);}
  };
  return <section aria-label="导入学习视频"><h3>导入本地教程</h3><p>选择的视频会作为新课程加入学习库，原文件不会被删除；已有素材需先确认归属。</p><button type="button" disabled={busy} onClick={()=>void choose()}>导入本地教程…</button>{status&&<p role="status">{status}</p>}{error&&<p role="alert">{error}</p>}</section>;
}
