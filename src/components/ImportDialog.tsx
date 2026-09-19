import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CheckCircle2, Download, FileSpreadsheet, Upload, X } from "lucide-react";
import { api } from "../lib/api";
import type { ImportMapping, ImportPreview, ImportReport } from "../types";

interface Props { onClose: () => void; onImported: () => void; notify: (message: string, error?: boolean) => void }

const fields: { key: string; label: string; required?: boolean }[] = [
  { key: "name_zh", label: "中文名称（至少一种）" }, { key: "name_en", label: "英文名称（至少一种）" }, { key: "name", label: "旧版素材名称" }, { key: "share_url", label: "分享链接", required: true },
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
    const result = await save({ defaultPath: "DCC素材导入模板.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (result) { await api.exportTemplate(result); notify("导入模板已保存"); }
  };
  const commit = async () => {
    setBusy(true);
    try { const result = await api.commitImport(path, mapping); setReport(result); onImported(); notify(`成功导入 ${result.imported} 条素材`); }
    catch (error) { notify(String(error), true); } finally { setBusy(false); }
  };

  useEffect(() => { if (path && preview) { const timer = setTimeout(() => inspect(path, mapping), 250); return () => clearTimeout(timer); } }, [mapping]);

  return <div className="modal-backdrop"><section className="import-modal" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">批量录入</span><h2>导入 Excel / CSV</h2></div><button className="icon-button" onClick={onClose}><X size={19} /></button></header>
    <div className="import-body">
      {!path ? <div className="import-drop"><FileSpreadsheet size={42} /><h3>选择素材表格</h3><p>支持 .xlsx 和 UTF-8 .csv，可在下一步映射列名。</p><div><button className="primary-button" onClick={choose}><Upload size={16} />选择文件</button><button className="secondary-button" onClick={exportTemplate}><Download size={16} />下载模板</button></div></div> : <>
        <div className="selected-file"><FileSpreadsheet size={20} /><span title={path}>{path.split(/[\\/]/).at(-1)}</span><button onClick={choose}>更换文件</button></div>
        <h3>列映射</h3><div className="mapping-grid">{fields.map(field => <label key={field.key}><span>{field.label}{field.required && " *"}</span><select value={mapping[field.key] || ""} onChange={e => setMapping(prev => ({ ...prev, [field.key]: e.target.value }))}><option value="">不导入</option>{preview?.headers.map(header => <option key={header} value={header}>{header}</option>)}</select></label>)}</div>
        {preview && <><div className="import-summary"><span className="valid"><CheckCircle2 size={15} />有效 {preview.validCount}</span><span className="warning"><AlertCircle size={15} />警告 {preview.warningCount}</span><span className="error"><AlertCircle size={15} />错误 {preview.errorCount}</span></div>
          <div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>行</th><th>名称</th><th>状态</th><th>说明</th></tr></thead><tbody>{preview.rows.slice(0, 100).map(row => <tr key={row.row}><td>{row.row}</td><td>{row.name || "—"}</td><td><span className={`status ${row.status}`}>{row.status === "valid" ? "有效" : row.status === "warning" ? "警告" : "错误"}</span></td><td>{row.messages.join("；") || "—"}</td></tr>)}</tbody></table></div></>}
        {report && <div className="report-banner">导入完成：新增 {report.imported}，跳过 {report.skipped}，失败 {report.failed}</div>}
      </>}
    </div>
    <footer className="modal-footer"><button className="secondary-button" onClick={onClose}>{report ? "完成" : "取消"}</button>{path && !report && <button className="primary-button" disabled={busy || !mapping.name || !mapping.share_url || !preview?.validCount} onClick={commit}><Upload size={16} />{busy ? "正在处理…" : `导入 ${preview?.validCount || 0} 条`}</button>}</footer>
  </section></div>;
}
