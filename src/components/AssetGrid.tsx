import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Box, Check, Heart } from "lucide-react";
import type { AssetCard, LibraryPreferences } from "../types";
import type { LibraryDragPayload } from "../types";
import { ImagePreview } from "./ImagePreview";
import type { BeginLibraryPointerDrag } from "../lib/drag";
import { MediaCover } from "./MediaPreview";

interface Props {
  items: AssetCard[];
  total: number;
  loading: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onFavorite: (item: AssetCard) => void;
  onLoadMore: () => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  selectionMode?: boolean;
  selectedIds?: Set<string>;
  onToggleSelect?: (id: string, rangeIds?: string[]) => void;
  onPointerDragStart?: BeginLibraryPointerDrag;
  preferences?: LibraryPreferences;
  query?: string;
}

export function assetIdsForDrag(itemId: string, selectionMode: boolean, selectedIds: Set<string>) {
  return selectionMode && selectedIds.has(itemId) ? [...selectedIds] : [itemId];
}

export function AssetGrid({ items, total, loading, selectedId, onSelect, onFavorite, onLoadMore, scrollRef, selectionMode = false, selectedIds = new Set(), onToggleSelect, onPointerDragStart, preferences, query = "" }: Props) {
  const [width, setWidth] = useState(900);
  const lastSelected = useRef<number | null>(null);
  const viewMode = preferences?.assetView || "grid";
  const cardWidth = preferences?.cardSize === "small" ? 190 : preferences?.cardSize === "large" ? 330 : 252;
  const rowHeight = preferences?.cardSize === "small" ? 225 : preferences?.cardSize === "large" ? 326 : 267;
  const columns = viewMode === "list" ? 1 : Math.max(1, Math.floor(width / cardWidth));
  const rows = Math.ceil(items.length / columns);
  const observerRef = useRef<ResizeObserver | null>(null);

  useEffect(() => {
    const node = scrollRef.current;
    if (!node) return;
    observerRef.current = new ResizeObserver(entries => setWidth(entries[0].contentRect.width));
    observerRef.current.observe(node);
    return () => observerRef.current?.disconnect();
  }, [scrollRef]);

  const virtualizer = useVirtualizer({ count: rows, getScrollElement: () => scrollRef.current, estimateSize: () => viewMode === "list" ? 70 : rowHeight, overscan: viewMode === "list" ? 8 : 3 });
  const virtualRows = virtualizer.getVirtualItems();
  const lastIndex = virtualRows.at(-1)?.index ?? 0;
  useEffect(() => { if (!loading && items.length < total && lastIndex >= rows - 2) onLoadMore(); }, [lastIndex, rows, items.length, total, loading, onLoadMore]);

  const content = useMemo(() => virtualRows.map(row => {
    const slice = items.slice(row.index * columns, row.index * columns + columns);
    return <div className={viewMode === "list" ? "asset-list-virtual-row" : "asset-row"} key={row.key} style={{ transform: `translateY(${row.start}px)`, ...(viewMode === "grid" ? { gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` } : {}) }}>
      {slice.map(item => { const itemIndex = items.findIndex(value => value.id === item.id); const checked = selectedIds.has(item.id); const interaction = {
        onPointerDown: (event: React.PointerEvent<HTMLElement>) => {
        if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
        const ids = assetIdsForDrag(item.id, selectionMode, selectedIds);
        const payload: LibraryDragPayload = { kind: "assets", ids, label: ids.length > 1 ? `${ids.length} 项素材` : item.name };
        onPointerDragStart?.(payload, event);
      }, onClick: (event: React.MouseEvent<HTMLElement>) => {
        if (!selectionMode) return onSelect(item.id);
        let rangeIds: string[] | undefined;
        if (event.shiftKey && lastSelected.current !== null) { const [start, end] = [lastSelected.current, itemIndex].sort((a, b) => a - b); rangeIds = items.slice(start, end + 1).map(value => value.id); }
        lastSelected.current = itemIndex; onToggleSelect?.(item.id, rangeIds);
      }};
      if (viewMode === "list") return <article key={item.id} className={`asset-list-item ${selectedId === item.id ? "selected" : ""} ${checked ? "multi-selected" : ""}`} {...interaction}>
        {selectionMode && <button className={`list-select ${checked ? "active" : ""}`} onClick={event => { event.stopPropagation(); onToggleSelect?.(item.id); }}><span>{checked && <Check size={12} />}</span></button>}
        {item.coverImageId ? <ImagePreview imageId={item.coverImageId} alt={item.name} className="asset-list-thumb" /> : item.coverMediaId && item.coverMediaKind ? <MediaCover mediaId={item.coverMediaId} kind={item.coverMediaKind} className="asset-list-thumb" /> : <ImagePreview imageId={null} alt={item.name} className="asset-list-thumb" />}
        <div className="asset-list-name"><strong><HighlightedText text={item.name} query={query} /></strong>{preferences?.cardFields.category !== false && <span>{item.categoryName || "未分类"}</span>}</div>
        {preferences?.cardFields.software !== false && <span className="asset-list-cell">{item.dccTools.join("、") || "—"}</span>}
        {preferences?.cardFields.version !== false && <span className="asset-list-cell">{item.versions.join("、") || "—"}</span>}
        {preferences?.cardFields.format !== false && <span className="asset-list-cell">{item.formats.join("、") || "—"}</span>}
        {preferences?.cardFields.tags !== false && <span className="asset-list-tags">{item.tags.slice(0, 3).map(tag => `#${tag}`).join("  ") || "—"}</span>}
        {preferences?.cardFields.linkStatus !== false && <span className={`asset-list-status ${item.hasShareLink === false ? "missing" : item.linkCheckStatus}`}>{item.hasShareLink === false ? "无链接" : item.linkCheckStatus === "invalid" ? "失效" : item.linkCheckStatus === "valid" ? "有效" : "未检查"}</span>}
        <button className={`list-favorite ${item.favorite ? "active" : ""}`} onClick={event => { event.stopPropagation(); onFavorite(item); }}><Heart size={15} fill={item.favorite ? "currentColor" : "none"} /></button>
        <time>{new Date(item.updatedAt).toLocaleDateString("zh-CN")}</time>
      </article>;
      return <article key={item.id} className={`asset-card card-${preferences?.cardSize || "medium"} ${selectedId === item.id ? "selected" : ""} ${checked ? "multi-selected" : ""}`} {...interaction}>
        <div className="card-image">
          {item.coverImageId ? <ImagePreview imageId={item.coverImageId} alt={item.name} className="card-image-content" style={{ objectFit: preferences?.coverFit || "cover" }} /> : item.coverMediaId && item.coverMediaKind ? <MediaCover mediaId={item.coverMediaId} kind={item.coverMediaKind} className="card-image-content" hoverPlay /> : <ImagePreview imageId={null} alt={item.name} className="card-image-content" />}
          {preferences?.cardFields.linkStatus !== false && item.linkCheckStatus === "invalid" && <span className="link-invalid-badge">网盘失效</span>}
          {preferences?.cardFields.linkStatus !== false && item.hasShareLink === false && <span className="link-missing-badge">无网盘链接</span>}
          {selectionMode && <button className={`select-button ${checked ? "active" : ""}`} onClick={event => { event.stopPropagation(); lastSelected.current = itemIndex; onToggleSelect?.(item.id); }}><span>{checked && <Check size={13} />}</span></button>}
          <button className={`favorite-button ${item.favorite ? "active" : ""}`} title={item.favorite ? "取消收藏" : "收藏"} onClick={event => { event.stopPropagation(); onFavorite(item); }}>
            <Heart size={17} fill={item.favorite ? "currentColor" : "none"} />
          </button>
          {preferences?.cardFields.category !== false && item.categoryName && <span className="category-badge">{item.categoryName}</span>}
        </div>
        <div className="card-body">
          <h3 title={item.name}><HighlightedText text={item.name} query={query} />{item.languageFallback && <small className="language-fallback" title={item.contentLanguage === "en" ? "暂无中文，显示英文" : "No English version"}>{item.contentLanguage === "en" ? "EN" : "中"}</small>}</h3>
          <div className="card-meta">{preferences?.cardFields.software !== false && <span>{item.dccTools[0] || "DCC"}</span>}{preferences?.cardFields.version !== false && item.versions[0] && <span>{item.versions[0]}</span>}{preferences?.cardFields.format !== false && item.formats[0] && <span>{item.formats[0]}</span>}</div>
          <div className="card-bottom-row">{preferences?.cardFields.tags !== false && <div className="card-tags">{item.tags.slice(0, 3).map(tag => <span key={tag}>#{tag}</span>)}</div>}</div>
        </div>
      </article>; })}
    </div>;
  }), [virtualRows, items, columns, onSelect, onFavorite, selectedId, selectionMode, selectedIds, onToggleSelect, onPointerDragStart, preferences, viewMode, query]);

  if (!loading && items.length === 0) return <div className="empty-state"><div className="empty-icon"><Box size={34} /></div><h2>还没有找到素材</h2><p>调整搜索条件，或者添加第一条素材。</p></div>;
  return <div className="virtual-grid" style={{ height: virtualizer.getTotalSize() }}>{content}{loading && <div className="grid-loading">正在读取素材…</div>}</div>;
}

function HighlightedText({ text, query }: { text: string; query: string }) {
  const terms = query.match(/[\p{L}\p{N}_-]{2,}/gu)?.filter(term => !term.includes(":")) || [];
  if (!terms.length) return <>{text}</>;
  const escaped = terms.map(term => term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const regex = new RegExp(`(${escaped.join("|")})`, "ig");
  return <>{text.split(regex).map((part,index) => terms.some(term => term.toLowerCase() === part.toLowerCase()) ? <mark key={index}>{part}</mark> : part)}</>;
}
