import {invoke} from "@tauri-apps/api/core";
import type {Page} from "../../types";
import type {Course,Series,SeriesInput,ExternalVideoPreview,PlaybackSnapshot,PlaybackAction,NativeViewport} from "./types";

// No unscoped commands and no frontend progress writer: playback owns the future lease.
export const learningApi={
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
  startPlayback:(expectedLibraryId:string,courseId:string)=>invoke<PlaybackSnapshot>("start_learning_playback",{expectedLibraryId,courseId}),
  getPlayback:(expectedLibraryId:string,sessionId:string)=>invoke<PlaybackSnapshot>("get_learning_playback",{expectedLibraryId,sessionId}),
  controlPlayback:(expectedLibraryId:string,sessionId:string,action:PlaybackAction)=>invoke<PlaybackSnapshot>("control_learning_playback",{expectedLibraryId,sessionId,action}),
  setViewport:(expectedLibraryId:string,sessionId:string,viewport:NativeViewport)=>invoke<void>("set_learning_viewport",{expectedLibraryId,sessionId,viewport}),
  stopPlayback:(expectedLibraryId:string,sessionId:string)=>invoke<void>("stop_learning_playback",{expectedLibraryId,sessionId}),
};
