import {useEffect,useState} from "react";
import type {LibraryTask,Page} from "../../types";
import {learningApi} from "./api";
import {learningTime} from "./LearningControls";
import {LearningVideoImport} from "./LearningVideoImport";
import type {Course} from "./types";
import type {UnassignedLearningVideo} from "./api";
import "./learningLibraryHome.css";

export function LearningLibraryHome({libraryId,onOpen,onOrganize}:{libraryId:string;onOpen:(courseId:string,mediaId:string)=>void;onOrganize:()=>void}) {
  const [page,setPage]=useState<Page<Course>|null>(null),[offset,setOffset]=useState(0),[revision,setRevision]=useState(0),[error,setError]=useState("");
  const [unassigned,setUnassigned]=useState<Page<UnassignedLearningVideo>|null>(null),[unassignedOffset,setUnassignedOffset]=useState(0),[unassignedError,setUnassignedError]=useState("");
  useEffect(()=>{let active=true;setPage(null);setError("");void learningApi.listCourses(libraryId,offset,50).then(value=>{if(active)setPage(value)}).catch(reason=>{if(active)setError(String(reason))});return()=>{active=false};},[libraryId,offset,revision]);
  useEffect(()=>{let active=true;setUnassigned(null);setUnassignedError("");void learningApi.listUnassignedVideos(libraryId,unassignedOffset,50).then(value=>{if(active)setUnassigned(value)}).catch(reason=>{if(active)setUnassignedError(String(reason))});return()=>{active=false};},[libraryId,unassignedOffset,revision]);
  useEffect(()=>{const refresh=(event:Event)=>{const task=(event as CustomEvent<LibraryTask>).detail;if(task?.libraryId===libraryId&&task.payload.kind==="learningMediaImport")setRevision(value=>value+1)};const membership=(event:Event)=>{if((event as CustomEvent<{libraryId:string}>).detail?.libraryId===libraryId)setRevision(value=>value+1)};window.addEventListener("library-task-completed",refresh);window.addEventListener("learning-memberships-changed",membership);return()=>{window.removeEventListener("library-task-completed",refresh);window.removeEventListener("learning-memberships-changed",membership)};},[libraryId]);
  return <div className="learning-home"><header className="learning-home-header"><div><span className="eyebrow">LEARNING LIBRARY</span><h1>学习库</h1><p>整理教程，回到上次学习的位置。</p></div><button type="button" className="secondary-button" onClick={onOrganize}>整理与迁入…</button></header>
    <LearningVideoImport libraryId={libraryId}/>
    <section aria-label="学习课程"><div className="learning-home-section-title"><h2>学习课程</h2>{page&&<span>{page.total} 门课程</span>}</div>
      {error&&<p role="alert">读取课程失败：{error}<button type="button" onClick={()=>setRevision(value=>value+1)}>重试</button></p>}
      {!page&&!error&&<p role="status">正在读取课程…</p>}
      {page&&page.items.length===0&&<p>暂无课程。导入本地教程，或整理已有视频归属。</p>}
      {page&&page.items.length>0&&<div className="learning-home-cards">{page.items.map(course=>{
        const percentage=course.durationMs&&course.durationMs>0?Math.min(100,Math.round(course.positionMs/course.durationMs*100)):null;
        return <article key={course.id} className="learning-home-card"><h3>{course.title}</h3><p>{course.completed?"已完成":`已学习 ${learningTime(course.positionMs)}`}{course.durationMs==null?" · 时长未知":` / ${learningTime(course.durationMs)}`}</p>{percentage!==null&&<progress value={percentage} max={100} aria-label={`${course.title}学习进度`}/>}<button type="button" className="secondary-button" aria-label={`继续学习 ${course.title}`} onClick={()=>onOpen(course.id,course.mediaId)}>继续学习</button></article>;
      })}</div>}
      {page&&page.total>50&&<nav aria-label="课程分页"><button type="button" disabled={offset===0} onClick={()=>setOffset(value=>Math.max(0,value-50))}>上一页</button><span>{offset+1}—{Math.min(offset+page.items.length,page.total)} / {page.total}</span><button type="button" disabled={offset+page.items.length>=page.total} onClick={()=>setOffset(value=>value+50)}>下一页</button></nav>}
    </section>
    {unassignedError&&<p role="alert">读取未建课视频失败：{unassignedError}<button type="button" onClick={()=>setRevision(value=>value+1)}>重试</button></p>}
    {unassigned&&unassigned.total>0&&<section aria-label="尚未建课的视频"><div className="learning-home-section-title"><h2>尚未建课的视频</h2><span>{unassigned.total} 个视频</span></div><p>这些视频已加入学习库，尚未创建课程；不会自动写入零进度。</p><ul>{unassigned.items.map(item=><li key={item.mediaId}><strong>{item.name}</strong><p>尚未建课 · 进度未知</p></li>)}</ul>{unassigned.total>50&&<nav aria-label="未建课视频分页"><button type="button" disabled={unassignedOffset===0} onClick={()=>setUnassignedOffset(value=>Math.max(0,value-50))}>上一页</button><span>{unassignedOffset+1}—{Math.min(unassignedOffset+unassigned.items.length,unassigned.total)} / {unassigned.total}</span><button type="button" disabled={unassignedOffset+unassigned.items.length>=unassigned.total} onClick={()=>setUnassignedOffset(value=>value+50)}>下一页</button></nav>}</section>}
  </div>;
}
