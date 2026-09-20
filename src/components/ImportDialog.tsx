import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CheckCircle2, Download, FileSpreadsheet, Upload, X } from "lucide-react";
import { api } from "../lib/api";
import { listen } from "@tauri-apps/api/event";
import type { ImportMapping, ImportPreview, ImportProgress, ImportReport } from "../types";

interface Props { onClose: () => void; onImported: () => void; notify: (message: string, error?: boolean) => void }

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

export function ImportDialog({ onClose, onImported, notify }: Props) {
  const [path, setPath] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [mapping, setMapping] = useState<ImportMapping>({});
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);

  const inspect = async (selectedPath: string, nextMapping: ImportMapping = {}) => {
    setBusy(true);
    try {
      const result = await api.inspectImport(selectedPath, nextMapping);
      setPreview(result); setMapping(Object.keys(nextMapping).length ? nextMapping : result.suggestedMapping);
    } catch (error) { notify(String(error), true); } finally { setBusy(false); }
  };
  const choose = async () => {
    const result = await open({ multiple: false, filters: [{ name: "表格", extensions: ["xlsx", "csv"] }] });
    if (typeof result === "string") { setPath(result); setReport(null); await inspect(result); }
  };
  const exportTemplate = async () => {
    const result = await save({ defaultPath: "栈藏-Fab批量导入模板.xlsx", filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }] });
    if (result) { await api.exportTemplate(result); notify("Excel 导入模板已保存"); }
  };
  const commit = async () => {
    setBusy(true);
    try { setProgress(null); const result = await api.commitImport(path, mapping); setReport(result); onImported(); notify(`成功导入 ${result.imported} 条素材`); }
    catch (error) { notify(String(error), true); } finally { setBusy(false); }
  };

  useEffect(() => { if (path && preview) { const timer = setTimeout(() => inspect(path, mapping), 250); return () => clearTimeout(timer); } }, [mapping]);
  useEffect(() => { let dispose: (() => void) | undefined; void listen<ImportProgress>("fab-import-progress", event => setProgress(event.payload)).then(value => { dispose = value; }); return () => dispose?.(); }, []);

  const fields = mapping.fab_url || preview?.suggestedMapping.fab_url ? simpleFields : legacyFields;
  const canCommit = Boolean(mapping.fab_url || mapping.name || mapping.name_zh || mapping.name_en);

  return <div className="modal-backdrop"><section className="import-modal" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">批量录入</span><h2>导入 Excel / CSV</h2></div><button className="icon-button" onClick={onClose}><X size={19} /></button></header>
    <div className="import-body">
      {!path ? <div className="import-drop"><FileSpreadsheet size={42} /><h3>选择素材表格</h3><p>支持直接 URL，也支持显示标题但链接到 Fab 页面的 Excel 超链接；启用 Fab 自动翻译后会逐条补齐中文。</p><div><button className="primary-button" onClick={choose}><Upload size={16} />选择文件</button><button className="secondary-button" onClick={exportTemplate}><Download size={16} />下载 Excel 模板</button></div></div> : <>
        <div className="selected-file"><FileSpreadsheet size={20} /><span title={path}>{path.split(/[\\/]/).at(-1)}</span><button onClick={choose}>更换文件</button></div>
        <h3>{fields === simpleFields ? "Fab 简化列映射" : "旧版列映射"}</h3>{fields === simpleFields && <p className="import-hint">Fab 单元格可填写完整 URL，也可使用网页标题作为显示文字并把真实地址设为 Excel 超链接。批量导入遵循设置中的 Fab 自动翻译开关。</p>}<div className="mapping-grid">{fields.map(field => <label key={field.key}><span>{field.label}{field.required && " *"}</span><select value={mapping[field.key] || ""} onChange={e => setMapping(prev => ({ ...prev, [field.key]: e.target.value }))}><option value="">不导入</option>{preview?.headers.map(header => <option key={header} value={header}>{header}</option>)}</select></label>)}</div>
        {preview && <><div className="import-summary"><span className="valid"><CheckCircle2 size={15} />有效 {preview.validCount}</span><span className="warning"><AlertCircle size={15} />警告 {preview.warningCount}</span><span className="warning"><AlertCircle size={15} />重复 {preview.duplicateCount}</span><span className="error"><AlertCircle size={15} />错误 {preview.errorCount}</span></div>
          <div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>行</th><th>名称</th><th>分类</th><th>状态</th><th>说明</th></tr></thead><tbody>{preview.rows.slice(0, 100).map(row => <tr key={row.row}><td>{row.row}</td><td>{row.name || "—"}</td><td>{row.actualCategoryPath || row.suggestedCategoryPath || "导入时识别"}</td><td><span className={`status ${row.status}`}>{row.status === "valid" ? "有效" : row.status === "warning" ? "警告" : row.status === "duplicate" ? "重复" : row.status === "imported" ? "已导入" : row.status === "skipped" ? "已跳过" : "错误"}</span></td><td>{row.messages.join("；") || "—"}</td></tr>)}</tbody></table></div></>}
        {report && <div className="report-banner">导入完成：新增 {report.imported}，跳过 {report.skipped}，失败 {report.failed}</div>}
        {busy && progress && <div className="import-live-progress"><div><span style={{ width: `${progress.total ? progress.current / progress.total * 100 : 0}%` }} /></div><strong>{progress.current} / {progress.total} · {progress.phase}</strong><small>{progress.currentName || "正在完成导入"}</small><p>成功 {progress.imported} · 跳过 {progress.skipped} · 失败 {progress.failed}</p></div>}
      </>}
    </div>
    <footer className="modal-footer"><button className="secondary-button" onClick={onClose}>{report ? "完成" : "取消"}</button>{path && !report && <button className="primary-button" disabled={busy || !canCommit || !preview?.validCount} onClick={commit}><Upload size={16} />{busy ? "正在逐条处理…" : `导入 ${preview?.validCount || 0} 条`}</button>}</footer>
  </section></div>;
}
