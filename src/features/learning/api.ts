import {invoke} from "@tauri-apps/api/core";
import type {Page} from "../../types";
import type {Course,Series,SeriesInput,ExternalVideoPreview,PlaybackSnapshot,PlaybackAction,NativeViewport,LearningSubtitle,TranscriptCue} from "./types";
import type {CourseAnalysisOptions,CourseAnalysisPreview,CourseAnalysisDetail,CourseAnalysisHistoryItem,CourseAnalysisTemplate} from "./types";
import type {NotePreviewRequest,StoredNotePreview,NoteCommitReceipt} from "./types";
import type {ExcerptTaskRequest,ExcerptDestination,StoredExcerpt,ExcerptOperation,ExcerptOperationSummary,ExcerptTargets} from "./types";
import type {LibraryTask} from "../../types";

export type MembershipPreview={mediaId:string;name:string;materials:boolean;learning:boolean;hasCourse:boolean;projectCount:number;assetCount:number;snapshot:string};
export type MembershipChange={mediaId:string;expectedMaterials:boolean;expectedLearning:boolean;expectedSnapshot:string;materials:boolean;learning:boolean};
export type UnassignedLearningVideo={mediaId:string;name:string};

// Structural types do not strip snapshot metadata from runtime objects.
function writableExcerptDestination(destination:ExcerptDestination):ExcerptDestination {
  switch(destination.kind){
    case "new":return {kind:"new",title:destination.title,notebookId:destination.notebookId};
    case "append":return {kind:"append",noteId:destination.noteId,expectedRevision:destination.expectedRevision,insertIndex:destination.insertIndex};
    case "project":return {kind:"project",projectId:destination.projectId,title:destination.title,notebookId:destination.notebookId};
    case "board":return {kind:"board",boardId:destination.boardId,x:destination.x,y:destination.y};
  }
}
function writableExcerptTask(task:ExcerptTaskRequest):ExcerptTaskRequest {
  if(task.action==="prepare"){
    const {subtitleId,cueIds,ranges,comment,frameTimeMs}=task.selection;
    return {action:"prepare",courseId:task.courseId,selection:{subtitleId,cueIds:[...cueIds],ranges:ranges.map(({startMs,endMs})=>({startMs,endMs})),comment,frameTimeMs}};
  }
  const {operationId,excerptId,includeFrame,destination}=task.request;
  return {action:"commit",request:{operationId,excerptId,includeFrame,destination:writableExcerptDestination(destination)}};
}

// No unscoped commands and no frontend progress writer: playback owns the future lease.

export type ImportSourceBinding={
  profileId:string;epoch:string;vault:string;root:string;libraryId:string;boundary:string;
};
export type ImportNotePreview={
  profileId:string;epoch:string;vault:string;relativePath:string;sourceSha256:string;selectionId:string;targetToken:string;
  conflict:string;targetId:string|null;title:string;pendingConversion:boolean;warnings:string[];
  canPrepare:boolean;willOverwrite:boolean;message:string;
};
export type ImportBatchView={
  id:string;status:string;requestSha256:string;sourceProfileId:string;sourceEpoch:string;
  currentSourceEpoch:string;createdAt:string;finishedAt:string|null;receipt:unknown|null;revertReport:unknown|null;
};
export type ImportItemView={
  id:string;batchId:string;status:string;conflictCode:string|null;targetId:string|null;
  sourceSha256:string;stageRelative:string|null;relativePath?:string;
};
export type CourseImportBatchSummary={id:string;status:string;createdAt:string;staged:boolean;committed:number;conflicts:number;reverted:boolean;revertDamaged:boolean;currentSourceEpoch:string|null};
export type CourseBatchCursor={createdAt:string;id:string};
export type CourseImportEntry={id:string;entryKind:"course"|"series";sourceId:string;status:"committed"|"conflict";conflictCode:string|null;targetId:string|null};
export type CourseRevertReport={batchId:string;reverted:string[];kept:{entryId:string;outcome:string;targetKind:"course"|"series";targetId:string|null}[];archivesKept:true};
export type CourseImportEntryPage={items:CourseImportEntry[];hasMore:boolean;revertReport:CourseRevertReport|null;revertDamaged:boolean};
export type CourseSourceBinding={selectionId:string;libraryId:string;profileId:string;epoch:string;fileName:string};
export type CourseImportBinding={sourceItemId:string;mediaId:string};
export type CourseImportProgress={positionMs:number;durationMs:number|null;rate:number;completed:boolean};
export type CourseSqlitePreview={previewId:string;selectionId:string;sourceFingerprint:string;targetFingerprint:string;expiresAt:number;series:{id:string;title:string;conflictCodes?:string[];items:{id:string;title:string;mediaId:string|null;sourceProgress:CourseImportProgress|null;targetProgress:CourseImportProgress|null;conflictCodes:string[]}[]}[]};
export type CourseSourceSeries={id:string;title:string;courseCount:number};

export const learningApi={
  listUnassignedVideos:(expectedLibraryId:string,offset=0,limit=50)=>invoke<Page<UnassignedLearningVideo>>("list_learning_unassigned_videos",{expectedLibraryId,offset,limit}),
  listMembershipPreview:(expectedLibraryId:string,offset=0,limit=50)=>invoke<Page<MembershipPreview>>("list_learning_membership_preview",{expectedLibraryId,offset,limit}),
  applyMembershipChanges:(expectedLibraryId:string,changes:MembershipChange[])=>invoke<void>("apply_learning_membership_changes",{expectedLibraryId,changes}),
  startExcerptTask:(expectedLibraryId:string,request:ExcerptTaskRequest)=>invoke<LibraryTask>("start_library_task",{expectedLibraryId,payload:{kind:"courseExcerpt",request:writableExcerptTask(request)}}),
  getExcerpt:(expectedLibraryId:string,previewId:string)=>invoke<StoredExcerpt>("get_learning_excerpt",{expectedLibraryId,previewId}),
  getExcerptOperation:(expectedLibraryId:string,operationId:string)=>invoke<ExcerptOperation>("get_learning_excerpt_operation",{expectedLibraryId,operationId}),
  listExcerptOperations:(expectedLibraryId:string,courseId:string,offset=0,limit=20)=>invoke<Page<ExcerptOperationSummary>>("list_learning_excerpt_operations",{expectedLibraryId,courseId,offset,limit}),
  cancelExcerptPreview:(expectedLibraryId:string,previewId:string)=>invoke<void>("cancel_learning_excerpt_preview",{expectedLibraryId,previewId}),
  cancelExcerptOperation:(expectedLibraryId:string,operationId:string)=>invoke<void>("cancel_learning_excerpt_operation",{expectedLibraryId,operationId}),
  excerptTargets:(expectedLibraryId:string)=>invoke<ExcerptTargets>("get_learning_excerpt_targets",{expectedLibraryId}),
  listProjectNotes:(expectedLibraryId:string,projectId:string,offset=0,limit=50)=>invoke<Page<import("./types").ProjectLearningNote>>("list_project_learning_notes",{expectedLibraryId,projectId,offset,limit}),
  linkProjectNote:(expectedLibraryId:string,projectId:string,noteId:string)=>invoke<void>("link_project_learning_note",{expectedLibraryId,projectId,noteId}),
  unlinkProjectNote:(expectedLibraryId:string,projectId:string,noteId:string)=>invoke<void>("unlink_project_learning_note",{expectedLibraryId,projectId,noteId}),
  prepareBoardExcerptJump:(expectedLibraryId:string,boardId:string,itemId:string)=>invoke<import("./types").BoardExcerptJump|null>("prepare_learning_board_excerpt_jump",{expectedLibraryId,boardId,itemId}),
  listNoteArchives:(expectedLibraryId:string,offset=0,limit=50)=>invoke<import("./types").NoteArchivePage>("list_learning_note_archives",{expectedLibraryId,offset,limit}),
  readNoteArchive:(expectedLibraryId:string,archiveId:string)=>invoke<import("./types").NoteArchiveView>("read_learning_note_archive",{expectedLibraryId,archiveId}),
  exportNoteArchive:(expectedLibraryId:string,archiveId:string,path:string)=>invoke<void>("export_learning_note_archive",{expectedLibraryId,archiveId,path}),
  readNotePreviewBlock:(expectedLibraryId:string,previewId:string,placeholderId:string)=>invoke<string>("read_note_conversion_block",{expectedLibraryId,previewId,placeholderId}),
  prepareNotePreview:(expectedLibraryId:string,request:NotePreviewRequest)=>invoke<StoredNotePreview>("prepare_note_conversion_preview",{expectedLibraryId,request}),
  getNotePreview:(expectedLibraryId:string,previewId:string)=>invoke<StoredNotePreview>("get_note_conversion_preview",{expectedLibraryId,previewId}),
  commitNotePreview:(expectedLibraryId:string,previewId:string)=>invoke<NoteCommitReceipt>("commit_note_preview",{expectedLibraryId,previewId}),
  cancelNotePreview:(expectedLibraryId:string,previewId:string)=>invoke<string[]>("cancel_note_conversion_preview",{expectedLibraryId,previewId}),
  prepareNoteJump:(expectedLibraryId:string,reference:import("../../lib/learningNoteNodes").LearningReferenceData,rangeIndex:number)=>invoke<{courseId:string;mediaId:string;positionMs:number;endMs:number}>("prepare_learning_note_jump",{expectedLibraryId,reference,rangeIndex}),
  transcriptionComponents:(expectedLibraryId:string)=>invoke<import("./types").WhisperComponentStatus>("get_learning_transcription_components",{expectedLibraryId}),
  importTranscriptionModel:(expectedLibraryId:string,modelId:string,selectedPath:string)=>invoke<import("./types").WhisperComponentStatus>("import_learning_transcription_model",{expectedLibraryId,modelId,selectedPath}),
  installTranscriptionTools:(expectedLibraryId:string,selectedResourceRoot:string)=>invoke<import("./types").WhisperComponentStatus>("install_learning_transcription_tools",{expectedLibraryId,selectedResourceRoot}),
  listCourseAnalyses:(expectedLibraryId:string,courseId:string,offset=0,limit=50)=>invoke<Page<CourseAnalysisHistoryItem>>("list_course_analyses",{expectedLibraryId,courseId,offset,limit}),
  getCourseAnalysis:(expectedLibraryId:string,courseId:string,analysisId:string,stageKey:string|null=null,offset=0,limit=50)=>invoke<CourseAnalysisDetail>("get_course_analysis",{expectedLibraryId,courseId,analysisId,stageKey,offset,limit}),
  listCourseAnalysisTemplates:(expectedLibraryId:string)=>invoke<CourseAnalysisTemplate[]>("list_course_analysis_templates",{expectedLibraryId}),
  saveCourseAnalysisTemplate:(expectedLibraryId:string,id:string|null,version:number|null,name:string,focus:string)=>invoke<CourseAnalysisTemplate>("save_course_analysis_template",{expectedLibraryId,id,version,name,focus}),
  deleteCourseAnalysisTemplate:(expectedLibraryId:string,id:string,version:number)=>invoke<void>("delete_course_analysis_template",{expectedLibraryId,id,version}),
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
  bindImportSource:(expectedLibraryId:string,vault:string)=>invoke<ImportSourceBinding|null>("bind_learning_import_source",{expectedLibraryId,vault}),
  selectCourseSource:(expectedLibraryId:string)=>invoke<CourseSourceBinding|null>("select_learning_course_source",{expectedLibraryId}),
  selectOfflineCourseSource:(expectedLibraryId:string)=>invoke<CourseSourceBinding|null>("select_learning_offline_course_source",{expectedLibraryId}),
  currentCourseSource:(expectedLibraryId:string)=>invoke<CourseSourceBinding|null>("current_learning_course_source",{expectedLibraryId}),
  releaseCourseSource:(expectedLibraryId:string,selectionId:string)=>invoke<boolean>("release_learning_course_source",{expectedLibraryId,selectionId}),
  listCourseSourceSeries:(expectedLibraryId:string,selectionId:string)=>invoke<CourseSourceSeries[]>("list_learning_course_source_series",{expectedLibraryId,selectionId}),
  previewCourseImport:(expectedLibraryId:string,selectionId:string,bindings:CourseImportBinding[],seriesIds?:string[])=>invoke<CourseSqlitePreview>("preview_learning_course_import",{expectedLibraryId,selectionId,bindings:bindings.map(({sourceItemId,mediaId})=>({sourceItemId,mediaId})),...(seriesIds?{seriesIds:[...seriesIds]}:{})}),
  queueCourseImport:(expectedLibraryId:string,selectionId:string,previewId:string)=>invoke<LibraryTask>("queue_learning_course_import",{expectedLibraryId,selectionId,previewId}),
  pickImportNote:(expectedLibraryId:string,profileId:string,epoch:string,vault:string,selectedRoot:string)=>invoke<ImportNotePreview|null>("pick_learning_import_note",{expectedLibraryId,profileId,epoch,vault,selectedRoot}),
  pickImportNotes:(expectedLibraryId:string,profileId:string,epoch:string,vault:string,selectedRoot:string)=>invoke<ImportNotePreview[]|null>("pick_learning_import_notes",{expectedLibraryId,profileId,epoch,vault,selectedRoot}),
  previewImportNote:(expectedLibraryId:string,profileId:string,epoch:string,vault:string,selectedRoot:string,relativePath:string)=>invoke<ImportNotePreview>("preview_learning_import_note",{expectedLibraryId,profileId,epoch,vault,selectedRoot,relativePath}),
  queueImportPrepare:(expectedLibraryId:string,profileId:string,epoch:string,vault:string,selectedRoot:string,relativePath:string,expectedSourceSha256:string,expectedSelectionId:string,expectedTargetToken:string)=>invoke<LibraryTask>("queue_learning_import_prepare",{expectedLibraryId,profileId,epoch,vault,selectedRoot,relativePath,expectedSourceSha256,expectedSelectionId,expectedTargetToken}),
  queueImportPrepareBatch:(expectedLibraryId:string,profileId:string,epoch:string,vault:string,selectedRoot:string,notes:{relativePath:string;sourceSha256:string;targetToken:string}[],expectedSelectionId:string)=>invoke<LibraryTask>("queue_learning_import_prepare_batch",{expectedLibraryId,profileId,epoch,vault,selectedRoot,notes,expectedSelectionId}),
  listImportBatches:(expectedLibraryId:string)=>invoke<ImportBatchView[]>("list_learning_import_batches",{expectedLibraryId}),
  listCourseImportBatches:(expectedLibraryId:string,cursor?:CourseBatchCursor|null)=>invoke<CourseImportBatchSummary[]>("list_learning_course_batches",{expectedLibraryId,cursor:cursor?{createdAt:cursor.createdAt,id:cursor.id}:null}),
  listCourseBatchEntries:(expectedLibraryId:string,batchId:string,offset:number)=>invoke<CourseImportEntryPage>("list_learning_course_batch_entries",{expectedLibraryId,batchId,offset}),
  listImportItems:(expectedLibraryId:string,batchId:string)=>invoke<ImportItemView[]>("list_learning_import_items",{expectedLibraryId,batchId}),
};
