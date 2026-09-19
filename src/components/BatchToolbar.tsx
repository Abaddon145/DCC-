import { useMemo, useState } from "react";
import { CheckSquare, Images, X } from "lucide-react";
import type { BatchAssetUpdate, Category } from "../types";
import { splitValues } from "../lib/validation";

interface Props { count: number; loadedCount: number; categories: Category[]; onSelectAll: () => void; onClear: () => void; onExit: () => void; onAddToReference: () => void; onApply: (update: Omit<BatchAssetUpdate, "ids" | "contentLanguage">) => Promise<void> }

export function BatchToolbar({ count, loadedCount, categories, onSelectAll, onClear, onExit, onAddToReference, onApply }: Props) {
  const [category, setCategory] = useState("unchanged");
  const [addTags, setAddTags] = useState("");
  const [removeTags, setRemoveTags] = useState("");
  const [favorite, setFavorite] = useState("unchanged");
  const [busy, setBusy] = useState(false);
  const options = useMemo(() => [...categories].sort((a, b) => a.name.localeCompare(b.name, "zh-CN")), [categories]);
  const apply = async () => {
    setBusy(true);
    try {
      await onApply({ categoryId: category.startsWith("set:") ? category.slice(4) : null, clearCategory: category === "clear", addTags: splitValues(addTags), removeTags: splitValues(removeTags), favorite: favorite === "true" ? true : favorite === "false" ? false : null });
    } finally { setBusy(false); }
  };
  return <div className="batch-toolbar">
    <div className="batch-count"><CheckSquare size={17} /><strong>{count}</strong> 项已选择</div>
    <button className="link-button" onClick={onSelectAll}>选择已加载 {loadedCount} 项</button><button className="link-button" onClick={onClear}>清空</button>
    <select value={category} onChange={event => setCategory(event.target.value)}><option value="unchanged">分类不变</option><option value="clear">清空分类</option>{options.map(item => <option key={item.id} value={`set:${item.id}`}>设为：{item.name}</option>)}</select>
    <input value={addTags} onChange={event => setAddTags(event.target.value)} placeholder="添加标签；分隔" />
    <input value={removeTags} onChange={event => setRemoveTags(event.target.value)} placeholder="移除标签；分隔" />
    <select value={favorite} onChange={event => setFavorite(event.target.value)}><option value="unchanged">收藏不变</option><option value="true">设为收藏</option><option value="false">取消收藏</option></select>
    <button className="secondary-button compact" disabled={!count || busy} onClick={onAddToReference}><Images size={14} />加入参考板</button>
    <button className="primary-button compact" disabled={!count || busy} onClick={apply}>{busy ? "处理中…" : "应用"}</button><button className="icon-button subtle" onClick={onExit}><X size={16} /></button>
  </div>;
}
