import { useEffect, useState } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import type { AssetImage, AssetMedia, LinkedMediaPreview } from "../types";
import { ImagePreview } from "./ImagePreview";
import { MediaPreview } from "./MediaPreview";
import { ModelViewer } from "./ModelViewer";
import { assetMediaUrl, mediaLibraryBundleUrl, mediaLibraryFileUrl } from "../lib/api";

export type PreviewItem = { type: "image"; image: AssetImage } | { type: "media"; media: AssetMedia } | { type: "library"; media: LinkedMediaPreview };

export function MediaViewer({ items, initialIndex, title, onClose }: { items: PreviewItem[]; initialIndex: number; title: string; onClose: () => void }) {
  const [index, setIndex] = useState(Math.max(0, Math.min(initialIndex, items.length - 1)));
  const item = items[index];
  const move = (delta: number) => setIndex(value => (value + delta + items.length) % items.length);
  useEffect(() => { const handler = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); if (event.key === "ArrowLeft") move(-1); if (event.key === "ArrowRight") move(1); }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler); }, [items.length, onClose]);
  if (!item) return null;
  const name = item.type === "image" ? item.image.originalName : item.media.originalName;
  const content = item.type === "image" ? <ImagePreview imageId={item.image.id} thumbnail={false} alt={name} className="media-viewer-image" /> : item.type === "library" ? <LibraryMediaContent media={item.media} /> : item.media.kind === "video" ? <video controls autoPlay loop className="media-viewer-player" src={assetMediaUrl(item.media.id)} /> : item.media.kind === "audio" ? <div className="audio-viewer"><MediaPreview media={item.media} /><audio controls autoPlay src={assetMediaUrl(item.media.id)} /></div> : <ModelViewer media={item.media} />;
  return <div className="lightbox media-viewer" role="dialog" aria-modal="true"><header className="lightbox-header"><div><strong>{title}</strong><span>{index + 1} / {items.length} · {name}</span></div><button onClick={onClose}><X size={21} /></button></header><div className="media-viewer-stage">{items.length > 1 && <button className="lightbox-nav previous" onClick={() => move(-1)}><ChevronLeft size={28} /></button>}{content}{items.length > 1 && <button className="lightbox-nav next" onClick={() => move(1)}><ChevronRight size={28} /></button>}</div><footer className="lightbox-footer"><div className="lightbox-thumbs">{items.map((entry, itemIndex) => <button key={entry.type === "image" ? entry.image.id : `${entry.type}-${entry.media.id}`} className={itemIndex === index ? "active" : ""} onClick={() => setIndex(itemIndex)}>{entry.type === "image" ? <ImagePreview imageId={entry.image.id} alt={entry.image.originalName} className="lightbox-thumb" /> : entry.type === "library" ? <LibraryMediaPreview media={entry.media} className="lightbox-thumb" /> : <MediaPreview media={entry.media} className="lightbox-thumb" />}</button>)}</div></footer></div>;
}

export function LibraryMediaPreview({media,className=""}:{media:LinkedMediaPreview;className?:string}){const preview=media.thumbnailFileId||media.primaryFileId;if(media.kind==="image"||media.thumbnailFileId)return <div className={`media-thumb-wrap ${className}`}><img src={mediaLibraryFileUrl(preview)} alt={media.name}/><span className="media-kind-badge">{media.kind.toUpperCase()}</span></div>;return <div className={`media-placeholder ${className}`}><span>{media.kind==="model"?"3D":media.kind.toUpperCase()}</span></div>}
function LibraryMediaContent({media}:{media:LinkedMediaPreview}){if(media.kind==="image")return <img className="media-viewer-image" src={mediaLibraryFileUrl(media.primaryFileId)} alt={media.name}/>;if(media.kind==="video")return <video controls autoPlay loop className="media-viewer-player" src={mediaLibraryFileUrl(media.streamFileId)}/>;if(media.kind==="audio")return <div className="audio-viewer"><LibraryMediaPreview media={media}/><audio controls autoPlay src={mediaLibraryFileUrl(media.streamFileId)}/></div>;return <ModelViewer sourceUrl={mediaLibraryBundleUrl(media.id,media.logicalPath)} sourceName={media.originalName}/>}
