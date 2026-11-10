export interface Course {
  id: string; mediaId: string; title: string; positionMs: number;
  durationMs: number | null; rate: number; completed: boolean; progressRevision: number;
}
export interface ExcerptSelection {subtitleId:string|null;cueIds:string[];ranges:import("../../lib/learningNoteNodes").LearningReferenceData["ranges"];comment:string;frameTimeMs:number|null}
export interface ExcerptText {courseId:string;mediaId:string;title:string;subtitleId:string|null;subtitleSha256:string|null;ranges:ExcerptSelection["ranges"];cues:TranscriptCue[];comment:string;frameTimeMs:number|null}
export interface ExcerptFrame {timeMs:number;sha256:string;dataUrl:string}
export interface PreparedExcerpt {libraryId:string;rootHash:string;sourceFileId:string;sourceSha256:string;durationMs:number;text:ExcerptText;document:import("@tiptap/react").JSONContent;frame:ExcerptFrame|null;warnings:string[]}
export interface StoredExcerpt {id:string;status:"ready"|"committed"|"cancelled";prepared:PreparedExcerpt}
export type ExcerptDestination={kind:"new";title:string;notebookId:string|null}|{kind:"append";noteId:string;expectedRevision:number;insertIndex:number}|{kind:"project";projectId:string;title:string;notebookId:string|null}|{kind:"board";boardId:string;x:number;y:number};
export interface CommitExcerptRequest {operationId:string;excerptId:string;destination:ExcerptDestination;includeFrame:boolean}
export interface ExcerptReceipt {operationId:string;excerptId:string;noteId?:string;revision?:number;boardId?:string;boardItemId?:string;warnings:string[]}
export interface CommitExcerptResult {receipt:ExcerptReceipt;targetAvailable:boolean}
export type ExcerptTaskRequest={action:"prepare";courseId:string;selection:ExcerptSelection}|{action:"commit";request:CommitExcerptRequest};
export interface ExcerptOperation {operationId:string;excerptId:string;status:"pending"|"committed"|"cancelled";request:CommitExcerptRequest;receipt:ExcerptReceipt|null;createdAt:string;previewReadable:boolean;warning:string|null}
export interface ExcerptOperationSummary {operationId:string;excerptId:string;status:ExcerptOperation["status"]|"unreadable";title:string;receipt:ExcerptReceipt|null;createdAt:string;error:string|null}
export interface ExcerptTargets {projects:import("../../types").ProjectSummary[];boards:import("../../types").ReferenceBoardSummary[]}
export interface ProjectLearningNote {id:string;title:string;updatedAt:string;courseCount:number}
export interface BoardExcerptJump {courseId:string;mediaId:string;positionMs:number;endMs:number;sourceFingerprint:string}
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
export interface Series {id:string;name:string;description:string;revision:number;orderedCourseIds:string[];categoryId?:string|null;coverFileId?:string|null}
export interface LearningCategory {id:string;name:string;revision:number}
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
export interface CourseAnalysisOptions {subtitleId?:string|null;frameTimesMs:number[];templateId?:string;templateVersion?:number}
export interface CourseAnalysisTemplate {id:string;name:string;focus:string;version:number}
export interface CourseAnalysisRequest {courseId:string;previewId:string;stage:"segments"|"summary";autoSummary?:boolean}
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
