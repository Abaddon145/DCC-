export interface Course {
  id: string; mediaId: string; title: string; positionMs: number;
  durationMs: number | null; rate: number; completed: boolean; progressRevision: number;
}
export interface Series {id:string;name:string;description:string;revision:number;orderedCourseIds:string[]}
export interface SeriesInput {id?:string;name:string;description:string;expectedRevision?:number}
export interface ProgressInput {positionMs:number;durationMs:number|null;rate:number;completed:boolean}
export interface Progress extends ProgressInput {revision:number}
export interface ExternalVideoPreview {previewId:string;libraryId:string;name:string;sizeBytes:number;sha256:string;durationMs:number|null;expiresAt:number}
