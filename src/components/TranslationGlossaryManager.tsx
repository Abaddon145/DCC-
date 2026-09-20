import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Download, Pencil, Plus, RotateCcw, Search, Trash2, Upload, X } from "lucide-react";
import { api } from "../lib/api";
import type { ContentLanguage, TranslationTerm, TranslationTermInput, TranslationTermMode } from "../types";

interface Props { notify: (message: string, error?: boolean) => void }

const blankTerm = (): TranslationTermInput => ({
  sourceLanguage: "en", targetLanguage: "zh-CN", source: "", target: "", mode: "translate",
  caseSensitive: false, enabled: true,
});

export function TranslationGlossaryManager({ notify }: Props) {
  const [terms, setTerms] = useState<TranslationTerm[]>([]);
  const [query, setQuery] = useState("");
  const [direction, setDirection] = useState("en:zh-CN");
  const [origin, setOrigin] = useState<"all" | "builtin" | "custom">("all");
  const [enabled, setEnabled] = useState<"all" | "enabled" | "disabled">("all");
  const [editing, setEditing] = useState<TranslationTermInput | null>(null);
  const [busy, setBusy] = useState(false);
  const [sourceLanguage, targetLanguage] = direction.split(":") as [ContentLanguage, ContentLanguage];

  const load = async () => {
    try {
      setTerms(await api.listTranslationTerms({
        query: query.trim() || undefined,
        sourceLanguage,
        targetLanguage,
        origin: origin === "all" ? undefined : origin,
        enabled: enabled === "all" ? undefined : enabled === "enabled",
      }));
    } catch (error) { notify(`读取术语库失败：${String(error)}`, true); }
  };
  useEffect(() => { void load(); }, [query, direction, origin, enabled]);

  const counts = useMemo(() => ({ builtin: terms.filter(term => term.origin === "builtin").length, custom: terms.filter(term => term.origin === "custom").length }), [terms]);
  const edit = (term?: TranslationTerm) => setEditing(term ? {
    id: term.origin === "custom" ? term.id : null,
    sourceLanguage: term.sourceLanguage, targetLanguage: term.targetLanguage,
    source: term.source, target: term.target, mode: term.mode,
    caseSensitive: term.caseSensitive, enabled: true,
  } : { ...blankTerm(), sourceLanguage, targetLanguage });
  const saveTerm = async () => {
    if (!editing?.source.trim() || (editing.mode === "translate" && !editing.target.trim())) { notify("请填写原词和目标词", true); return; }
    setBusy(true);
    try { await api.upsertTranslationTerm(editing); setEditing(null); await load(); notify("专业术语已保存"); }
    catch (error) { notify(String(error), true); } finally { setBusy(false); }
  };
  const toggle = async (term: TranslationTerm) => {
    try { await api.setTranslationTermEnabled(term.id, !term.enabled); await load(); }
    catch (error) { notify(String(error), true); }
  };
  const remove = async (term: TranslationTerm) => {
    if (!window.confirm(`删除自定义术语“${term.source}”？`)) return;
    try { await api.deleteTranslationTerm(term.id); await load(); notify("自定义术语已删除"); }
    catch (error) { notify(String(error), true); }
  };
  const reset = async () => {
    if (!window.confirm("清除全部自定义术语并恢复所有内置术语？素材内容不会改变。")) return;
    try { await api.resetTranslationTerms(); setEditing(null); await load(); notify("专业术语库已恢复默认"); }
    catch (error) { notify(String(error), true); }
  };
  const importTerms = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Excel 术语表", extensions: ["xlsx"] }] });
    if (typeof path !== "string") return;
    try {
      const report = await api.importTranslationTerms(path); await load();
      notify(`术语导入完成：新增 ${report.imported}，更新 ${report.updated}，跳过 ${report.skipped}${report.warnings.length ? `；${report.warnings[0]}` : ""}`);
    } catch (error) { notify(String(error), true); }
  };
  const exportTerms = async () => {
    const path = await save({ defaultPath: "栈藏-专业术语库.xlsx", filters: [{ name: "Excel 术语表", extensions: ["xlsx"] }] });
    if (!path) return;
    try { await api.exportTranslationTerms(path); notify("专业术语库已导出"); }
    catch (error) { notify(String(error), true); }
  };

  return <div className="glossary-manager">
    <div className="glossary-heading"><div><strong>专业术语库</strong><span>翻译前锁定专业词，所有素材库共用 · 当前显示 {terms.length} 项（内置 {counts.builtin} / 自定义 {counts.custom}）</span></div><button className="primary-button compact" onClick={() => edit()}><Plus size={14} />新增术语</button></div>
    <div className="glossary-filters">
      <label className="glossary-search"><Search size={14} /><input aria-label="搜索专业术语" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索原词或译名" /></label>
      <select aria-label="术语方向" value={direction} onChange={event => setDirection(event.target.value)}><option value="en:zh-CN">English → 中文</option><option value="zh-CN:en">中文 → English</option></select>
      <select aria-label="术语来源" value={origin} onChange={event => setOrigin(event.target.value as typeof origin)}><option value="all">全部来源</option><option value="builtin">内置</option><option value="custom">自定义</option></select>
      <select aria-label="术语状态" value={enabled} onChange={event => setEnabled(event.target.value as typeof enabled)}><option value="all">全部状态</option><option value="enabled">已启用</option><option value="disabled">已停用</option></select>
    </div>
    <div className="glossary-list">{terms.length ? terms.map(term => <div key={term.id} className={!term.enabled ? "disabled" : ""}>
      <button className={`term-toggle ${term.enabled ? "active" : ""}`} onClick={() => void toggle(term)} title={term.enabled ? "停用" : "启用"}><span /></button>
      <div className="term-copy"><strong>{term.source}</strong><span>{term.mode === "preserve" ? "保持原文" : term.target}</span></div>
      <small>{term.origin === "builtin" ? "内置" : "自定义"} · {term.mode === "preserve" ? "保护" : "强制译名"}</small>
      <button className="icon-button subtle" title={term.origin === "builtin" ? "创建自定义覆盖" : "编辑"} onClick={() => edit(term)}><Pencil size={14} /></button>
      {term.origin === "custom" && <button className="icon-button subtle danger" title="删除" onClick={() => void remove(term)}><Trash2 size={14} /></button>}
    </div>) : <p className="empty-mini">没有符合条件的术语</p>}</div>
    <div className="glossary-actions"><button className="secondary-button compact" onClick={() => void importTerms()}><Upload size={14} />导入 Excel</button><button className="secondary-button compact" onClick={() => void exportTerms()}><Download size={14} />导出 Excel</button><button className="secondary-button compact" onClick={() => void reset()}><RotateCcw size={14} />恢复默认</button></div>
    {editing && <div className="glossary-editor"><div className="glossary-editor-title"><strong>{editing.id ? "编辑自定义术语" : "新增自定义术语"}</strong><button className="icon-button subtle" onClick={() => setEditing(null)}><X size={14} /></button></div><div className="glossary-editor-grid">
      <label><span>原词</span><input autoFocus value={editing.source} onChange={event => setEditing(current => current && ({ ...current, source: event.target.value }))} /></label>
      <label><span>目标词</span><input disabled={editing.mode === "preserve"} value={editing.mode === "preserve" ? editing.source : editing.target} onChange={event => setEditing(current => current && ({ ...current, target: event.target.value }))} /></label>
      <label><span>处理方式</span><select value={editing.mode} onChange={event => setEditing(current => current && ({ ...current, mode: event.target.value as TranslationTermMode }))}><option value="translate">强制译名</option><option value="preserve">保持原文</option></select></label>
      <label className="checkbox-label"><input type="checkbox" checked={editing.caseSensitive} onChange={event => setEditing(current => current && ({ ...current, caseSensitive: event.target.checked }))} />区分英文大小写</label>
    </div><div className="translation-actions"><button className="secondary-button" onClick={() => setEditing(null)}>取消</button><button className="primary-button" disabled={busy} onClick={() => void saveTerm()}>{busy ? "保存中…" : "保存术语"}</button></div></div>}
  </div>;
}
