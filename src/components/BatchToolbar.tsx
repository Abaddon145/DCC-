import { useEffect, useMemo, useRef, useState } from "react";
import { BriefcaseBusiness, CheckSquare, Images, Languages, X } from "lucide-react";
import type { BatchAssetUpdate, Category } from "../types";
import { splitValues } from "../lib/validation";

export type BatchApplyResult = "updated" | "identical";

interface Props { onAi?: () => void; count: number; totalCount: number; selectingAll?: boolean; selectionKey?: string; categories: Category[]; onSelectAll: () => void; onClear: () => void; onExit: () => void; onAddToReference: () => void; onAddToProject?: () => void; onTranslate: () => void; translating?: boolean; onApply: (update: Omit<BatchAssetUpdate, "ids" | "contentLanguage">) => Promise<BatchApplyResult | void> }

function noRequestedChange(category: string, addTags: string, removeTags: string, favorite: string) {
  return category === "unchanged" && favorite === "unchanged" && splitValues(addTags).length === 0 && splitValues(removeTags).length === 0;
}

export function BatchToolbar({ onAi, count, totalCount, selectingAll = false, selectionKey = "", categories, onSelectAll, onClear, onExit, onAddToReference, onAddToProject = () => undefined, onTranslate, translating = false, onApply }: Props) {
  const [category, setCategory] = useState("unchanged");
  const [addTags, setAddTags] = useState("");
  const [removeTags, setRemoveTags] = useState("");
  const [favorite, setFavorite] = useState("unchanged");
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<BatchApplyResult | "error" | null>(null);
  const [errorText, setErrorText] = useState("");
  const options = useMemo(() => [...categories].sort((a, b) => a.name.localeCompare(b.name, "zh-CN")), [categories]);
  const noChange = count > 0 && noRequestedChange(category, addTags, removeTags, favorite);
  const identical = outcome === "identical" && !noChange;
  const applyBlocked = !count || busy || noChange || identical;
  const reason = noChange ? "没有要应用的更改" : identical ? "所选操作与当前值相同，不会更新记录" : outcome === "error" ? errorText : "";
  const live = useRef({ category, addTags, removeTags, favorite, selectionKey, count });
  live.current = { category, addTags, removeTags, favorite, selectionKey, count };
  useEffect(() => { setOutcome(null); setErrorText(""); }, [category, addTags, removeTags, favorite, count, selectionKey]);
  const apply = async () => {
    if (applyBlocked) return;
    const submitted = live.current;
    const submittedKey = JSON.stringify(submitted);
    setBusy(true);
    setErrorText("");
    try {
      const result = await onApply({ categoryId: submitted.category.startsWith("set:") ? submitted.category.slice(4) : null, clearCategory: submitted.category === "clear", addTags: splitValues(submitted.addTags), removeTags: splitValues(submitted.removeTags), favorite: submitted.favorite === "true" ? true : submitted.favorite === "false" ? false : null });
      if (JSON.stringify(live.current) !== submittedKey) return;
      setOutcome(result === "identical" ? "identical" : "updated");
    } catch (error) {
      if (JSON.stringify(live.current) !== submittedKey) return;
      setOutcome("error");
      setErrorText(error instanceof Error ? error.message : String(error));
    } finally { setBusy(false); }
  };
  return <div className="batch-toolbar">
    {onAi && <button className="secondary-button compact" disabled={!count || busy} onClick={onAi}>AI 整理</button>}
    <div className="batch-count"><CheckSquare size={17} /><strong>{count}</strong> 项已选择</div>
    <button className="link-button" disabled={selectingAll} onClick={onSelectAll}>{selectingAll ? "正在全选…" : `全选当前结果 ${totalCount} 项`}</button><button className="link-button" onClick={onClear}>清空</button>
    <select aria-label="批量分类" value={category} onChange={event => setCategory(event.target.value)}><option value="unchanged">分类不变</option><option value="clear">清空分类</option>{options.map(item => <option key={item.id} value={`set:${item.id}`}>设为：{item.name}</option>)}</select>
    <input aria-label="添加标签" value={addTags} onChange={event => setAddTags(event.target.value)} placeholder="添加标签；分隔" />
    <input aria-label="移除标签" value={removeTags} onChange={event => setRemoveTags(event.target.value)} placeholder="移除标签；分隔" />
    <select aria-label="批量收藏" value={favorite} onChange={event => setFavorite(event.target.value)}><option value="unchanged">收藏不变</option><option value="true">设为收藏</option><option value="false">取消收藏</option></select>
    <button className="secondary-button compact" disabled={!count || busy} onClick={onAddToReference}><Images size={14} />加入参考板</button>
    <button className="secondary-button compact" disabled={!count || busy} onClick={onAddToProject}><BriefcaseBusiness size={14} />加入项目</button>
    <button className="secondary-button compact" disabled={!count || busy || translating} onClick={onTranslate}><Languages size={14} />{translating ? "正在补译…" : "补齐双语"}</button>
    <button className="primary-button compact" disabled={applyBlocked} aria-describedby={reason ? "batch-apply-reason" : undefined} onClick={() => void apply()}>{busy ? "处理中…" : "应用"}</button>
    {reason && <p id="batch-apply-reason" className={outcome === "error" && !noChange && !identical ? "batch-apply-note error" : "batch-apply-note"} role={outcome === "error" && !noChange && !identical ? "alert" : "status"}>{reason}</p>}
    <button className="icon-button subtle" onClick={onExit}><X size={16} /></button>
  </div>;
}
