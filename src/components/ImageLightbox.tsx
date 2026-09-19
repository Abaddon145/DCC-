import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Maximize2, Minus, Plus, X } from "lucide-react";
import type { AssetImage } from "../types";
import { ImagePreview, preloadImage } from "./ImagePreview";

interface Props { images: AssetImage[]; initialIndex: number; title: string; onClose: () => void }

export function ImageLightbox({ images, initialIndex, title, onClose }: Props) {
  const [index, setIndex] = useState(Math.max(0, Math.min(initialIndex, images.length - 1)));
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const drag = useRef<{ x: number; y: number; ox: number; oy: number } | null>(null);
  const image = images[index];
  const reset = () => { setZoom(1); setOffset({ x: 0, y: 0 }); };
  const move = (delta: number) => { setIndex(value => (value + delta + images.length) % images.length); reset(); };
  const setZoomClamped = (value: number) => setZoom(Math.max(.25, Math.min(4, value)));

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      if (event.key === "ArrowLeft") move(-1);
      if (event.key === "ArrowRight") move(1);
      if (event.key === "+" || event.key === "=") setZoom(value => Math.min(4, value + .25));
      if (event.key === "-") setZoom(value => Math.max(.25, value - .25));
      if (event.key === "0") reset();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [images.length, onClose]);

  useEffect(() => {
    [-1, 1].forEach(delta => {
      const adjacent = images[(index + delta + images.length) % images.length];
      if (adjacent && adjacent.id !== image?.id) preloadImage(adjacent.id, false).catch(() => undefined);
    });
  }, [images, index, image?.id]);

  if (!image) return null;
  return <div className="lightbox" role="dialog" aria-modal="true" aria-label={`${title} 图片浏览器`}>
    <header className="lightbox-header"><div><strong>{title}</strong><span>{index + 1} / {images.length} · {image.originalName}</span></div><button aria-label="关闭大图" onClick={onClose}><X size={21} /></button></header>
    <div className="lightbox-stage" onWheel={event => { event.preventDefault(); setZoomClamped(zoom + (event.deltaY < 0 ? .2 : -.2)); }} onPointerDown={event => { if (zoom <= 1) return; drag.current = { x: event.clientX, y: event.clientY, ox: offset.x, oy: offset.y }; event.currentTarget.setPointerCapture(event.pointerId); }} onPointerMove={event => { if (!drag.current) return; setOffset({ x: drag.current.ox + event.clientX - drag.current.x, y: drag.current.oy + event.clientY - drag.current.y }); }} onPointerUp={() => { drag.current = null; }} onDoubleClick={() => { if (zoom === 1) setZoom(2); else reset(); }}>
      {images.length > 1 && <button aria-label="上一张" className="lightbox-nav previous" onClick={() => move(-1)}><ChevronLeft size={28} /></button>}
      <div className="lightbox-image-wrap" style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${zoom})` }}><ImagePreview imageId={image.id} thumbnail={false} alt={image.originalName} className="lightbox-image" /></div>
      {images.length > 1 && <button aria-label="下一张" className="lightbox-nav next" onClick={() => move(1)}><ChevronRight size={28} /></button>}
    </div>
    <footer className="lightbox-footer">
      <div className="lightbox-tools"><button aria-label="缩小" onClick={() => setZoomClamped(zoom - .25)}><Minus size={16} /></button><span>{Math.round(zoom * 100)}%</span><button aria-label="放大" onClick={() => setZoomClamped(zoom + .25)}><Plus size={16} /></button><button aria-label="适应窗口" onClick={reset} title="适应窗口 (0)"><Maximize2 size={16} /></button></div>
      <div className="lightbox-thumbs">{images.map((item, itemIndex) => <button key={item.id} className={itemIndex === index ? "active" : ""} onClick={() => { setIndex(itemIndex); reset(); }}><ImagePreview imageId={item.id} alt={item.originalName} className="lightbox-thumb" /></button>)}</div>
    </footer>
  </div>;
}
