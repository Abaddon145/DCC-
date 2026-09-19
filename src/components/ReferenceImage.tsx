import { useEffect, useState } from "react";
import { ImageOff } from "lucide-react";
import { api } from "../lib/api";

const cache = new Map<string, string>();
const order: string[] = [];
const MAX_CACHE = 96;

async function load(itemId: string, thumbnail: boolean) {
  const key = `${itemId}:${thumbnail ? "thumb" : "full"}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const data = await api.referenceImageData(itemId, thumbnail);
  cache.set(key, data); order.push(key);
  while (order.length > MAX_CACHE) { const oldest = order.shift(); if (oldest) cache.delete(oldest); }
  return data;
}

export function ReferenceImage({ itemId, name, thumbnail }: { itemId: string; name: string; thumbnail: boolean }) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true; setFailed(false); setSrc(null);
    load(itemId, thumbnail).then(value => { if (active) setSrc(value); }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [itemId, thumbnail]);
  if (failed) return <div className="reference-image-missing"><ImageOff size={26} /><span>图片缺失</span></div>;
  if (!src) return <div className="reference-image-loading" />;
  return <img src={src} alt={name} draggable={false} />;
}
