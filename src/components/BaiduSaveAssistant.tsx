import { useState } from "react";
import { CheckCircle2, ExternalLink, SkipForward, X } from "lucide-react";
import { api } from "../lib/api";
import type { BaiduSaveTask } from "../types";

interface Props { tasks: BaiduSaveTask[]; selectedCount: number; onClose: () => void; notify: (message: string, error?: boolean) => void }

export function BaiduSaveAssistant({ tasks, selectedCount, onClose, notify }: Props) {
  const [index, setIndex] = useState(0);
  const [done, setDone] = useState(0);
  const current = tasks[index];
  const next = (completed: boolean) => { if (completed) setDone(value => value + 1); setIndex(value => value + 1); };
  const openCurrent = async () => { if (!current) return; try { await api.openShare(current.id); notify(current.extractionCode ? "分享页已打开，提取码已复制" : "分享页已打开"); } catch (error) { notify(String(error), true); } };
  const finished = index >= tasks.length;
  return <div className="modal-backdrop"><section className="baidu-assistant" role="dialog" aria-modal="true"><header className="modal-header"><div><span className="eyebrow">BAIDU SAVE ASSISTANT</span><h2>网盘保存助手</h2></div><button className="icon-button" onClick={onClose}><X size={18} /></button></header>
    <div className="baidu-assistant-body">{finished ? <div className="assistant-finished"><CheckCircle2 size={44} /><h3>队列处理完成</h3><p>已确认完成 {done} 项，跳过 {tasks.length - done} 项；无可用百度链接 {selectedCount - tasks.length} 项。</p></div> : <><div className="assistant-progress"><span style={{ width: `${tasks.length ? index / tasks.length * 100 : 100}%` }} /></div><small>{index + 1} / {tasks.length} · 使用系统浏览器当前登录的百度账号</small><h3>{current.name}</h3><p>{current.shareUrl}</p>{current.extractionCode && <code>提取码：{current.extractionCode}</code>}<div className="assistant-note">软件不会登录或切换百度账号。请在打开的官方页面完成转存或下载，再返回这里继续。</div></>}</div>
    <footer className="modal-footer"><button className="secondary-button" onClick={onClose}>{finished ? "完成" : "关闭"}</button>{!finished && <><button className="secondary-button" onClick={() => next(false)}><SkipForward size={15} />跳过</button><button className="secondary-button" onClick={() => void openCurrent()}><ExternalLink size={15} />打开分享页</button><button className="primary-button" onClick={() => next(true)}><CheckCircle2 size={15} />已完成并继续</button></>}</footer>
  </section></div>;
}
