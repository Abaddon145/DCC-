import { useEffect, useState } from "react";
import { ImageOff } from "lucide-react";
import { api } from "../lib/api";

interface Props { imageId: string | null; alt: string; thumbnail?: boolean; className?: string; style?: React.CSSProperties }

const imageCache = new Map<string, string>();
const cacheKey = (imageId: string, thumbnail: boolean) => `${imageId}:${thumbnail ? "thumb" : "full"}`;

export async function preloadImage(imageId: string, thumbnail = false) {
  const key = cacheKey(imageId, thumbnail);
  if (!imageCache.has(key)) imageCache.set(key, await api.imageData(imageId, thumbnail));
  return imageCache.get(key)!;
}

export function ImagePreview({ imageId, alt, thumbnail = true, className = "", style }: Props) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let active = true;
    setSrc(null); setFailed(false);
    if (!imageId) return;
    const key = cacheKey(imageId, thumbnail);
    const cached = imageCache.get(key);
    if (cached) setSrc(cached);
    else preloadImage(imageId, thumbnail).then(value => active && setSrc(value)).catch(() => active && setFailed(true));
    return () => { active = false; };
  }, [imageId, thumbnail]);

  if (!imageId || failed) return <div className={`image-placeholder ${className}`} style={style}><ImageOff size={28} /><span>暂无预览</span></div>;
  if (!src) return <div className={`image-placeholder shimmer ${className}`} style={style} />;
  return <img className={className} style={style} src={src} alt={alt} draggable={false} />;
}
