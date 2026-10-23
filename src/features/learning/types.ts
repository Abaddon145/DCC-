export interface Course {
  id: string; mediaId: string; title: string; positionMs: number;
  durationMs: number | null; rate: number; completed: boolean; progressRevision: number;
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
  | {type:"pause";paused:boolean}|{type:"seek";positionMs:number}|{type:"rate";rate:number}
  | {type:"volume";volume:number}|{type:"mute";muted:boolean}|{type:"subtitle"|"audio";trackId:number|null}
  | {type:"frameStep";direction:"forward"|"backward"}|{type:"aspect";ratio:"source"|"16:9"|"4:3"|"fill"}
  | {type:"subtitleDelay";delayMs:number}|{type:"abLoop";aMs:number;bMs:number}|{type:"clearAbLoop"}
  | {type:"fullscreen";enabled:boolean}|{type:"shortcuts";bindings:import("./nativeBindings").NativeBinding[]};
export interface ScopedPlaybackEvent {libraryId:string;sessionId:string;courseId:string}
export interface NativePointerEvent extends ScopedPlaybackEvent {kind:"click"|"doubleClick"|"wheelUp"|"wheelDown"|"context";doubleClickMs?:number}
