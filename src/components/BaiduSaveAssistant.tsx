import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, ChevronLeft, CloudUpload, ExternalLink, Folder, RefreshCw, SkipForward, X } from "lucide-react";
import { api } from "../lib/api";
import type { BaiduNetdiskFolder, BaiduNetdiskSettings, BaiduSaveTask, BaiduTransferResult } from "../types";

const SHARE_PERMISSION_DOC = "https://pan.baidu.com/union/doc/%E5%9F%BA%E7%A1%80%E7%BD%91%E7%9B%98%E6%9C%8D%E5%8A%A1/%E6%96%87%E4%BB%B6%E5%88%86%E4%BA%AB%E6%9C%8D%E5%8A%A1%E6%96%B0/%E5%88%86%E4%BA%AB%E6%96%87%E4%BB%B6%E8%BD%AC%E5%AD%98/";

interface Props { tasks: BaiduSaveTask[]; selectedCount: number; onClose: () => void; notify: (message: string, error?: boolean) => void }

function parentPath(path: string) {
  if (path === "/") return "/";
  const parts = path.split("/").filter(Boolean);
  parts.pop();
  return parts.length ? `/${parts.join("/")}` : "/";
}

const wait = (milliseconds: number) => new Promise(resolve => window.setTimeout(resolve, milliseconds));

export function BaiduSaveAssistant({ tasks, selectedCount, onClose, notify }: Props) {
  const [settings, setSettings] = useState<BaiduNetdiskSettings | null>(null);
  const [path, setPath] = useState("/");
  const [folders, setFolders] = useState<BaiduNetdiskFolder[]>([]);
  const [loadingFolders, setLoadingFolders] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [results, setResults] = useState<Record<string, BaiduTransferResult>>({});
  const [manualIndex, setManualIndex] = useState(0);
  const [manualDone, setManualDone] = useState(0);

  const loadFolders = async (nextPath: string) => {
    setLoadingFolders(true);
    try { setFolders(await api.listBaiduNetdiskFolders(nextPath)); setPath(nextPath); }
    catch (error) { notify(String(error), true); }
    finally { setLoadingFolders(false); }
  };

  useEffect(() => {
    void api.getBaiduNetdiskSettings().then(value => {
      setSettings(value);
      const initialPath = value.defaultPath || "/";
      setPath(initialPath);
      if (value.connected) void loadFolders(initialPath);
    }).catch(error => notify(String(error), true));
  }, []);

  const directCount = Object.values(results).filter(item => item.status === "success" || item.status === "submitted").length;
  const failedCount = Object.values(results).filter(item => item.status === "failed").length;
  const pendingResults = useMemo(() => Object.values(results).filter(item => item.status === "submitted"), [results]);
  const permissionError = Object.values(results).some(item => item.message.includes("13998") || item.message.includes("文件分享服务"));

  const choosePath = async () => {
    try { const next = await api.setBaiduNetdiskDefaultPath(path); setSettings(next); notify(`默认转存目录已设为 ${path}`); }
    catch (error) { notify(String(error), true); }
  };

  const transferAll = async () => {
    if (!settings?.connected) { notify("请先在设置 → 在线服务中登录百度网盘", true); return; }
    if (!tasks.length) { notify("所选素材都没有可用的百度网盘链接", true); return; }
    setProcessing(true);
    let submitted = 0;
    try {
      await api.setBaiduNetdiskDefaultPath(path);
      for (const task of tasks) {
        try {
          let result = await api.transferBaiduSaveTask(task.id, path);
          setResults(previous => ({ ...previous, [task.id]: result }));
          if (result.status !== "failed") submitted += 1;
          for (let attempt = 0; result.status === "submitted" && attempt < 120; attempt += 1) {
            await wait(attempt === 0 ? 800 : 2000);
            try {
              result = await api.queryBaiduTransferTask(task.id, result.taskId);
              setResults(previous => ({ ...previous, [task.id]: result }));
            } catch (error) {
              if (attempt === 119) notify(`任务仍在网盘处理中：${String(error)}`, true);
            }
          }
          if (result.status === "submitted") {
            notify("当前转存任务处理时间较长，已暂停后续队列；稍后刷新状态再继续。", true);
            break;
          }
        } catch (error) {
          const message = String(error);
          setResults(previous => ({ ...previous, [task.id]: { assetId: task.id, taskId: "", status: "failed", message, savedCount: 0 } }));
          if (message.includes("正在进行") || message.includes("13998") || message.includes("文件分享服务")) break;
        }
      }
      notify(`已提交 ${submitted} 个转存任务${tasks.length !== submitted ? `，${tasks.length - submitted} 个失败` : ""}`);
    } finally { setProcessing(false); }
  };

  const refreshStatuses = async () => {
    if (!pendingResults.length) return;
    setProcessing(true);
    try {
      for (const result of pendingResults) {
        try {
          const next = await api.queryBaiduTransferTask(result.assetId, result.taskId);
          setResults(previous => ({ ...previous, [result.assetId]: next }));
        } catch (error) { notify(`任务状态暂不可用：${String(error)}`, true); }
      }
    } finally { setProcessing(false); }
  };

  const current = tasks[manualIndex];
  const manualNext = (completed: boolean) => { if (completed) setManualDone(value => value + 1); setManualIndex(value => value + 1); };
  const openCurrent = async () => { if (!current) return; try { await api.openShare(current.id); notify(current.extractionCode ? "分享页已打开，提取码已复制" : "分享页已打开"); } catch (error) { notify(String(error), true); } };

  return <div className="modal-backdrop"><section className="baidu-assistant" role="dialog" aria-modal="true">
    <header className="modal-header"><div><span className="eyebrow">BAIDU NETDISK</span><h2>网盘保存助手</h2></div><button className="icon-button" onClick={onClose}><X size={18} /></button></header>
    <div className="baidu-assistant-body">
      <div className={`netdisk-status ${settings?.connected ? "connected" : ""}`}><CloudUpload size={20} /><div><strong>{settings?.connected ? `已连接${settings.accountName ? `：${settings.accountName}` : ""}` : "尚未登录百度网盘"}</strong><span>{settings?.connected ? `选择保存路径后可直接转存 ${tasks.length} 个素材` : "请先前往设置 → 在线服务配置应用并完成官方授权"}</span></div></div>
      {settings?.connected && <section className="folder-picker"><header><button className="icon-button" disabled={path === "/" || loadingFolders} onClick={() => void loadFolders(parentPath(path))}><ChevronLeft size={16} /></button><strong title={path}>{path}</strong><button className="secondary-button" disabled={loadingFolders} onClick={() => void loadFolders(path)}><RefreshCw size={14} />刷新</button></header><div className="folder-list">{loadingFolders ? <p>正在读取网盘目录…</p> : folders.length ? folders.map(folder => <button key={folder.path} onClick={() => void loadFolders(folder.path)}><Folder size={17} /><span>{folder.name}</span></button>) : <p>此目录没有子文件夹</p>}</div><footer><span>当前保存目录：<strong>{path}</strong></span><button className="secondary-button" onClick={() => void choosePath()}>设为默认路径</button></footer></section>}
      <div className="assistant-note">直接转存使用百度官方开放平台接口，不会把账号密码交给本软件。没有开放平台权限或转存失败时，仍可使用下方“打开分享页”手动处理。</div>
      {permissionError && <div className="netdisk-permission-warning"><strong>当前应用未开通文件分享服务权限</strong><span>请确认设置中的 App ID 是当前百度网盘开放平台应用的数字 ID，并为该应用申请“文件分享服务”权限；权限开通后建议重新授权账号。</span><button className="secondary-button" onClick={() => void api.openExternal(SHARE_PERMISSION_DOC)}><ExternalLink size={14} />查看官方权限说明</button></div>}
      {Object.keys(results).length > 0 && <div className="transfer-summary"><strong>任务结果</strong><span>已提交/成功 {directCount} · 失败 {failedCount} · 尚未处理 {tasks.length - Object.keys(results).length}</span>{tasks.map(task => results[task.id] && <div key={task.id} className={`transfer-result ${results[task.id].status}`}><span>{task.name}</span><small>{results[task.id].message}</small></div>)}</div>}
      {current && <div className="manual-fallback"><small>手动处理 {manualIndex + 1} / {tasks.length}</small><strong>{current.name}</strong><span>{current.shareUrl}</span>{current.extractionCode && <code>提取码：{current.extractionCode}</code>}<div><button className="secondary-button" onClick={() => manualNext(false)}><SkipForward size={14} />跳过</button><button className="secondary-button" onClick={() => void openCurrent()}><ExternalLink size={14} />打开分享页</button><button className="secondary-button" onClick={() => manualNext(true)}><CheckCircle2 size={14} />已完成</button></div></div>}
      {!current && tasks.length > 0 && <p className="empty-mini">手动队列完成：确认 {manualDone} 项，跳过 {tasks.length - manualDone} 项。</p>}
      {selectedCount > tasks.length && <small>另有 {selectedCount - tasks.length} 个素材没有百度网盘链接，已跳过。</small>}
    </div>
    <footer className="modal-footer"><button className="secondary-button" onClick={onClose}>关闭</button><span className="grow" />{pendingResults.length > 0 && <button className="secondary-button" disabled={processing} onClick={() => void refreshStatuses()}><RefreshCw size={14} />刷新任务状态</button>}<button className="primary-button" disabled={!settings?.connected || processing || !tasks.length} onClick={() => void transferAll()}><CloudUpload size={15} />{processing ? "正在处理…" : "全部直接转存"}</button></footer>
  </section></div>;
}
