import { Component, type ReactNode } from "react";
import { recordDiagnostic } from "../lib/diagnostics";

export function LoadFeedback({ label = "正在读取…", skeleton = false }: { label?: string; skeleton?: boolean }) {
  return <div className="load-feedback" role="status" aria-live="polite" aria-busy="true"><span className="load-indicator" aria-hidden="true"/><span>{label}</span>{skeleton && <div className="load-skeleton" aria-hidden="true">{[0,1,2].map(i=><i key={i}/>)}</div>}</div>;
}

export function ReadFailure({ onRetry }: { onRetry: () => void }) {
  return <div className="read-failure" role="alert"><span>读取失败，已有内容仍保留。</span><button className="secondary-button" onClick={onRetry}>重新读取</button></div>;
}

export class WorkspaceBoundary extends Component<{children:ReactNode}, {failed:boolean}> {
  state = { failed:false };
  static getDerivedStateFromError() { return {failed:true}; }
  componentDidCatch() { recordDiagnostic("render-failure"); }
  render() {
    if(this.state.failed) return <section className="workspace-recovery" role="alert"><h2>界面暂时无法显示</h2><p>不会自动删除或覆盖素材库。请先重试；若问题仍在，请重启应用并检查恢复入口。</p><button className="primary-button" onClick={()=>this.setState({failed:false})}>重试显示</button><button className="secondary-button" onClick={()=>window.location.reload()}>重新加载界面</button></section>;
    return this.props.children;
  }
}
