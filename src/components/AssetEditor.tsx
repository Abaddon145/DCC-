import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowDown, ArrowUp, ClipboardPaste, ImagePlus, Languages, Save, Sparkles, Star, Trash2, X } from "lucide-react";
import type { AssetDetail, AssetInput, Category, ContentLanguage, FabDuplicateMatch, ImageInput, LocalizedAssetText, ParsedShareText, TranslationPreview } from "../types";
import { formatBytes, parseSize, splitValues, validateAsset } from "../lib/validation";
import { ImagePreview } from "./ImagePreview";
import { api } from "../lib/api";
import { applyFabMetadata, isFabUrl } from "../lib/fab";

interface Props { asset: AssetDetail | null; categories: Category[]; contentLanguage: ContentLanguage; initialShare?: ParsedShareText | null; saveShortcut?: string; onClose: () => void; onSave: (input: AssetInput) => Promise<void>; onOpenExisting?: (id: string) => void }

const blankLocalized = (): LocalizedAssetText => ({ name: "", description: "", tags: [], license: "" });

const empty: AssetInput = {
  name: "", description: "", categoryId: null, tags: [], dccTools: ["Unreal Engine"], versions: [], formats: [],
  sizeBytes: null, author: "", sourceUrl: "", fabListingId: null, autoCategoryPath: [], license: "", shareUrl: "", extractionCode: "", favorite: false, images: [],
  localizations: { "zh-CN": blankLocalized(), en: blankLocalized() }, contentLanguage: "zh-CN"
};

function toInput(asset: AssetDetail | null, contentLanguage: ContentLanguage, initialShare?: ParsedShareText | null): AssetInput {
  if (!asset) return { ...empty, tags: [], dccTools: ["Unreal Engine"], versions: [], formats: [], images: [], localizations: { "zh-CN": blankLocalized(), en: blankLocalized() }, contentLanguage, shareUrl: initialShare?.shareUrl || "", extractionCode: initialShare?.extractionCode || "" };
  return {
    id: asset.id, name: asset.name, description: asset.description, categoryId: asset.categoryId, tags: asset.tags,
    dccTools: asset.dccTools, versions: asset.versions, formats: asset.formats, sizeBytes: asset.sizeBytes,
    author: asset.author, sourceUrl: asset.sourceUrl, license: asset.license, shareUrl: asset.shareUrl,
    fabListingId: asset.fabListingId, autoCategoryPath: [],
    extractionCode: asset.extractionCode, favorite: asset.favorite,
    images: asset.images.map(image => ({ id: image.id, originalName: image.originalName, isCover: image.isCover, sortOrder: image.sortOrder })),
    localizations: { "zh-CN": asset.localizations["zh-CN"] || blankLocalized(), en: asset.localizations.en || blankLocalized() }, contentLanguage
  };
}

function flattenCategories(categories: Category[]) {
  const children = new Map<string | null, Category[]>();
  categories.forEach(category => children.set(category.parentId, [...(children.get(category.parentId) || []), category]));
  const result: { id: string; label: string }[] = [];
  const walk = (parent: string | null, depth: number) => (children.get(parent) || []).sort((a, b) => a.name.localeCompare(b.name, "zh-CN")).forEach(item => {
    result.push({ id: item.id, label: `${"　".repeat(depth)}${depth ? "└ " : ""}${item.name}` }); walk(item.id, depth + 1);
  });
  walk(null, 0); return result;
}

export function AssetEditor({ asset, categories, contentLanguage, initialShare, saveShortcut = "Ctrl+S", onClose, onSave, onOpenExisting }: Props) {
  const [form, setForm] = useState<AssetInput>(() => toInput(asset, contentLanguage, initialShare));
  const [editLanguage, setEditLanguage] = useState<ContentLanguage>(contentLanguage);
  const [sizeText, setSizeText] = useState(asset?.sizeBytes ? formatBytes(asset.sizeBytes) : "");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [warnings, setWarnings] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [fabUrl, setFabUrl] = useState(() => isFabUrl(asset?.sourceUrl || "") ? asset!.sourceUrl : "");
  const [fetchingFab, setFetchingFab] = useState(false);
  const [fabStatus, setFabStatus] = useState<{ kind: "success" | "error"; message: string } | null>(null);
  const [fabDuplicate, setFabDuplicate] = useState<FabDuplicateMatch | null>(null);
  const [translating, setTranslating] = useState(false);
  const [translation, setTranslation] = useState<{ target: ContentLanguage; preview: TranslationPreview; selected: Set<keyof LocalizedAssetText>; characterCount?: number } | null>(null);
  const formRef = useRef<HTMLFormElement>(null);
  const categoryOptions = useMemo(() => flattenCategories(categories), [categories]);

  const addPaths = (paths: string[]) => setForm(prev => {
    const imagePaths = paths.filter(path => /\.(png|jpe?g|webp|bmp|gif|tiff?)$/i.test(path));
    const incoming: ImageInput[] = imagePaths.map((sourcePath, index) => ({ sourcePath, sortOrder: prev.images.length + index, isCover: prev.images.length === 0 && index === 0 }));
    return { ...prev, images: [...prev.images, ...incoming] };
  });

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebviewWindow().onDragDropEvent(event => {
      if (event.payload.type === "drop") addPaths(event.payload.paths);
    }).then(fn => { unlisten = fn; }).catch(() => undefined);
    return () => unlisten?.();
  }, []);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      const pressed = [event.ctrlKey || event.metaKey ? "Ctrl" : "", event.altKey ? "Alt" : "", event.shiftKey ? "Shift" : "", !["Control","Shift","Alt","Meta"].includes(event.key) ? (event.key.length === 1 ? event.key.toUpperCase() : event.key) : ""].filter(Boolean).join("+");
      if (pressed.toLowerCase() === saveShortcut.toLowerCase()) { event.preventDefault(); formRef.current?.requestSubmit(); }
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [saveShortcut]);

  const chooseImages = async () => {
    const result = await open({ multiple: true, filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"] }] });
    if (result) addPaths(Array.isArray(result) ? result : [result]);
  };

  const update = <K extends keyof AssetInput>(key: K, value: AssetInput[K]) => setForm(prev => ({ ...prev, [key]: value }));
  const localized = form.localizations[editLanguage] || blankLocalized();
  const updateLocalized = <K extends keyof LocalizedAssetText>(key: K, value: LocalizedAssetText[K]) => setForm(previous => ({ ...previous, localizations: { ...previous.localizations, [editLanguage]: { ...(previous.localizations[editLanguage] || blankLocalized()), [key]: value } } }));

  const requestTranslation = async (sourceLanguage: ContentLanguage) => {
    const target: ContentLanguage = sourceLanguage === "en" ? "zh-CN" : "en";
    const source = form.localizations[sourceLanguage] || blankLocalized();
    if (![source.name, source.description, source.license, ...source.tags].some(value => value.trim())) { setFabStatus({ kind: "error", message: "源语言没有可翻译的内容" }); return; }
    setTranslating(true);
    try {
      const preview = await api.translateAssetFields({ sourceLanguage, targetLanguage: target, fields: source });
      const existing = form.localizations[target] || blankLocalized();
      const selected = new Set<keyof LocalizedAssetText>();
      (["name", "description", "tags", "license"] as const).forEach(key => { const emptyTarget = Array.isArray(existing[key]) ? !(existing[key] as string[]).length : !(existing[key] as string).trim(); if (emptyTarget) selected.add(key); });
      setTranslation({ target, preview, selected });
    } catch (error) { setFabStatus({ kind: "error", message: String(error) }); }
    finally { setTranslating(false); }
  };

  const applyTranslation = () => {
    if (!translation) return;
    setForm(previous => {
      const existing = previous.localizations[translation.target] || blankLocalized();
      const next = { ...existing };
      translation.selected.forEach(key => { (next as unknown as Record<string, unknown>)[key] = translation.preview.fields[key]; });
      return { ...previous, localizations: { ...previous.localizations, [translation.target]: next } };
    });
    setEditLanguage(translation.target); setTranslation(null);
  };
  const moveImage = (index: number, delta: number) => setForm(prev => {
    const images = [...prev.images]; const next = index + delta;
    if (next < 0 || next >= images.length) return prev;
    [images[index], images[next]] = [images[next], images[index]];
    return { ...prev, images: images.map((image, i) => ({ ...image, sortOrder: i })) };
  });
  const removeImage = (index: number) => setForm(prev => {
    const images = prev.images.filter((_, i) => i !== index).map((image, i) => ({ ...image, sortOrder: i }));
    if (images.length && !images.some(image => image.isCover)) images[0].isCover = true;
    return { ...prev, images };
  });
  const setCover = (index: number) => setForm(prev => ({ ...prev, images: prev.images.map((image, i) => ({ ...image, isCover: i === index })) }));

  const parseClipboard = async () => {
    try {
      const parsed = await api.parseShareText(await api.readClipboard());
      setForm(prev => ({ ...prev, shareUrl: prev.shareUrl || parsed.shareUrl, extractionCode: prev.extractionCode || parsed.extractionCode }));
      setWarnings(["已从剪贴板识别链接和提取码；已有内容未被覆盖"]);
    } catch (error) { setWarnings([String(error)]); }
  };

  const fetchFab = async () => {
    if (!fabUrl.trim() || fetchingFab) return;
    setFetchingFab(true); setFabStatus(null); setFabDuplicate(null);
    try {
      const duplicate = await api.checkFabUrl(fabUrl.trim(), asset?.id);
      if (duplicate) {
        setFabDuplicate(duplicate);
        setFabStatus({ kind: "error", message: duplicate.location === "trash" ? `“${duplicate.assetName}”已在回收站，请先恢复或永久删除后再录入。` : `“${duplicate.assetName}”已存在于${duplicate.categoryPath ? `“${duplicate.categoryPath}”` : "素材库"}，不会重复录入。` });
        return;
      }
      const metadata = await api.fetchFabMetadata(fabUrl.trim());
      let next = applyFabMetadata(form, metadata);
      setForm(next);
      const settings = await api.getTranslationSettings();
      let translationNote = "";
      if (settings.fabAutoTranslate && settings.configured) {
        try {
          const preview = await api.translateAssetFields({ sourceLanguage: "en", targetLanguage: "zh-CN", fields: next.localizations.en || blankLocalized() });
          const existing = next.localizations["zh-CN"] || blankLocalized();
          const merged = { name: existing.name || preview.fields.name, description: existing.description || preview.fields.description, license: existing.license || preview.fields.license, tags: existing.tags.length ? existing.tags : preview.fields.tags };
          next = { ...next, localizations: { ...next.localizations, "zh-CN": merged } };
          setForm(next);
          translationNote = preview.warnings.length ? ` 部分翻译失败：${preview.warnings.join("；")}` : " 已自动补齐中文版本。";
        } catch (error) { translationNote = ` 在线翻译未完成：${String(error)}`; }
      } else if (settings.fabAutoTranslate && !settings.configured) translationNote = " 尚未配置百度翻译，已跳过中文翻译。";
      setForm(next);
      setFabUrl(metadata.canonicalUrl);
      const details = [metadata.author && "作者", metadata.tags.length && "标签", metadata.dccTools.length && "DCC", metadata.versions.length && "版本", metadata.formats.length && "格式", metadata.license && "许可", metadata.previewImages.length && `${metadata.previewImages.length} 张预览图`].filter(Boolean).join("、");
      const categoryNote = !form.categoryId && metadata.suggestedCategoryPath.length ? ` 保存时将自动归类到“${metadata.suggestedCategoryPath.join(" / ")}”。` : "";
      setFabStatus({ kind: "success", message: `已读取“${metadata.name}”${details ? `，并补充${details}` : ""}。已有文字和图片不会被覆盖。${categoryNote}${metadata.imageWarning ? ` ${metadata.imageWarning}` : ""}${translationNote}` });
    } catch (error) {
      setFabStatus({ kind: "error", message: String(error) });
    } finally { setFetchingFab(false); }
  };

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const parsedSize = parseSize(sizeText);
    const display = form.localizations[form.contentLanguage] || form.localizations[form.contentLanguage === "en" ? "zh-CN" : "en"] || blankLocalized();
    const next = { ...form, name: display.name, description: display.description, tags: display.tags, license: display.license, sizeBytes: Number.isNaN(parsedSize) ? -1 : parsedSize };
    const validation = validateAsset(next);
    setErrors(validation.errors); setWarnings(validation.warnings);
    if (Object.keys(validation.errors).length) return;
    setSaving(true);
    try {
      const duplicate = next.shareUrl.trim() ? await api.checkShareUrl(next.shareUrl) : null;
      if (duplicate && duplicate.id !== asset?.id) {
        setErrors(current => ({ ...current, shareUrl: `该链接已属于“${duplicate.name}”` }));
        return;
      }
      const payload = {
        ...next,
        images: next.images.map(({ previewDataUrl: _previewDataUrl, remoteUrl: _remoteUrl, ...image }) => image)
      };
      await onSave(payload);
    } finally { setSaving(false); }
  };

  return <div className="modal-backdrop"><section className="editor-modal" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">{asset ? "编辑条目" : "录入素材"}</span><h2>{asset?.name || "添加新素材"}</h2></div><button className="icon-button" onClick={onClose}><X size={19} /></button></header>
    <form ref={formRef} onSubmit={submit} className="editor-form">
      <div className="form-section"><h3>基本信息</h3>
        <div className="fab-import-panel">
          <div className="fab-import-copy"><Sparkles size={17} /><div><strong>从 Fab 自动填充</strong><span>读取名称、描述、作者、标签、许可、格式和 UE 版本</span></div></div>
          <div className="fab-import-controls"><input value={fabUrl} onChange={event => { setFabUrl(event.target.value); setFabDuplicate(null); setFabStatus(null); }} onKeyDown={event => { if (event.key === "Enter") { event.preventDefault(); void fetchFab(); } }} placeholder="https://www.fab.com/listings/…" /><button type="button" className="secondary-button" disabled={!fabUrl.trim() || fetchingFab} onClick={fetchFab}>{fetchingFab ? "正在读取…" : "读取并填充"}</button></div>
          {fabStatus && <div className={`fab-import-status ${fabStatus.kind}`}>{fabStatus.message}{fabDuplicate?.location === "library" && onOpenExisting && <button type="button" className="secondary-button" onClick={() => onOpenExisting(fabDuplicate.assetId)}>打开现有素材</button>}</div>}
        </div>
        <div className="language-editor-bar"><div className="language-tabs"><button type="button" className={editLanguage === "zh-CN" ? "active" : ""} onClick={() => setEditLanguage("zh-CN")}>中文</button><button type="button" className={editLanguage === "en" ? "active" : ""} onClick={() => setEditLanguage("en")}>English</button></div><button type="button" className="secondary-button" disabled={translating} onClick={() => void requestTranslation(editLanguage)}><Languages size={15} />{translating ? "正在翻译…" : editLanguage === "en" ? "翻译到中文" : "Translate to English"}</button></div>
        {translation && <div className="translation-preview"><div><strong>翻译预览 · {translation.characterCount ?? translation.preview.characterCount} 字符</strong><span>勾选需要写入 {translation.target === "zh-CN" ? "中文" : "English"} 版本的字段；已有内容默认不覆盖。</span></div>{translation.preview.appliedTerms.length > 0 && <div className="translation-term-summary"><strong>术语库已校正 {translation.preview.appliedTerms.reduce((sum, item) => sum + item.count, 0)} 处</strong><span>{translation.preview.appliedTerms.map(item => `${item.source} → ${item.target}${item.count > 1 ? ` ×${item.count}` : ""}`).join("；")}</span></div>}{(["name", "description", "tags", "license"] as const).map(key => <label key={key}><input type="checkbox" checked={translation.selected.has(key)} onChange={event => setTranslation(current => { if (!current) return current; const selected = new Set(current.selected); event.target.checked ? selected.add(key) : selected.delete(key); return { ...current, selected }; })} /><span>{({ name: "名称", description: "描述", tags: "标签", license: "许可" } as const)[key]}</span><em>{Array.isArray(translation.preview.fields[key]) ? (translation.preview.fields[key] as string[]).join("；") : translation.preview.fields[key] as string || "—"}</em></label>)}{translation.preview.warnings.map(item => <small key={item}>{item}</small>)}<div className="translation-actions"><button type="button" className="secondary-button" onClick={() => setTranslation(null)}>取消</button><button type="button" className="primary-button" disabled={!translation.selected.size} onClick={applyTranslation}>应用所选译文</button></div></div>}
        <div className="form-grid">
        <label className="wide"><span>{editLanguage === "zh-CN" ? "中文名称" : "English name"} *</span><input autoFocus value={localized.name} onChange={e => updateLocalized("name", e.target.value)} placeholder={editLanguage === "zh-CN" ? "例如：中世纪古堡环境包" : "e.g. Medieval Castle Environment"} />{errors.name && <small className="field-error">{errors.name}</small>}</label>
        <label><span>分类</span><select value={form.categoryId || ""} onChange={e => setForm(previous => ({ ...previous, categoryId: e.target.value || null, autoCategoryPath: e.target.value ? [] : previous.autoCategoryPath }))}><option value="">未分类</option>{categoryOptions.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}</select>{!form.categoryId && Boolean(form.autoCategoryPath?.length) && <small className="field-hint">保存时自动归类：{form.autoCategoryPath?.join(" / ")}</small>}</label>
        <label><span>作者 / 工作室</span><input value={form.author} onChange={e => update("author", e.target.value)} /></label>
        <label className="wide"><span>{editLanguage === "zh-CN" ? "中文描述" : "English description"}</span><textarea rows={4} value={localized.description} onChange={e => updateLocalized("description", e.target.value)} placeholder="适用场景、内容构成、注意事项…" /></label>
        <label><span>{editLanguage === "zh-CN" ? "中文标签" : "English tags"}（分号分隔）</span><input value={localized.tags.join("; ")} onChange={e => updateLocalized("tags", splitValues(e.target.value))} placeholder="写实; 建筑; Nanite" /></label>
        <label><span>DCC 软件</span><input value={form.dccTools.join("; ")} onChange={e => update("dccTools", splitValues(e.target.value))} /></label>
        <label><span>版本</span><input value={form.versions.join("; ")} onChange={e => update("versions", splitValues(e.target.value))} placeholder="5.3; 5.4; 5.5" /></label>
        <label><span>格式</span><input value={form.formats.join("; ")} onChange={e => update("formats", splitValues(e.target.value))} placeholder="uasset; FBX" /></label>
        <label><span>素材大小</span><input value={sizeText} onChange={e => setSizeText(e.target.value)} placeholder="例如 2.4 GB" />{errors.sizeBytes && <small className="field-error">{errors.sizeBytes}</small>}</label>
        <label><span>{editLanguage === "zh-CN" ? "中文许可" : "English license"}</span><input value={localized.license} onChange={e => updateLocalized("license", e.target.value)} placeholder="个人/商用、CC0…" /></label>
        <label className="wide"><span>来源地址</span><input value={form.sourceUrl} onChange={e => update("sourceUrl", e.target.value)} placeholder="https://…" />{errors.sourceUrl && <small className="field-error">{errors.sourceUrl}</small>}</label>
      </div></div>
      <div className="form-section"><div className="section-heading"><h3>百度网盘</h3><button type="button" className="secondary-button" onClick={parseClipboard}><ClipboardPaste size={15} />从剪贴板解析</button></div><div className="form-grid">
        <label className="wide"><span>分享链接（可选）</span><input value={form.shareUrl} onChange={e => update("shareUrl", e.target.value)} placeholder="https://pan.baidu.com/s/…" />{errors.shareUrl && <small className="field-error">{errors.shareUrl}</small>}</label>
        <label><span>提取码</span><input value={form.extractionCode} onChange={e => update("extractionCode", e.target.value.trim())} maxLength={32} /></label>
        <label className="checkbox-label"><input type="checkbox" checked={form.favorite} onChange={e => update("favorite", e.target.checked)} />加入收藏</label>
        {warnings.map(warning => <div key={warning} className="form-warning wide">{warning}</div>)}
      </div></div>
      <div className="form-section"><div className="section-heading"><h3>预览图片</h3><button type="button" className="secondary-button" onClick={chooseImages}><ImagePlus size={16} />选择图片</button></div>
        <p className="section-help">可多选或拖入图片。软件会复制原图并生成缩略图。</p>
        <div className="image-editor-list">{form.images.map((image, index) => <div className={`image-editor-item ${image.isCover ? "cover" : ""}`} key={image.id || image.sourcePath || index}>
          {image.id ? <ImagePreview imageId={image.id} alt={`预览 ${index + 1}`} className="editor-thumb" /> : image.previewDataUrl ? <img src={image.previewDataUrl} alt={image.originalName || `Fab 预览 ${index + 1}`} className="editor-thumb" /> : <div className="new-image-name">{image.originalName || image.sourcePath?.split(/[\\/]/).at(-1)}</div>}
          <button type="button" className="cover-toggle" onClick={() => setCover(index)}><Star size={13} fill={image.isCover ? "currentColor" : "none"} />{image.isCover ? "封面" : "设为封面"}</button>
          <div className="image-order"><button type="button" onClick={() => moveImage(index, -1)} disabled={index === 0}><ArrowUp size={13} /></button><button type="button" onClick={() => moveImage(index, 1)} disabled={index === form.images.length - 1}><ArrowDown size={13} /></button><button type="button" onClick={() => removeImage(index)}><Trash2 size={13} /></button></div>
        </div>)}</div>
      </div>
      <footer className="modal-footer"><button type="button" className="secondary-button" onClick={onClose}>取消</button><button type="submit" className="primary-button" disabled={saving}><Save size={17} />{saving ? "正在保存…" : "保存素材"}</button></footer>
    </form>
  </section></div>;
}
