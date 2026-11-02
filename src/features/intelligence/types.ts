import type {Asset} from "../assets/types";
export const relationNames = {
  inspired_by:"灵感来自", generated_from:"生成自", derived_from:"衍生自",
  used_in:"用于", reference_of:"作为参考", variant_of:"变体", related_to:"相关",
} as const;
export type RelationType = keyof typeof relationNames;
export interface RelationInput {sourceAssetId:string;targetAssetId:string;relationType:RelationType}
export interface AssetRelation extends RelationInput {id:string;createdAt:string}
export interface AssetGraph {center:Asset;nodes:Asset[];relations:AssetRelation[];total:number;offset:number;limit:number}
export type AnalysisMode="text"|"visual";
export interface AnalysisRequest {assetId:string;previewId:string;force:boolean}
export interface AnalysisInputPreview {id:string;assetId:string;model:string;mode:AnalysisMode;summary:string;fingerprint:string;images:{dataUrl:string;timestamp:number|null;label:string}[];estimatedTokens:number;warnings:string[];cachedAnalysisId:string|null}
export interface AssetAnalysis {id:string;assetId:string;description:string;keywords:string[];scene:string;style:string;camera:string;lighting:string;mood:string;model:string;mode:AnalysisMode;inputFingerprint:string;formatVersion:number;createdAt:string;actualTokens:number|null;validity:"current"|"stale"|"unknown";validityMessage:string}
export interface PromptInput {assetId:string|null;expectedRevision:number|null;name:string;description:string;content:string;model:string;parameters:Record<string,unknown>}
export interface PromptAsset {asset:Asset;content:string;model:string;parameters:Record<string,unknown>;revision:number;createdAt:string;updatedAt:string}
export interface ShotInput {assetId:string|null;expectedRevision:number|null;projectId:string|null;projectUnitId:string|null;parentSceneId:string|null;act:string;shotNumber:string;name:string;description:string;status:string}
export interface ShotAsset {asset:Asset;revision:number;projectId:string|null;projectUnitId:string|null;act:string;shotNumber:string;status:string;unit:import("../../types").ProjectUnit|null;createdAt:string;updatedAt:string}
