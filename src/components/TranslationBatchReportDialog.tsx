import { Languages, RefreshCw, X } from "lucide-react";
import type { BatchTranslationReport, ImportProgress } from "../types";

interface Props {
  report: BatchTranslationReport | null;
  progress: ImportProgress | null;
  busy: boolean;
  onClose: () => void;
  onRetry: (ids: string[]) => void;
}

export function TranslationBatchReportDialog({ report, progress, busy, onClose, onRetry }: Props) {
  const retryIds = report?.rows.filter(row => row.status === "pending").map(row => row.assetId) ?? [];
  return <div className="modal-backdrop"><section className="import-modal translation-batch-modal" role="dialog" aria-modal="true" aria-label="双语补译结果">
    <header className="modal-header"><div><span className="eyebrow">ONLINE TRANSLATION</span><h2><Languages size={20} />双语补译</h2></div><button className="icon-button" disabled={busy} onClick={onClose} aria-label="关闭补译结果"><X size={19} /></button></header>
    <div className="import-body">
      {busy && <div className="import-live-progress"><div><span style={{ width: `${progress?.total ? progress.current / progress.total * 100 : 0}%` }} /></div><strong>{progress ? `${progress.current} / ${progress.total} · ${progress.phase}` : "正在准备补译…"}</strong><small>{progress?.currentName || "逐条处理选中素材"}</small></div>}
      {report && <><div className="report-banner">双语完成 {report.completed} · 待补译 {report.pending} · 无需处理 {report.skipped}</div><div className="preview-table-wrap"><table className="preview-table"><thead><tr><th>素材</th><th>结果</th><th>说明</th></tr></thead><tbody>{report.rows.slice(0, 500).map(row => <tr key={row.assetId}><td>{row.name || row.assetId}</td><td><span className={`status ${row.status === "complete" ? "imported" : row.status === "pending" ? "translationPending" : "skipped"}`}>{row.status === "complete" ? "已补齐" : row.status === "pending" ? "待补译" : "已跳过"}</span></td><td>{row.messages.join("；") || "—"}</td></tr>)}</tbody></table></div>{report.rows.length > 500 && <p className="import-hint">仅显示前 500 行，共 {report.rows.length} 行。</p>}</>}
    </div>
    <footer className="modal-footer">{retryIds.length > 0 && <button className="secondary-button" disabled={busy} onClick={() => onRetry(retryIds)}><RefreshCw size={15} />重试待补译 {retryIds.length} 项</button>}<button className="primary-button" disabled={busy} onClick={onClose}>完成</button></footer>
  </section></div>;
}
