import {invoke} from "@tauri-apps/api/core";
import type {AssetGraph,AssetRelation,RelationInput,AnalysisMode,AnalysisInputPreview,AssetAnalysis} from "./types";
export const intelligenceApi = {
  prepareAnalysis:(expectedLibraryId:string,assetId:string,mode:AnalysisMode)=>invoke<AnalysisInputPreview>("prepare_asset_analysis",{expectedLibraryId,assetId,mode}),
  getAnalysis:(expectedLibraryId:string,assetId:string,analysisId?:string)=>invoke<AssetAnalysis|null>("get_asset_analysis",{expectedLibraryId,assetId,analysisId:analysisId||null}),
  applyAnalysisTags:(expectedLibraryId:string,assetId:string,analysisId:string,locale:string,selected:string[])=>invoke<void>("apply_asset_analysis_tags",{expectedLibraryId,assetId,analysisId,locale,selected}),
  graph:(expectedLibraryId:string,assetId:string,locale:string,offset=0)=>invoke<AssetGraph>("get_asset_graph",{expectedLibraryId,request:{assetId,locale,offset,limit:80}}),
  createRelation:(expectedLibraryId:string,input:RelationInput)=>invoke<AssetRelation>("create_asset_relation",{expectedLibraryId,input}),
  deleteRelation:(expectedLibraryId:string,assetId:string,relationId:string)=>invoke<void>("delete_asset_relation",{expectedLibraryId,assetId,relationId}),
};
