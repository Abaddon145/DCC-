import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api, mediaLibraryFileUrl } from "../lib/api";
import { LoadFeedback } from "./LoadFeedback";

export interface CompareImage { kind: "asset" | "media"; id: string; title: string }
const CompareContext = createContext<{ add: (image: CompareImage) => void; open: (images: CompareImage[]) => void } | null>(null);
export const useImageCompare = () => useContext(CompareContext);
const valid = (image: CompareImage) => /^(?:[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})$/i.test(image.id);

/** A two-image tray lives only as long as the current library, never copies files. */
export function ImageCompareProvider({ children }: { children: ReactNode }) {
  const [images, setImages] = useState<CompareImage[]>([]), [show, setShow] = useState(false);
  const add = (image: CompareImage) => { if (!valid(image)) return; setImages(previous => previous.some(value => value.kind === image.kind && value.id === image.id) ? previous : previous.length < 2 ? [...previous, image] : [previous[0], image]); };
  const open = (next: CompareImage[]) => { if (next.length !== 2 || !next.every(valid) || (next[0].kind === next[1].kind && next[0].id === next[1].id)) return; setImages(next); setShow(true); };
  return <CompareContext.Provider value={{ add, open }}>{children}{images.length > 0 && !show && <aside className="compare-tray" aria-label="图片对比待选"><strong>图片 A/B</strong>{images.map((image, i) => <span key={`${image.kind}-${image.id}`}>{i ? "B" : "A"}：{image.title}</span>)}<button className="primary-button compact" disabled={images.length !== 2} onClick={() => setShow(true)}>开始对比</button><button onClick={() => setImages([])}>清空</button></aside>}{show && images.length === 2 && <ImageCompareDialog images={images} onClose={() => setShow(false)} />}</CompareContext.Provider>;
}

function CompareSource({ image }: { image: CompareImage }) {
  const [url, setUrl] = useState<string | null>(null), [failed, setFailed] = useState(false), [retry, setRetry] = useState(0);
  useEffect(() => { let active = true; setUrl(null); setFailed(false); const load = image.kind === "media" ? Promise.resolve(mediaLibraryFileUrl(image.id)) : api.imageData(image.id, false); void load.then(value => { if (active) setUrl(value); }).catch(() => { if (active) setFailed(true); }); return () => { active = false; }; }, [image.kind, image.id, retry]);
  if (failed) return <div className="compare-error" role="alert">图片无法读取，原件可能已被删除。<button onClick={() => setRetry(value => value + 1)}>重试图片</button></div>;
  if (!url) return <LoadFeedback label="正在读取原图…" />;
  return <img src={url} alt={image.title} draggable={false} onError={() => setFailed(true)} />;
}

export function ImageCompareDialog({ images, onClose }: { images: CompareImage[]; onClose: () => void }) {
  const [mode, setMode] = useState<"split" | "single">("split"), [active, setActive] = useState(0);
  const root = useRef<HTMLElement>(null), percent = useRef<HTMLOutputElement>(null);
  const view = useRef({ scale: 1, x: 0, y: 0 }), frame = useRef(0);
  const drag = useRef<{ pointerId: number; x: number; y: number; ox: number; oy: number } | null>(null);
  const draw = () => { frame.current = 0; for (const stage of root.current?.querySelectorAll<HTMLElement>(".compare-stage") ?? []) { const wrap = stage.querySelector<HTMLElement>(".compare-image"); if (wrap) wrap.style.transform = `translate(${view.current.x * stage.clientWidth}px, ${view.current.y * stage.clientHeight}px) scale(${view.current.scale})`; } if (percent.current) percent.current.textContent = `${Math.round(view.current.scale * 100)}%`; };
  const schedule = () => { if (!frame.current) frame.current = requestAnimationFrame(draw); };
  const zoom = (factor: number, x = 0, y = 0) => { const v = view.current, scale = Math.max(.25, Math.min(8, v.scale * factor)); view.current = { scale, x: x - (x - v.x) * scale / v.scale, y: y - (y - v.y) * scale / v.scale }; schedule(); };
  const reset = () => { view.current = { scale: 1, x: 0, y: 0 }; schedule(); };
  useLayoutEffect(() => { draw(); const node = root.current; if (!node) return; const observer = new ResizeObserver(draw); observer.observe(node); return () => observer.disconnect(); }, [mode, active]);
  useEffect(() => { const previous = document.activeElement as HTMLElement | null; root.current?.querySelector<HTMLButtonElement>("button")?.focus(); return () => { cancelAnimationFrame(frame.current); drag.current = null; if (previous?.isConnected) previous.focus(); }; }, []);
  useEffect(() => { const stages = Array.from(root.current?.querySelectorAll<HTMLElement>(".compare-stage") ?? []); const wheel = (e: WheelEvent) => { e.preventDefault(); const stage = e.currentTarget as HTMLElement, rect = stage.getBoundingClientRect(); zoom(Math.exp(-e.deltaY * .0015), (e.clientX - rect.left) / Math.max(1, rect.width) - .5, (e.clientY - rect.top) / Math.max(1, rect.height) - .5); }; for (const stage of stages) stage.addEventListener("wheel", wheel, { passive: false }); return () => { for (const stage of stages) stage.removeEventListener("wheel", wheel); }; }, [mode, active]);
  if (images.length !== 2) return null;
  return <div className="lightbox image-compare" role="presentation"><section ref={root} role="dialog" aria-modal="true" aria-label="图片 A/B 对比" onKeyDown={e => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); onClose(); }
    else if (["a", "b", "A", "B"].includes(e.key)) { setActive(e.key.toLowerCase() === "a" ? 0 : 1); setMode("single"); }
    else if (e.key === "+" || e.key === "=") { e.preventDefault(); zoom(1.2); }
    else if (e.key === "-") { e.preventDefault(); zoom(1 / 1.2); }
    else if (e.key === "0") reset();
    else if (e.key === "Tab") { const nodes = Array.from(e.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")); if (e.shiftKey && document.activeElement === nodes[0]) { e.preventDefault(); nodes.at(-1)?.focus(); } else if (!e.shiftKey && document.activeElement === nodes.at(-1)) { e.preventDefault(); nodes[0]?.focus(); } }
  }}>
    <header className="compare-header"><div><h2>图片对比</h2><p>同步缩放与平移，按 A / B 切换单图</p></div><div><button aria-pressed={mode === "split"} onClick={() => setMode("split")}>并排</button><button aria-pressed={mode === "single" && active === 0} onClick={() => { setActive(0); setMode("single"); }}>A</button><button aria-pressed={mode === "single" && active === 1} onClick={() => { setActive(1); setMode("single"); }}>B</button><button aria-label="缩小对比图片" onClick={() => zoom(1 / 1.2)}>−</button><output ref={percent} aria-label="对比缩放比例">100%</output><button aria-label="放大对比图片" onClick={() => zoom(1.2)}>+</button><button onClick={reset}>适应窗口</button><button aria-label="关闭图片对比" onClick={onClose}>关闭</button></div></header>
    <div className={`compare-panels ${mode}`} >{(mode === "split" ? [0, 1] : [active]).map(i => <article key={i}><header><strong>{i ? "B" : "A"}</strong><span title={images[i].title}>{images[i].title}</span></header><div className="compare-stage" onPointerDown={e => { if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return; e.preventDefault(); drag.current = { pointerId: e.pointerId, x: e.clientX, y: e.clientY, ox: view.current.x, oy: view.current.y }; e.currentTarget.setPointerCapture(e.pointerId); }} onPointerMove={e => { const d = drag.current; if (!d || d.pointerId !== e.pointerId) return; view.current.x = d.ox + (e.clientX - d.x) / Math.max(1, e.currentTarget.clientWidth); view.current.y = d.oy + (e.clientY - d.y) / Math.max(1, e.currentTarget.clientHeight); schedule(); }} onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }} onLostPointerCapture={() => { drag.current = null; }} onDoubleClick={reset}><div className="compare-image"><CompareSource image={images[i]} /></div></div></article>)}</div>
  </section></div>;
}
