import {invoke} from "@tauri-apps/api/core";
import type {AssetGraph,AssetRelation,RelationInput} from "./types";
export const intelligenceApi = {
  graph:(expectedLibraryId:string,assetId:string,locale:string,offset=0)=>invoke<AssetGraph>("get_asset_graph",{expectedLibraryId,request:{assetId,locale,offset,limit:80}}),
  createRelation:(expectedLibraryId:string,input:RelationInput)=>invoke<AssetRelation>("create_asset_relation",{expectedLibraryId,input}),
  deleteRelation:(expectedLibraryId:string,assetId:string,relationId:string)=>invoke<void>("delete_asset_relation",{expectedLibraryId,assetId,relationId}),
};
