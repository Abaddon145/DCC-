import { useState } from "react";
import { Box, Music2, Play, TriangleAlert } from "lucide-react";
import { assetMediaUrl } from "../lib/api";
import type { AssetMedia, AssetMediaKind } from "../types";

export function MediaPreview({ media, className = "", hoverPlay = false }: { media: AssetMedia; className?: string; hoverPlay?: boolean }) {
  const [hovered, setHovered] = useState(false);
  if (media.processingStatus === "error") return <div className={`media-placeholder error ${className}`}><TriangleAlert size={22} /><span>处理失败</span></div>;
  if (media.kind === "video" && hoverPlay && hovered) return <video className={className} muted loop autoPlay playsInline preload="metadata" src={assetMediaUrl(media.id)} onMouseLeave={() => setHovered(false)} />;
  if (media.hasThumbnail) return <div className={`media-thumb-wrap ${className}`} onMouseEnter={() => setHovered(true)}><img src={assetMediaUrl(media.id, "thumbnail")} alt={media.originalName} /><span className="media-kind-badge">{media.kind === "video" ? <Play size={14} /> : <Music2 size={14} />}</span></div>;
  return <div className={`media-placeholder ${className}`} onMouseEnter={() => setHovered(true)}>{media.kind === "model" ? <Box size={28} /> : media.kind === "audio" ? <Music2 size={28} /> : <Play size={28} />}<span>{media.originalName}</span></div>;
}

export function MediaCover({ mediaId, kind, className = "", hoverPlay = false }: { mediaId: string; kind: AssetMediaKind; className?: string; hoverPlay?: boolean }) {
  const [hovered, setHovered] = useState(false);
  if (kind === "video" && hoverPlay && hovered) return <video className={className} muted loop autoPlay playsInline preload="metadata" src={assetMediaUrl(mediaId)} onMouseLeave={() => setHovered(false)} />;
  if (kind === "video" || kind === "audio") return <div className={`media-thumb-wrap ${className}`} onMouseEnter={() => setHovered(true)}><img src={assetMediaUrl(mediaId, "thumbnail")} alt="媒体封面" /><span className="media-kind-badge">{kind === "video" ? <Play size={14} /> : <Music2 size={14} />}</span></div>;
  return <div className={`media-placeholder ${className}`}><Box size={28} /><span>3D</span></div>;
}
