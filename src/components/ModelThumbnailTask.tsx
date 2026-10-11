import { useEffect, useRef, useState } from "react";
import { api, mediaLibraryBundleUrl } from "../lib/api";
import type { ModelThumbnailJob } from "../types";
import { ModelViewer } from "./ModelViewer";

export function ModelThumbnailTask({id}:{id:string}) {
  const [job,setJob]=useState<ModelThumbnailJob|null>(null);const sent=useRef(false);
  useEffect(()=>{let active=true;sent.current=false;void api.getModelThumbnailTask(id).then(value=>{if(active)setJob(value)}).catch(error=>{if(active)void api.finishModelThumbnailTask(id,null,String(error)).catch(()=>undefined)});return()=>{active=false;sent.current=true}},[id]);
  const fail=(error:string)=>{if(sent.current)return;sent.current=true;void api.finishModelThumbnailTask(id,null,error).catch(()=>undefined)};
  return job?<div className="model-thumbnail-worker" aria-hidden="true"><ModelViewer sourceUrl={mediaLibraryBundleUrl(job.id,job.logicalPath)} sourceName={job.originalName} thumbnailMode onError={fail} onReady={capture=>{if(sent.current)return;sent.current=true;void capture().then(bytes=>api.finishModelThumbnailTask(id,Array.from(bytes),null)).catch(error=>{void api.finishModelThumbnailTask(id,null,String(error)).catch(()=>undefined)})}}/></div>:null;
}
