import { AlertTriangle, CheckCircle2, RefreshCw, ShieldCheck, Square } from "lucide-react";
import type { HealthSummary, LinkCheckProgress } from "../types";

interface Props { summary: HealthSummary | null; activeIssue: string | null; loading: boolean; linkCheckRunning: boolean; linkProgress: LinkCheckProgress | null; onSelect: (issue: string | null) => void; onRefresh: () => void; onCheckLinks: () => void; onCancelLinkCheck: () => void }

export function HealthPanel({ summary, activeIssue, loading, linkCheckRunning, linkProgress, onSelect, onRefresh, onCheckLinks, onCancelLinkCheck }: Props) {
  const percent = linkProgress?.total ? Math.round(linkProgress.checked / linkProgress.total * 100) : 0;
  return <section className="health-panel">
    <div className="health-intro"><div className={summary?.totalIssues ? "health-status warning" : "health-status good"}>{summary?.totalIssues ? <AlertTriangle size={20} /> : <CheckCircle2 size={20} />}</div><div><strong>{summary?.totalIssues ? `${summary.totalIssues} 个待整理项` : "素材库状态良好"}</strong><span>本地项目会自动统计；网盘链接只在你手动检查时联网验证。</span></div><div className="health-actions"><button className="secondary-button" onClick={linkCheckRunning ? onCancelLinkCheck : onCheckLinks}>{linkCheckRunning ? <Square size={14} /> : <ShieldCheck size={15} />}{linkCheckRunning ? "停止检查" : "检查全部网盘链接"}</button><button className="icon-button subtle" onClick={onRefresh} title="重新统计"><RefreshCw size={15} className={loading ? "spinning" : ""} /></button></div></div>
    {linkCheckRunning && <div className="link-check-progress"><div><span>正在检查百度网盘链接</span><strong>{linkProgress?.checked || 0} / {linkProgress?.total || 0} · {percent}%</strong></div><div className="progress-track"><span style={{ width: `${percent}%` }} /></div><small>有效 {linkProgress?.valid || 0} · 失效 {linkProgress?.invalid || 0} · 无法判断 {linkProgress?.error || 0}</small></div>}
    <div className="health-chips"><button className={!activeIssue ? "active" : ""} onClick={() => onSelect(null)}>全部素材</button>{summary?.counts.map(item => <button key={item.issue} className={activeIssue === item.issue ? "active" : ""} onClick={() => onSelect(item.issue)}><span>{item.label}</span><strong>{item.count}</strong></button>)}</div>
  </section>;
}
