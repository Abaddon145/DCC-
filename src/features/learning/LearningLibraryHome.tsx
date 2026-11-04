import {useEffect,useRef,useState} from "react";
import type {LibraryTask,Page} from "../../types";
import {learningApi} from "./api";
import {learningTime} from "./LearningControls";
import {LearningVideoImport} from "./LearningVideoImport";
import {CourseAnalysisPanel} from "./CourseAnalysisPanel";
import {BookOpen,Play,ScanSearch} from "lucide-react";
import type {Course,Series} from "./types";
import type {UnassignedLearningVideo} from "./api";
import "./learningLibraryHome.css";

export function LearningLibraryHome({libraryId,onOpen,onOrganize}:{libraryId:string;onOpen:(courseId:string,mediaId:string)=>void;onOrganize:()=>void}) {
  const [page,setPage]=useState<Page<Course>|null>(null),[offset,setOffset]=useState(0),[revision,setRevision]=useState(0),[error,setError]=useState("");
  const [unassigned,setUnassigned]=useState<Page<UnassignedLearningVideo>|null>(null),[unassignedOffset,setUnassignedOffset]=useState(0),[unassignedError,setUnassignedError]=useState("");
  const [analysisCourse,setAnalysisCourse]=useState<Course|null>(null);
  const [query,setQuery]=useState("");
  const [series,setSeries]=useState<Series[]|null>(null),[seriesId,setSeriesId]=useState(""),[seriesError,setSeriesError]=useState("");
  const selectedSeries=series?.find(item=>item.id===seriesId);
  const analysisRegion=useRef<HTMLElement|null>(null);
  useEffect(()=>{if(analysisCourse)analysisRegion.current?.scrollIntoView?.({block:"start",behavior:"smooth"});},[analysisCourse]);
  useEffect(()=>{let active=true;setSeriesError("");void learningApi.listSeries(libraryId).then(rows=>{if(active){setSeries(rows);setSeriesId(old=>rows.some(row=>row.id===old)?old:"");}}).catch(reason=>{if(active)setSeriesError(String(reason))});return()=>{active=false};},[libraryId,revision]);
  useEffect(()=>{let active=true;setPage(null);setError("");if(seriesId&&!selectedSeries)return()=>{active=false};const load=seriesId||query.trim()?learningApi.searchCourses(libraryId,query,seriesId||null,offset,50):learningApi.listCourses(libraryId,offset,50);void load.then(value=>{if(active)setPage(value)}).catch(reason=>{if(active)setError(String(reason))});return()=>{active=false};},[libraryId,offset,revision,seriesId,selectedSeries,query]);
  useEffect(()=>{let active=true;setUnassigned(null);setUnassignedError("");void learningApi.listUnassignedVideos(libraryId,unassignedOffset,50).then(value=>{if(active)setUnassigned(value)}).catch(reason=>{if(active)setUnassignedError(String(reason))});return()=>{active=false};},[libraryId,unassignedOffset,revision]);
  useEffect(()=>{const refresh=(event:Event)=>{const task=(event as CustomEvent<LibraryTask>).detail;if(task?.libraryId===libraryId&&task.payload.kind==="learningMediaImport")setRevision(value=>value+1)};const membership=(event:Event)=>{if((event as CustomEvent<{libraryId:string}>).detail?.libraryId===libraryId)setRevision(value=>value+1)};window.addEventListener("library-task-completed",refresh);window.addEventListener("learning-memberships-changed",membership);return()=>{window.removeEventListener("library-task-completed",refresh);window.removeEventListener("learning-memberships-changed",membership)};},[libraryId]);
  return <div className="learning-home"><header className="learning-home-header"><div><span className="eyebrow">LEARNING LIBRARY</span><h1>学习库</h1><p>整理教程，回到上次学习的位置。</p></div><button type="button" className="secondary-button" onClick={onOrganize}>整理与迁入…</button></header>
    <LearningVideoImport libraryId={libraryId}/>
    <section aria-label="课程系列导航"><div className="learning-home-section-title"><h2>课程系列</h2>{series&&<span>{series.length} 个系列</span>}</div>{seriesError&&<p role="alert">读取系列失败：{seriesError}<button type="button" onClick={()=>setRevision(value=>value+1)}>重试</button></p>}{series&&series.length===0&&<p>暂无系列，可在课程学习页面创建。</p>}{series&&series.length>0&&<nav className="learning-series-nav" aria-label="选择课程系列"><button type="button" className="secondary-button" aria-pressed={!seriesId} onClick={()=>{setSeriesId("");setOffset(0);}}>全部课程</button>{series.map(item=><button type="button" className="secondary-button" key={item.id} aria-pressed={seriesId===item.id} onClick={()=>{setSeriesId(item.id);setOffset(0);}}>{item.name} · {item.orderedCourseIds.length} 课</button>)}</nav>}</section>
    {analysisCourse&&<section ref={analysisRegion} aria-label="快捷视频分析"><button type="button" className="secondary-button" onClick={()=>setAnalysisCourse(null)}>关闭分析</button><CourseAnalysisPanel key={analysisCourse.id} libraryId={libraryId} courseId={analysisCourse.id} durationMs={analysisCourse.durationMs} canSeek={false} autoPrepare onSeek={()=>{}}/></section>}
    <section aria-label="学习课程"><div className="learning-home-section-title"><h2>{selectedSeries?selectedSeries.name:"学习课程"}</h2>{page&&<span>{page.total} 门课程</span>}</div><label className="learning-course-search"><span>搜索课程</span><input type="search" aria-label="搜索学习课程" maxLength={200} placeholder="搜索课程名称" value={query} onChange={event=>{setQuery(event.target.value);setOffset(0);}}/></label>
      {error&&<p role="alert">读取课程失败：{error}<button type="button" onClick={()=>setRevision(value=>value+1)}>重试</button></p>}
      {!page&&!error&&<p role="status">正在读取课程…</p>}
      {page&&page.items.length===0&&<p>{query.trim()?"没有找到匹配的课程。":selectedSeries?"此系列暂无课程。":"暂无课程。导入本地教程，或整理已有视频归属。"}</p>}
      {page&&page.items.length>0&&<div className="learning-home-cards">{page.items.map(course=>{
        const percentage=course.durationMs&&course.durationMs>0?Math.min(100,Math.round(course.positionMs/course.durationMs*100)):null;
        return <article key={course.id} className="learning-home-card"><div className="learning-card-heading"><span className="learning-card-icon"><BookOpen size={18} aria-hidden="true"/></span><span>{course.completed?"已完成":"学习中"}</span></div><h3>{course.title}</h3><p>{course.completed?"已完成":`已学习 ${learningTime(course.positionMs)}`}{course.durationMs==null?" · 时长未知":` / ${learningTime(course.durationMs)}`}</p>{percentage!==null&&<><progress value={percentage} max={100} aria-label={`${course.title}学习进度`}/><small className="learning-card-progress">{percentage}% 已完成</small></>}<div className="learning-card-actions"><button type="button" className="secondary-button" aria-label={`继续学习 ${course.title}`} onClick={()=>onOpen(course.id,course.mediaId)}><Play size={14} aria-hidden="true"/>继续学习</button><button type="button" className="secondary-button" aria-label={`分析视频 ${course.title}`} onClick={()=>setAnalysisCourse(course)}><ScanSearch size={14} aria-hidden="true"/>分析视频</button></div></article>;
      })}</div>}
      {page&&page.total>50&&<nav aria-label="课程分页"><button type="button" disabled={offset===0} onClick={()=>setOffset(value=>Math.max(0,value-50))}>上一页</button><span>{offset+1}—{Math.min(offset+page.items.length,page.total)} / {page.total}</span><button type="button" disabled={offset+page.items.length>=page.total} onClick={()=>setOffset(value=>value+50)}>下一页</button></nav>}
    </section>
    {unassignedError&&<p role="alert">读取未建课视频失败：{unassignedError}<button type="button" onClick={()=>setRevision(value=>value+1)}>重试</button></p>}
    {unassigned&&unassigned.total>0&&<section aria-label="尚未建课的视频"><div className="learning-home-section-title"><h2>尚未建课的视频</h2><span>{unassigned.total} 个视频</span></div><p>这些视频已加入学习库，尚未创建课程；不会自动写入零进度。</p><ul>{unassigned.items.map(item=><li key={item.mediaId}><strong>{item.name}</strong><p>尚未建课 · 进度未知</p></li>)}</ul>{unassigned.total>50&&<nav aria-label="未建课视频分页"><button type="button" disabled={unassignedOffset===0} onClick={()=>setUnassignedOffset(value=>Math.max(0,value-50))}>上一页</button><span>{unassignedOffset+1}—{Math.min(unassignedOffset+unassigned.items.length,unassigned.total)} / {unassigned.total}</span><button type="button" disabled={unassignedOffset+unassigned.items.length>=unassigned.total} onClick={()=>setUnassignedOffset(value=>value+50)}>下一页</button></nav>}</section>}
  </div>;
}
