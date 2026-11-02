import {invoke} from "@tauri-apps/api/core";
import type {AssetGraph,AssetRelation,RelationInput,AnalysisMode,AnalysisInputPreview,AssetAnalysis,PromptInput,PromptAsset,ShotInput,ShotAsset} from "./types";
export const intelligenceApi = {
  upsertShot:(expectedLibraryId:string,input:ShotInput)=>invoke<ShotAsset>("upsert_shot_asset",{expectedLibraryId,input}),
  getShot:(expectedLibraryId:string,assetId:string)=>invoke<ShotAsset>("get_shot_asset",{expectedLibraryId,assetId}),
  listShots:(expectedLibraryId:string,projectId:string,offset=0)=>invoke<import("../assets/types").Page<ShotAsset>>("list_project_shot_assets",{expectedLibraryId,projectId,offset,limit:200}),
  upsertPrompt:(expectedLibraryId:string,input:PromptInput)=>invoke<PromptAsset>("upsert_prompt_asset",{expectedLibraryId,input}),
  getPrompt:(expectedLibraryId:string,assetId:string)=>invoke<PromptAsset>("get_prompt_asset",{expectedLibraryId,assetId}),
  trashTextAssets:(expectedLibraryId:string,assetIds:string[])=>invoke<import("../../types").DeleteResult>("trash_text_assets",{expectedLibraryId,assetIds}),
  prepareAnalysis:(expectedLibraryId:string,assetId:string,mode:AnalysisMode)=>invoke<AnalysisInputPreview>("prepare_asset_analysis",{expectedLibraryId,assetId,mode}),
  getAnalysis:(expectedLibraryId:string,assetId:string,analysisId?:string)=>invoke<AssetAnalysis|null>("get_asset_analysis",{expectedLibraryId,assetId,analysisId:analysisId||null}),
  applyAnalysisTags:(expectedLibraryId:string,assetId:string,analysisId:string,locale:string,selected:string[])=>invoke<void>("apply_asset_analysis_tags",{expectedLibraryId,assetId,analysisId,locale,selected}),
  graph:(expectedLibraryId:string,assetId:string,locale:string,offset=0)=>invoke<AssetGraph>("get_asset_graph",{expectedLibraryId,request:{assetId,locale,offset,limit:80}}),
  createRelation:(expectedLibraryId:string,input:RelationInput)=>invoke<AssetRelation>("create_asset_relation",{expectedLibraryId,input}),
  deleteRelation:(expectedLibraryId:string,assetId:string,relationId:string)=>invoke<void>("delete_asset_relation",{expectedLibraryId,assetId,relationId}),
};
