import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CheckCircle2, Download, FileSpreadsheet, Upload, X } from "lucide-react";
import { api } from "../lib/api";
import { listen } from "@tauri-apps/api/event";
import type { BatchTranslationRow, ImportMapping, ImportPreview, ImportProgress, ImportReport } from "../types";
import { useFabTranslationSettings } from "../lib/bilingual";
import { ImportTaskMap } from "./ImportTaskMap";
import { useRequestGate } from "../lib/workspace";

interface Props { onClose: () => void; onImported: () => void; notify: (message: string, error?: boolean) => void; expectedLibraryId?: string }

const simpleFields: { key: string; label: string; required?: boolean }[] = [
  { key: "fab_url", label: "Fab URL", required: true }, { key: "baidu_url", label: "百度网盘链接 / 分享文本" }, { key: "ue_versions", label: "UE 版本（分号分隔）" }
];
const legacyFields: { key: string; label: string; required?: boolean }[] = [
  { key: "name_zh", label: "中文名称（至少一种）" }, { key: "name_en", label: "英文名称（至少一种）" }, { key: "name", label: "旧版素材名称" }, { key: "share_url", label: "分享链接（可选）" },
  { key: "extraction_code", label: "提取码" }, { key: "description_zh", label: "中文描述" }, { key: "description_en", label: "英文描述" }, { key: "description", label: "旧版描述" }, { key: "category", label: "分类路径" },
  { key: "tags_zh", label: "中文标签" }, { key: "tags_en", label: "英文标签" }, { key: "tags", label: "旧版标签" }, { key: "dcc_tools", label: "DCC 软件" }, { key: "versions", label: "版本" },
  { key: "formats", label: "格式" }, { key: "size", label: "大小" }, { key: "author", label: "作者" },
  { key: "source_url", label: "来源地址" }, { key: "license_zh", label: "中文许可" }, { key: "license_en", label: "英文许可" }, { key: "license", label: "旧版许可" }, { key: "preview_paths", label: "预览图路径" }
];

export function ImportDialog({ onClose, onImported, notify, expectedLibraryId }: Props) {
  const gate = useRequestGate();
  const policy = useFabTranslationSettings();
  const working = useRef(false);
  const [operationError, setOperationError] = useState("");
  const [page, setPage] = useState(0);
  const [path, setPath] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [mapping, setMapping] = useState<ImportMapping>({});
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const translationSettings = policy.settings;
  const autoTranslate = translationSettings?.fabAutoTranslate ?? false;
  const close = () => { gate.invalidate(); onClose(); };
  const binding: [string?] = expectedLibraryId ? [expectedLibraryId] : [];

  const readPreview = async (selectedPath: string, nextMapping: ImportMapping, ticket: number) => {
    const result = await api.inspectImport(selectedPath, nextMapping);
    if (!gate.isCurrent(ticket)) return;
    setPreview(result); setMapping(Object.keys(nextMapping).length ? nextMapping : result.suggestedMapping); setPage(0);
  };

  const inspect = async (selectedPath: string, nextMapping: ImportMapping = {}) => {
    if (working.current) return;
    working.current = true; const ticket = gate.begin(); setBusy(true); setOperationError("");
    try {
      await readPreview(selectedPath, nextMapping, ticket);
    } catch (error) { if (gate.isCurrent(ticket)) { setPreview(null); setOperationError(String(error)); } }
    finally { if (gate.isCurrent(ticket)) { working.current = false; setBusy(false); } }
  };
  const choose = async () => {
    if (working.current) return;
    working.current = true; const ticket = gate.begin(); setBusy(true); setOperationError("");
    try {
      const result = await open({ multiple: false, filters: [{ name: "表格", extensions: ["xlsx", "csv"] }] });
      if (typeof result !== "string" || !gate.isCurrent(ticket)) return;
      setPath(result); setReport(null); setPreview(null); setMapping({}); setPage(0);
      await readPreview(result, {}, ticket);
    } catch (error) { if (gate.isCurrent(ticket)) setOperationError(String(error)); }
    finally { if (gate.isCurrent(ticket)) { working.current = false; setBusy(false); } }
  };
  const exportTemplate = async () => {
    const result = await save({ defaultPath: "栈藏-Fab批量导入模板.xlsx", filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }] });
    if (result) { await api.exportTemplate(result); notify("Excel 导入模板已保存"); }
  };
  const commit = async () => {
    if (working.current || policy.busy || !translationSettings || !preview?.validCount) return;
    if (autoTranslate && !translationSettings?.configured && !window.confirm("尚未配置在线翻译凭据。继续导入会保存原文，并把缺失译文标记为待补译，是否继续？")) return;
    working.current = true; const ticket = gate.begin(); setBusy(true); setOperationError("");
    try { setProgress(null); const result = await api.commitImport(path, mapping, autoTranslate, ...binding); if (!gate.isCurrent(ticket)) return; setReport(result); setPage(0); onImported(); const pending = result.rows.filter(row => row.status === "translationPending").length; notify(`已导入 ${result.imported} 条素材 · 双语完成 ${result.imported - pending} · 待补译 ${pending}`); }
    catch (error) { if (gate.isCurrent(ticket)) setOperationError(String(error)); }
    finally { if (gate.isCurrent(ticket)) { working.current = false; setBusy(false); } }
  };
  const retryPending = async () => {
    const ids = report?.rows.filter(row => row.status === "translationPending" && row.assetId).map(row => row.assetId!) ?? [];
    if (!ids.length || working.current) return;
    working.current = true; const ticket = gate.begin(); setBusy(true); setOperationError("");
    try {
      const settings = await api.getTranslationSettings();
      if (!gate.isCurrent(ticket)) return;
      if (!settings.configured) { notify("请先在设置中心配置在线翻译凭据，再重试待补译素材", true); return; }
      const byId = new Map<string, BatchTranslationRow>();
      for (let start = 0; start < ids.length; start += 5000) {
        const result = await api.batchTranslateMissingAssets(ids.slice(start, start + 5000), ...binding);
        if (!gate.isCurrent(ticket)) return;
        for (const row of result.rows) byId.set(row.assetId, row);
      }
      setReport(previous => previous && ({ ...previous, rows: previous.rows.map(row => {
        const update = row.assetId ? byId.get(row.assetId) : undefined;
        return update ? { ...row, status: update.status === "complete" || update.status === "skipped" ? "imported" as const : "translationPending" as const, messages: update.messages } : row;
      }) }));
      onImported();
    } catch (error) { if (gate.isCurrent(ticket)) setOperationError(`重试补译失败：${String(error)}`); }
    finally { if (gate.isCurrent(ticket)) { working.current = false; setBusy(false); } }
  };

  useEffect(() => { let active = true; let dispose: (() => void) | undefined; void listen<ImportProgress>("fab-import-progress", event => { if (active && working.current) setProgress(event.payload); }).then(value => { if (active) dispose = value; else value(); }).catch(() => undefined); return () => { active = false; dispose?.(); }; }, []);
  useEffect(() => { const changed = () => { gate.invalidate(); onClose(); }; window.addEventListener("library-location-changed", changed); return () => window.removeEventListener("library-location-changed", changed); }, [gate, onClose]);

  const fields = mapping.fab_url || preview?.suggestedMapping.fab_url ? simpleFields : legacyFields;
  const canCommit = Boolean(mapping.fab_url || mapping.name || mapping.name_zh || mapping.name_en);
  const visibleRows = report?.rows ?? preview?.rows ?? [];
  const pendingCount = report?.rows.filter(row => row.status === "translationPending").length ?? 0;

  return <div className="modal-backdrop"><section className="import-modal" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">批量录入</span><h2>导入 Excel / CSV</h2></div><button className="icon-button" aria-label="收起导入窗口" onClick={close}><X size={19} /></button></header>
    <div className="import-body"><ImportTaskMap current="table" initiallyOpen />
      {(operationError || policy.error) && <p className="form-warning" role="alert">{operationError || policy.error}</p>}
      {busy && !progress && <p role="status">正在处理，请稍候…</p>}
      {!path ? <div className="import-drop"><FileSpreadsheet size={42} /><h3>选择素材表格</h3><p>支持 Fab 简化模板和旧版多列表格；单条与批量导入共用自动补齐中英双语设置。</p><div><button className="primary-button" disabled={busy} onClick={choose}><Upload size={16} />选择文件</button><button className="secondary-button" onClick={exportTemplate}><Download size={16} />下载 Excel 模板</button></div></div> : <>
        <div className="selected-file"><FileSpreadsheet size={20} /><span title={path}>{path.split(/[\\/]/).at(-1)}</span><button disabled={busy || policy.busy} onClick={choose}>更换文件</button></div>
        <div className="import-translation-option"><label><input type="checkbox" disabled={busy || policy.busy || !translationSettings} checked={autoTranslate} onChange={event => void policy.toggle(event.target.checked)} />自动补齐中英双语</label><span>{autoTranslate ? translationSettings?.configured ? "已配置在线翻译；仅补缺失字段，不覆盖已有译文。" : "尚未确认翻译凭据；继续前会提示，未完成的字段将标记待补译。" : "本次不请求翻译；缺失字段仍会标记待补译。"}</span></div>
        {autoTranslate && <p className="translation-estimate">Fab 原文读取后才可预估字符与 Token；导入会逐条翻译，缓存命中不重复请求。本机 DeepSeek 月度上限 {(translationSettings?.preferences?.deepseekMonthlyLimit ?? 1_000_000).toLocaleString("zh-CN")} Token，当前已计入 {(translationSettings?.usage?.deepseekTokens ?? 0).toLocaleString("zh-CN")}；达到上限会保留原文并标记待补译。</p>}
        <h3>{fields === simpleFields ? "Fab 简化列映射" : "旧版列映射"}</h3>{fields === simpleFields && <p className="import-hint">Fab 单元格可填写完整 URL，也可使用网页标题作为显示文字并把真实地址设为 Excel 超链接。</p>}<div className="mapping-grid">{fields.map(field => <label key={field.key}><span>{field.label}{field.required && " *"}</span><select disabled={busy || !!report} value={mapping[field.key] || ""} onChange={e => { const next = { ...mapping, [field.key]: e.target.value }; setMapping(next); void inspect(path, next); }}><option value="">不导入</option>{preview?.headers.map(header => <option key={header} value={header}>{header}</option>)}</select></label>)}</div>
        {preview && <><div className="import-summary"><span className="valid"><CheckCircle2 size={15} />有效 {preview.validCount}</span><span className="warning"><AlertCircle size={15} />警告 {preview.warningCount}</span><span className="warning"><AlertCircle size={15} />重复 {preview.duplicateCount}</span><span className="error"><AlertCircle size={15} />错误 {preview.errorCount}</span></div>
          {report && <h3>逐行导入结果</h3>}<div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>行</th><th>名称</th><th>分类</th><th>状态</th><th>说明</th></tr></thead><tbody>{visibleRows.slice(page * 100, (page + 1) * 100).map(row => <tr key={row.row}><td>{row.row}</td><td>{row.name || "—"}</td><td>{row.actualCategoryPath || row.suggestedCategoryPath || "导入时识别"}</td><td><span className={`status ${row.status}`}>{row.status === "valid" ? "有效" : row.status === "warning" ? "警告" : row.status === "duplicate" ? "重复" : row.status === "imported" ? "已导入" : row.status === "translationPending" ? "待补译" : row.status === "skipped" ? "已跳过" : "错误"}</span></td><td>{row.messages.join("；") || "—"}</td></tr>)}</tbody></table></div>{visibleRows.length > 0 && <div className="result-pagination"><button disabled={page === 0} onClick={() => setPage(value => value - 1)}>上一页</button><span>第 {page + 1} / {Math.ceil(visibleRows.length / 100)} 页 · 共 {visibleRows.length} 条</span><button disabled={(page + 1) * 100 >= visibleRows.length} onClick={() => setPage(value => value + 1)}>下一页</button></div>}</>}
        {report && <div className="report-banner import-task-status">导入完成：新增 {report.imported}，双语完成 {report.imported - pendingCount}，待补译 {pendingCount}，跳过 {report.skipped}，失败 {report.failed}</div>}
        {busy && progress && <div className="import-live-progress import-task-status" key={progress.phase}><div><span style={{ width: `${progress.total ? progress.current / progress.total * 100 : 0}%` }} /></div><strong>{progress.current} / {progress.total} · {progress.phase}</strong><small>{progress.currentName || "正在完成导入"}</small><p>成功 {progress.imported} · 跳过 {progress.skipped} · 失败 {progress.failed}</p></div>}
      </>}
    </div>
    <footer className="modal-footer">{report && pendingCount > 0 && <button className="secondary-button" disabled={busy} onClick={() => void retryPending()}>重试待补译 {pendingCount} 条</button>}<button className="secondary-button" disabled={busy} onClick={close}>{report ? "完成" : "取消"}</button>{path && !report && <button className="primary-button" disabled={busy || policy.busy || !translationSettings || !canCommit || !preview?.validCount} onClick={commit}><Upload size={16} />{busy ? "正在逐条处理…" : `导入 ${preview?.validCount || 0} 条`}</button>}</footer>
  </section></div>;
}
