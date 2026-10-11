import {File,Image,Film,AudioLines,Box} from "lucide-react";
import {ImagePreview} from "../../components/ImagePreview";
import {assetMediaUrl,mediaLibraryFileUrl} from "../../lib/api";
import type {Asset,SourceFileRef} from "./types";
export function fileUrl(file:SourceFileRef){return file.kind==="mediaFile"?mediaLibraryFileUrl(file.id):assetMediaUrl(file.id,file.role==="thumbnail"?"thumbnail":file.role==="original"?"original":"stream");}
export const typeNames:Record<string,string>={asset:"普通素材",image:"图片",video:"视频",audio:"音频",model:"三维模型",prompt:"Prompt",shot:"镜头 Shot"};
export function AssetCard({asset,onOpen}:{asset:Asset;onOpen:()=>void}){
  const Icon={asset:File,image:Image,video:Film,audio:AudioLines,model:Box,prompt:File,shot:Film}[asset.assetType];
  return <button className="core-asset-card" aria-label={`打开 ${asset.name}`} onClick={onOpen}><div className="core-card-preview">{asset.thumbnail?asset.thumbnail.kind==="image"?<ImagePreview imageId={asset.thumbnail.id} alt={asset.name}/>:<img loading="lazy" src={fileUrl(asset.thumbnail)} alt={asset.name}/>:<Icon aria-hidden="true" size={32}/>}</div><div className="core-card-copy"><strong title={asset.name}>{asset.name}</strong><span>{typeNames[asset.assetType]}</span><small>{asset.tags.slice(0,3).join(" · ")||"未设置标签"}</small><time>{new Date(asset.updatedAt).toLocaleDateString("zh-CN")}</time></div></button>;
}
