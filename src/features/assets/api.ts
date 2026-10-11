import {invoke} from "@tauri-apps/api/core";
import type {Asset,AssetFile,AssetSource,AssetVersion,CoreAssetDetail,CoreSearchRequest,CoreSourceInput,CoreVersionInput,Page,ScanRow,SourceFileRef} from "./types";
export const coreApi={
  search:(expectedLibraryId:string,request:CoreSearchRequest)=>invoke<Page<Asset>>("search_core_assets",{expectedLibraryId,request}),
  get:(expectedLibraryId:string,assetId:string,locale:string)=>invoke<CoreAssetDetail>("get_core_asset",{expectedLibraryId,assetId,locale}),
  scanPage:(expectedLibraryId:string,taskId:string,offset=0)=>invoke<Page<ScanRow>>("get_asset_scan_page",{expectedLibraryId,taskId,offset,limit:100}),
  tags:(expectedLibraryId:string,assetId:string,locale:string,tags:string[])=>invoke<void>("set_core_asset_tags",{expectedLibraryId,assetId,locale,tags}),
  color:(expectedLibraryId:string,tagId:string,color:string)=>invoke<void>("set_core_tag_color",{expectedLibraryId,tagId,color}),
  project:(expectedLibraryId:string,assetId:string,projectId:string,link:boolean)=>invoke<void>("link_core_asset_project",{expectedLibraryId,assetId,projectId,link}),
  attach:(expectedLibraryId:string,assetId:string,source:SourceFileRef)=>invoke<AssetFile>("attach_core_asset_file",{expectedLibraryId,assetId,source}),
  createVersion:(expectedLibraryId:string,input:CoreVersionInput)=>invoke<AssetVersion>("create_core_asset_version",{expectedLibraryId,input}),
  deleteVersion:(expectedLibraryId:string,versionId:string)=>invoke<void>("delete_core_asset_version",{expectedLibraryId,versionId}),
  addSource:(expectedLibraryId:string,input:CoreSourceInput)=>invoke<AssetSource>("add_core_asset_source",{expectedLibraryId,input}),
  removeSource:(expectedLibraryId:string,assetId:string,sourceId:string)=>invoke<void>("remove_core_asset_source",{expectedLibraryId,assetId,sourceId}),
};
