import {invoke} from "@tauri-apps/api/core";
import type {Page} from "../../types";
import type {Course,Series,SeriesInput,ExternalVideoPreview,PlaybackSnapshot,PlaybackAction,NativeViewport,LearningSubtitle,TranscriptCue} from "./types";
import type {CourseAnalysisOptions,CourseAnalysisPreview,CourseAnalysisDetail,CourseAnalysisHistoryItem} from "./types";

// No unscoped commands and no frontend progress writer: playback owns the future lease.
export const learningApi={
  prepareNoteJump:(expectedLibraryId:string,reference:import("../../lib/learningNoteNodes").LearningReferenceData,rangeIndex:number)=>invoke<{courseId:string;mediaId:string;positionMs:number;endMs:number}>("prepare_learning_note_jump",{expectedLibraryId,reference,rangeIndex}),
  transcriptionComponents:(expectedLibraryId:string)=>invoke<import("./types").WhisperComponentStatus>("get_learning_transcription_components",{expectedLibraryId}),
  importTranscriptionModel:(expectedLibraryId:string,modelId:string,selectedPath:string)=>invoke<import("./types").WhisperComponentStatus>("import_learning_transcription_model",{expectedLibraryId,modelId,selectedPath}),
  installTranscriptionTools:(expectedLibraryId:string,selectedResourceRoot:string)=>invoke<import("./types").WhisperComponentStatus>("install_learning_transcription_tools",{expectedLibraryId,selectedResourceRoot}),
  listCourseAnalyses:(expectedLibraryId:string,courseId:string,offset=0,limit=50)=>invoke<Page<CourseAnalysisHistoryItem>>("list_course_analyses",{expectedLibraryId,courseId,offset,limit}),
  getCourseAnalysis:(expectedLibraryId:string,courseId:string,analysisId:string,stageKey:string|null=null,offset=0,limit=50)=>invoke<CourseAnalysisDetail>("get_course_analysis",{expectedLibraryId,courseId,analysisId,stageKey,offset,limit}),
  prepareCourseAnalysis:(expectedLibraryId:string,courseId:string,options:CourseAnalysisOptions)=>invoke<CourseAnalysisPreview>("prepare_course_analysis",{expectedLibraryId,courseId,options}),
  prepareCourseSummary:(expectedLibraryId:string,courseId:string,parentAnalysisId:string)=>invoke<CourseAnalysisPreview>("prepare_course_summary",{expectedLibraryId,courseId,parentAnalysisId}),
  previewExternalVideo:(expectedLibraryId:string,path:string)=>invoke<ExternalVideoPreview>("preview_external_video",{expectedLibraryId,path}),
  commitExternalVideo:(expectedLibraryId:string,previewId:string)=>invoke<string>("commit_external_video",{expectedLibraryId,previewId}),
  relocateExternalVideo:(expectedLibraryId:string,fileId:string,previewId:string,expectedSourceRevision:number)=>invoke<string>("relocate_external_video",{expectedLibraryId,fileId,previewId,expectedSourceRevision}),
  ensureCourse:(expectedLibraryId:string,mediaId:string)=>invoke<Course>("ensure_learning_course",{expectedLibraryId,mediaId}),
  getCourse:(expectedLibraryId:string,id:string)=>invoke<Course>("get_learning_course",{expectedLibraryId,id}),
  listCourses:(expectedLibraryId:string,offset=0,limit=80)=>invoke<Page<Course>>("list_learning_courses",{expectedLibraryId,offset,limit}),
  listSeries:(expectedLibraryId:string)=>invoke<Series[]>("list_learning_series",{expectedLibraryId}),
  upsertSeries:(expectedLibraryId:string,input:SeriesInput)=>invoke<Series>("upsert_learning_series",{expectedLibraryId,input}),
  setSeriesMembers:(expectedLibraryId:string,id:string,expectedRevision:number,courseIds:string[])=>invoke<Series>("set_learning_series_members",{expectedLibraryId,id,expectedRevision,courseIds}),
  deleteSeries:(expectedLibraryId:string,id:string,expectedRevision:number)=>invoke<void>("delete_learning_series",{expectedLibraryId,id,expectedRevision}),
  startPlayback:(expectedLibraryId:string,courseId:string,expectedFingerprint?:string)=>invoke<PlaybackSnapshot>("start_learning_playback",{expectedLibraryId,courseId,...(expectedFingerprint?{expectedFingerprint}:{})}),
  getPlayback:(expectedLibraryId:string,sessionId:string)=>invoke<PlaybackSnapshot>("get_learning_playback",{expectedLibraryId,sessionId}),
  controlPlayback:(expectedLibraryId:string,sessionId:string,action:PlaybackAction)=>invoke<PlaybackSnapshot>("control_learning_playback",{expectedLibraryId,sessionId,action}),
  setViewport:(expectedLibraryId:string,sessionId:string,viewport:NativeViewport)=>invoke<void>("set_learning_viewport",{expectedLibraryId,sessionId,viewport}),
  stopPlayback:(expectedLibraryId:string,sessionId:string)=>invoke<void>("stop_learning_playback",{expectedLibraryId,sessionId}),
  importSubtitle:(expectedLibraryId:string,courseId:string,selectedPath:string)=>invoke<LearningSubtitle>("import_learning_subtitle",{expectedLibraryId,courseId,selectedPath}),
  listSubtitles:(expectedLibraryId:string,courseId:string)=>invoke<LearningSubtitle[]>("list_learning_subtitles",{expectedLibraryId,courseId}),
  transcriptPage:(expectedLibraryId:string,courseId:string,subtitleId:string,offset=0,limit=50)=>invoke<Page<TranscriptCue>>("transcript_page",{expectedLibraryId,courseId,subtitleId,offset,limit}),
  loadSubtitle:(expectedLibraryId:string,sessionId:string,subtitleId:string)=>invoke<void>("load_learning_subtitle",{expectedLibraryId,sessionId,subtitleId}),
};
