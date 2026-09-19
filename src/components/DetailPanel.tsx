import { Calendar, Copy, Edit3, ExternalLink, FileBox, Heart, Images, Link2, RefreshCw, Trash2, UserRound, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { AssetDetail, ContentLanguage } from "../types";
import { formatBytes } from "../lib/validation";
import { ImagePreview } from "./ImagePreview";
import { ImageLightbox } from "./ImageLightbox";
import { HoverDismissDetails } from "./HoverDismissDetails";

interface Props {
  asset: AssetDetail | null;
  contentLanguage: ContentLanguage;
  loading: boolean;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onFavorite: () => void;
  onOpen: () => void;
  onSourceOpen: () => void;
  onCopy: () => void;
  onCheckLink: () => void;
  checkingLink: boolean;
  onAddReference?: (imageIds: string[]) => void;
}

export function DetailPanel({ asset, contentLanguage, loading, onClose, onEdit, onDelete, onFavorite, onOpen, onSourceOpen, onCopy, onCheckLink, checkingLink, onAddReference }: Props) {
  const [selectedImage, setSelectedImage] = useState(0);
  const [lightbox, setLightbox] = useState(false);
  const [detailLanguage, setDetailLanguage] = useState<ContentLanguage>(contentLanguage);
  useEffect(() => {
    if (!asset) return;
    const cover = asset.images.findIndex(image => image.isCover);
    setSelectedImage(cover >= 0 ? cover : 0);
    setLightbox(false);
    setDetailLanguage(contentLanguage);
  }, [asset?.id]);
  if (!asset && !loading) return null;
  const preferred = asset?.localizations[detailLanguage];
  const alternateLanguage: ContentLanguage = detailLanguage === "zh-CN" ? "en" : "zh-CN";
  const alternate = asset?.localizations[alternateLanguage];
  const shownName = preferred?.name.trim() || alternate?.name || asset?.name || "";
  const shownDescription = preferred?.description.trim() || alternate?.description || "";
  const shownTags = preferred?.tags.length ? preferred.tags : alternate?.tags || [];
  const shownLicense = preferred?.license.trim() || alternate?.license || "";
  return <aside className="detail-panel">
    {loading || !asset ? <div className="detail-loading">正在加载详情…</div> : <>
      <div className="detail-hero">
        <button className="detail-image-button" onClick={() => asset.images.length && setLightbox(true)} title="查看大图"><ImagePreview imageId={asset.images[selectedImage]?.id || null} alt={shownName} thumbnail={false} className="detail-image" /></button>
        <button className="detail-close" onClick={onClose}><X size={18} /></button>
      </div>
      {asset.images.length > 1 && <div className="thumb-strip">{asset.images.map((image, index) => <button key={image.id} className={index === selectedImage ? "active" : ""} onClick={() => setSelectedImage(index)}><ImagePreview imageId={image.id} alt={image.originalName} className="detail-thumb" /></button>)}</div>}
      <div className="detail-content">
        <div className="detail-language-row"><div className="language-tabs compact"><button className={detailLanguage === "zh-CN" ? "active" : ""} onClick={() => setDetailLanguage("zh-CN")}>中文</button><button className={detailLanguage === "en" ? "active" : ""} onClick={() => setDetailLanguage("en")}>English</button></div>{!preferred?.name.trim() && <span>{detailLanguage === "zh-CN" ? "暂无中文，当前显示英文" : "No English version · 当前显示中文"}</span>}</div>
        <div className="detail-title-row"><div><span className="eyebrow">{asset.categoryName || "未分类"}</span><h2>{shownName}</h2></div><button className={`icon-button ${asset.favorite ? "favorite-active" : ""}`} onClick={onFavorite}><Heart size={19} fill={asset.favorite ? "currentColor" : "none"} /></button></div>
        {shownDescription && <p className="description">{shownDescription}</p>}
        <div className="detail-tags">{shownTags.map(tag => <span key={tag}>#{tag}</span>)}</div>
        <dl className="property-grid">
          <div><dt><FileBox size={14} />软件 / 版本</dt><dd>{[...asset.dccTools, ...asset.versions].join(" · ") || "—"}</dd></div>
          <div><dt><FileBox size={14} />格式 / 大小</dt><dd>{[asset.formats.join(", "), formatBytes(asset.sizeBytes)].filter(v => v && v !== "—").join(" · ") || "—"}</dd></div>
          <div><dt><UserRound size={14} />作者</dt><dd>{asset.author || "—"}</dd></div>
          <div><dt><Calendar size={14} />更新于</dt><dd>{new Date(asset.updatedAt).toLocaleDateString("zh-CN")}</dd></div>
          <div className="wide"><dt><Link2 size={14} />许可</dt><dd>{shownLicense || "未填写"}</dd></div>
        </dl>
        <div className={`link-check-detail ${asset.linkCheckStatus}`}><div><strong>{asset.linkCheckStatus === "valid" ? "网盘链接有效" : asset.linkCheckStatus === "invalid" ? "网盘链接已失效" : asset.linkCheckStatus === "error" ? "暂时无法判断链接状态" : "网盘链接尚未检查"}</strong><span>{asset.linkCheckMessage || "仅验证分享是否存在，不检查提取码。"}</span>{asset.linkCheckedAt && <small>检查于 {new Date(asset.linkCheckedAt).toLocaleString("zh-CN")}</small>}</div><button className="secondary-button" disabled={checkingLink} onClick={onCheckLink}><RefreshCw size={14} className={checkingLink ? "spinning" : ""} />{checkingLink ? "检查中" : "重新检查"}</button></div>
        {asset.sourceUrl && <button className="source-link" onClick={onSourceOpen}>查看素材来源 <ExternalLink size={13} /></button>}
      </div>
      <div className="detail-actions">
        <button className="primary-button grow" onClick={onOpen}><ExternalLink size={17} />打开百度网盘</button>
        {asset.extractionCode && <button className="secondary-button" onClick={onCopy} title="复制提取码"><Copy size={17} /></button>}
        {!!asset.images.length && onAddReference && <HoverDismissDetails className="detail-reference-menu"><summary className="secondary-button" title="加入参考板"><Images size={17} /></summary><div className="menu-popover"><button onClick={() => onAddReference([asset.images[selectedImage].id])}>加入当前图片</button><button onClick={() => onAddReference(asset.images.map(image => image.id))}>加入全部预览图</button></div></HoverDismissDetails>}
        <button className="secondary-button" onClick={onEdit} title="编辑"><Edit3 size={17} /></button>
        <button className="secondary-button danger" onClick={onDelete} title="删除"><Trash2 size={17} /></button>
      </div>
      {lightbox && <ImageLightbox images={asset.images} initialIndex={selectedImage} title={shownName} onClose={() => setLightbox(false)} />}
    </>}
  </aside>;
}
