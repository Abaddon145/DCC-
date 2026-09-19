import { useCallback, useEffect, useState } from "react";
import { Combine, Languages, Search, Tags, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import type { ContentLanguage, TagUsage } from "../types";

interface Props { notify: (message: string, error?: boolean) => void; onChanged: () => Promise<void> }

export function TagManager({ notify, onChanged }: Props) {
  const [locale, setLocale] = useState<ContentLanguage>("zh-CN");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<TagUsage[]>([]);
  const [total, setTotal] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const load = useCallback(async () => { setLoading(true); try { const page = await api.listTags(locale, query, 0, 200); setItems(page.items); setTotal(page.total); setSelected(new Set()); } catch (error) { notify(String(error), true); } finally { setLoading(false); } }, [locale, query, notify]);
  useEffect(() => { const timer = window.setTimeout(() => void load(), 160); return () => clearTimeout(timer); }, [load]);
  const finish = async (message: string) => { notify(message); await Promise.all([load(), onChanged()]); };
  const rename = async (tag: TagUsage) => { const name = window.prompt("新的标签名称", tag.name)?.trim(); if (!name || name === tag.name) return; try { const result = await api.renameTag(tag.id, name); await finish(`标签已重命名，更新 ${result.affectedAssets} 项素材`); } catch (error) { notify(String(error), true); } };
  const merge = async () => {
    if (selected.size < 2) { notify("请至少选择两个同语言标签", true); return; }
    const candidates = items.filter(item => selected.has(item.id));
    const targetName = window.prompt(`输入要保留的目标标签：${candidates.map(item => item.name).join("、")}`, candidates[0].name)?.trim();
    const target = candidates.find(item => item.name.toLowerCase() === targetName?.toLowerCase());
    if (!target) { notify("目标标签必须是已选标签之一", true); return; }
    if (!window.confirm(`把其他 ${candidates.length - 1} 个标签合并到“${target.name}”？`)) return;
    try { const result = await api.mergeTags(candidates.filter(item => item.id !== target.id).map(item => item.id), target.id); await finish(`已合并 ${result.removedTags} 个标签，更新 ${result.affectedAssets} 项素材`); } catch (error) { notify(String(error), true); }
  };
  const remove = async () => { if (!selected.size || !window.confirm(`从素材中删除已选 ${selected.size} 个标签关联？素材本身不会被删除。`)) return; try { const result = await api.deleteTags([...selected]); await finish(`已删除 ${result.removedTags} 个标签，更新 ${result.affectedAssets} 项素材`); } catch (error) { notify(String(error), true); } };
  const clean = async () => { if (!window.confirm(`清理${locale === "zh-CN" ? "中文" : "英文"}未使用标签？`)) return; try { const result = await api.deleteUnusedTags(locale); await finish(`已清理 ${result.removedTags} 个未使用标签`); } catch (error) { notify(String(error), true); } };
  const loadMore = async () => { try { const page = await api.listTags(locale, query, items.length, 200); setItems(previous => [...previous, ...page.items]); } catch (error) { notify(String(error), true); } };

  return <section className="tag-manager">
    <header><div><span className="eyebrow">TAG MANAGER</span><h1>标签管理</h1><p>{loading ? "正在读取…" : `${total} 个${locale === "zh-CN" ? "中文" : "英文"}标签`}</p></div><div className="tag-manager-actions"><button className="secondary-button" disabled={selected.size < 2} onClick={() => void merge()}><Combine size={15} />合并</button><button className="secondary-button danger" disabled={!selected.size} onClick={() => void remove()}><Trash2 size={15} />删除关联</button><button className="secondary-button" onClick={() => void clean()}>清理未使用</button></div></header>
    <div className="tag-toolbar"><div className="content-language-toggle"><button className={locale === "zh-CN" ? "active" : ""} onClick={() => setLocale("zh-CN")}>中文</button><button className={locale === "en" ? "active" : ""} onClick={() => setLocale("en")}><Languages size={12} />English</button></div><label><Search size={15} /><input value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索标签…" /></label></div>
    <div className="tag-table"><div className="tag-table-head"><span /><span>标签名称</span><span>使用次数</span><span>素材数</span><span /></div>{items.map(tag => <div key={tag.id} className={selected.has(tag.id) ? "selected" : ""}><input type="checkbox" checked={selected.has(tag.id)} onChange={() => setSelected(previous => { const next = new Set(previous); next.has(tag.id) ? next.delete(tag.id) : next.add(tag.id); return next; })} /><strong><Tags size={14} />{tag.name}</strong><span>{tag.usageCount}</span><span>{tag.assetCount}</span><button onClick={() => void rename(tag)}>重命名</button></div>)}{!loading && !items.length && <p>没有符合条件的标签</p>}{items.length < total && <button className="tag-load-more" onClick={() => void loadMore()}>加载更多（{items.length}/{total}）</button>}</div>
  </section>;
}
