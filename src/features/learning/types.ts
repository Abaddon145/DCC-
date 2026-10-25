export interface Course {
  id: string; mediaId: string; title: string; positionMs: number;
  durationMs: number | null; rate: number; completed: boolean; progressRevision: number;
}
export interface NoteArchiveVersion {id:string;profileId:string;sourceKey:string;versionKey:string;sha256:string;size:number;createdAt:string;conversionState:"unconverted"|"partial"|"converted"}
export interface NoteArchivePage {versions:import("../../types").Page<NoteArchiveVersion>;unconvertedCount:number;partialCount:number}
export interface NoteArchiveView {archiveId:string;sha256:string;size:number;text:string|null}
export type NotePreviewDestination={kind:"new";title:string;notebookId:string|null}|{kind:"replace";noteId:string;expectedRevision:number};
export interface NotePreviewRequest {
  archiveId:string;destination:NotePreviewDestination;
  attachments:{sourceKey:string;attachmentId:string}[];courses:Record<string,string>;
}
export interface NoteCommitReceipt {noteId:string;revision:number;pending:boolean;warnings:string[]}
export interface StoredNotePreview {
  id:string;status:"ready"|"committed"|"cancelled";
  conversion:{document:import("@tiptap/react").JSONContent|null;archiveHash:string;warnings:string[];pending:boolean;unsupported:{placeholderId:string;startByte:number;endByte:number;label:string;reason:string}[]};
  receipt:NoteCommitReceipt|null;
}
export interface Series {id:string;name:string;description:string;revision:number;orderedCourseIds:string[]}
export interface SeriesInput {id?:string;name:string;description:string;expectedRevision?:number}
export interface ProgressInput {positionMs:number;durationMs:number|null;rate:number;completed:boolean}
export interface Progress extends ProgressInput {revision:number}
export interface ExternalVideoPreview {previewId:string;libraryId:string;name:string;sizeBytes:number;sha256:string;durationMs:number|null;expiresAt:number}
export interface PlaybackTrack {id:number;kind:"audio"|"sub";label:string}
export interface PlaybackSnapshot {
  sessionId:string;libraryId:string;courseId:string;ready:boolean;positionMs:number;durationMs:number|null;
  paused:boolean;rate:number;volume:number;muted:boolean;ended:boolean;tracks:PlaybackTrack[];
  audioId:number|null;subtitleId:number|null;fullscreen:boolean;error:string|null;saveError:string|null;
}
export interface NativeViewport {left:number;top:number;width:number;height:number;visible:boolean}
export type PlaybackAction =
  | {type:"completed";completed:boolean}
  | {type:"pause";paused:boolean}|{type:"seek";positionMs:number}|{type:"rate";rate:number}
  | {type:"volume";volume:number}|{type:"mute";muted:boolean}|{type:"subtitle"|"audio";trackId:number|null}
  | {type:"frameStep";direction:"forward"|"backward"}|{type:"aspect";ratio:"source"|"16:9"|"4:3"|"fill"}
  | {type:"subtitleDelay";delayMs:number}|{type:"abLoop";aMs:number;bMs:number}|{type:"clearAbLoop"}
  | {type:"fullscreen";enabled:boolean}|{type:"shortcuts";bindings:import("./nativeBindings").NativeBinding[]};
export interface ScopedPlaybackEvent {libraryId:string;sessionId:string;courseId:string}
export interface NativePointerEvent extends ScopedPlaybackEvent {kind:"click"|"doubleClick"|"wheelUp"|"wheelDown"|"context";doubleClickMs?:number}
export interface LearningSubtitle {id:string;courseId:string;name:string;format:"srt"|"vtt";sha256:string;cueCount:number}
export interface TranscriptCue {id:string;subtitleId:string;startMs:number;endMs:number;text:string}
export interface CourseTranscriptionRequest {courseId:string;modelId:"base"|"small"}
export interface WhisperModelStatus {id:"base"|"small";label:string;bytes:number;url:string;installed:boolean;message:string}
export interface WhisperComponentStatus {version:string;modelRevision:string;toolReady:boolean;toolMessage:string;models:WhisperModelStatus[]}
export interface CourseAnalysisOptions {subtitleId?:string|null;frameTimesMs:number[]}
export interface CourseAnalysisRequest {courseId:string;previewId:string;stage:"segments"|"summary"}
export interface CourseStagePreview {key:string;startMs:number;endMs:number;cached:boolean;tokensMax:number}
export interface CourseAnalysisPreview {
  previewId:string;courseId:string;fingerprint:string;subtitleId:string|null;frameIds:string[];
  model:string;templateId:string;templateVersion:number;promptVersion:string;createdAt:number;expiresAt:number;
  stages:CourseStagePreview[];tokensMax:number;warnings:string[];
}
export interface CourseAnalysisResult {
  contentType:"tutorial"|"lecture"|"interview"|"documentary"|"other";
  oneSentenceSummary:string;overview:string;
  chapters:{startMs:number;endMs:number;title:string;summary:string;keywords:string[];keyframeIds:string[]}[];
  keyPoints:{timeMs:number;title:string;content:string;importance:number;sourceSegmentIds:string[]}[];
  tutorialSteps:{order:number;timeMs:number;title:string;description:string;notes:string[]}[];
  terms:{name:string;definition:string;timeMs:number}[];
  actionItems:string[];tags:string[];warnings:string[];
  sections:{id:string;title:string;items:{title:string;content:string;timeMs:number|null;sourceSegmentIds:string[];keyframeIds:string[]}[]}[];
  sectionValidation:{stage:string;scopeStartMs:number;scopeEndMs:number;sectionId:string;itemIndex:number;itemTitle:string;disposition:string;reason:string}[];
}
export interface CourseAnalysisStage {key:string;startMs:number;endMs:number;status:"pending"|"completed"|"failed";tokensUsed:number|null;error:string}
export interface CourseAnalysisHistoryItem {
  id:string;stage:CourseAnalysisRequest["stage"];status:CourseAnalysisDetail["status"];
  completedStages:number;totalStages:number;error:string;createdAt:number;
}
export interface CourseAnalysisDetail {
  id:string;courseId:string;previewId:string;sourceFingerprint:string;model:string;
  parentAnalysisId:string|null;
  status:"queued"|"running"|"partial"|"completed"|"failed"|"cancelled";
  error:string;createdAt:number;updatedAt:number;staleReasons:string[];
  stages:import("../../types").Page<CourseAnalysisStage>;selectedStageKey:string|null;result:CourseAnalysisResult|null;
}
