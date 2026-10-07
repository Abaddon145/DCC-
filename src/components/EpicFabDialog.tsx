import { open } from "@tauri-apps/plugin-dialog";
import { Copy, ExternalLink, FolderOpen, RefreshCw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { knowledgeApi } from "../lib/knowledge";
import { useRequestGate } from "../lib/workspace";
import type { FabUeOptions, UeProjectTarget } from "../types";

export function ueVersionRelation(packageVersion: string, projectVersion: string | null): "same" | "older" | "newer" | "unknown" {
  const parse = (value: string) => /^(\d+)\.(\d+)(?:\.\d+)?$/.exec(value);
  const a = parse(packageVersion), b = parse(projectVersion || "");
  if (!a || !b) return "unknown";
  const difference = Number(a[1]) - Number(b[1]) || Number(a[2]) - Number(b[2]);
  return difference === 0 ? "same" : difference < 0 ? "older" : "newer";
}

interface Props { assetId: string; title: string; productName?: string; onClose: () => void }
export function EpicFabDialog({ assetId, title, productName = title, onClose }: Props) {
  const gate = useRequestGate();
  const dialog = useRef<HTMLElement>(null);
  const context = useRef<{ libraryId: string; enabled: boolean } | null>(null);
  const [options, setOptions] = useState<FabUeOptions | null>(null);
  const [targets, setTargets] = useState<UeProjectTarget[]>([]);
  const [selectedPath, setSelectedPath] = useState("");
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState<"loading" | "picking" | "opening" | "fallback" | null>("loading");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const target = targets.find(item => item.path === selectedPath);
  const relation = ueVersionRelation(version, target?.engineVersion || null);
  const canOpen = !!context.current && !!options && !!target?.available && relation !== "newer" && (!options.supportedVersions.length || !!version) && !busy;
  const close = () => { gate.invalidate(); context.current = null; onClose(); };

  const remember = (path: string, selectedVersion: string) => {
    if (!context.current?.enabled) return;
    try { localStorage.setItem(`dcc-epic-add:${context.current.libraryId}`, JSON.stringify({ path, version: selectedVersion })); } catch { /* Preference storage must not block Epic. */ }
  };
  const load = async () => {
    const ticket = gate.begin(); setBusy("loading"); setError(""); setMessage("");
    try {
      const value = await api.getWorkspaceContext("library");
      if (!gate.isCurrent(ticket)) return;
      context.current = value;
      const [fab, projects] = await Promise.allSettled([api.getFabUeOptions(assetId, value.libraryId), api.listUeProjectTargets(value.libraryId)]);
      if (!gate.isCurrent(ticket)) return;
      let recent: { path?: string; version?: string } = {};
      if (value.enabled) try { const raw = JSON.parse(localStorage.getItem(`dcc-epic-add:${value.libraryId}`) || "{}"); if (raw && typeof raw === "object") recent = { path: typeof raw.path === "string" ? raw.path : undefined, version: typeof raw.version === "string" ? raw.version : undefined }; } catch { /* Ignore damaged optional preferences. */ }
      const projectList = projects.status === "fulfilled" ? projects.value : [];
      if (recent.path && !projectList.some(item => item.path === recent.path)) {
        try { projectList.push(await api.inspectUeProject(recent.path, value.libraryId)); } catch { /* A moved project can be located again. */ }
        if (!gate.isCurrent(ticket)) return;
      }
      setTargets(previous => [...projectList, ...previous.filter(item => !projectList.some(project => project.path === item.path))]);
      const choice = projectList.find(item => item.path === recent.path && item.available) || projectList.find(item => item.available);
      setSelectedPath(previous => previous || choice?.path || "");
      if (fab.status === "fulfilled") {
        setOptions(fab.value);
        const versions = fab.value.supportedVersions;
        setVersion(previous => versions.includes(previous) ? previous : versions.includes(recent.version || "") ? recent.version! : versions.find(item => ueVersionRelation(item, choice?.engineVersion || null) === "same") || versions[0] || "");
      } else { setOptions(null); setVersion(""); }
      const failures = [fab, projects].filter(item => item.status === "rejected").map(item => String((item as PromiseRejectedResult).reason));
      setError(failures.join("；"));
    } catch (reason) { if (gate.isCurrent(ticket)) { context.current = null; setError(String(reason)); } }
    finally { if (gate.isCurrent(ticket)) setBusy(null); }
  };
  useEffect(() => {
    const previousFocus = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const changed = close;
    window.addEventListener("library-location-changed", changed);
    void load();
    return () => { gate.invalidate(); window.removeEventListener("library-location-changed", changed); previousFocus?.focus(); };
  }, [assetId]);
  const chooseProject = async () => {
    const bound = context.current; if (!bound) return;
    const ticket = gate.begin(); setBusy("picking"); setError(""); setMessage("");
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Unreal Engine 工程", extensions: ["uproject"] }] });
      if (!path || Array.isArray(path) || !gate.isCurrent(ticket)) return;
      const result = await api.inspectUeProject(path, bound.libraryId);
      if (!gate.isCurrent(ticket)) return;
      setTargets(previous => [...previous.filter(item => item.path !== result.path), result]); setSelectedPath(result.path);
      const same = options?.supportedVersions.find(item => ueVersionRelation(item, result.engineVersion) === "same");
      if (same) setVersion(same);
    } catch (reason) { if (gate.isCurrent(ticket)) setError(String(reason)); }
    finally { if (gate.isCurrent(ticket)) setBusy(null); }
  };
  const launch = async () => {
    if (!canOpen || !context.current || !target) return;
    const bound = context.current; const ticket = gate.begin(); setBusy("opening"); setError(""); setMessage("");
    try {
      const checked = await api.inspectUeProject(target.path, bound.libraryId);
      if (!gate.isCurrent(ticket)) return;
      setTargets(previous => previous.map(item => item.path === checked.path ? checked : item));
      if (!checked.available || ueVersionRelation(version, checked.engineVersion) === "newer") throw new Error("工程不可用或版本已变更，请重新核对目标工程。");
      const result = await api.openFabInEpic(assetId, bound.libraryId);
      if (!gate.isCurrent(ticket)) return;
      if (result.requested) { remember(checked.path, version); setMessage(result.message); }
      else setError(result.message);
    } catch (reason) { if (gate.isCurrent(ticket)) setError(String(reason)); }
    finally { if (gate.isCurrent(ticket)) setBusy(null); }
  };
  const fallback = async (action: "copy" | "web") => {
    const bound = context.current;
    if (!bound || busy || (action === "web" && !options)) return;
    const ticket = gate.begin(); setBusy("fallback"); setError(""); setMessage("");
    try {
      if (action === "copy") await knowledgeApi(bound.libraryId).writeClipboard(productName);
      else await api.openExternal(options!.canonicalUrl);
      if (gate.isCurrent(ticket)) setMessage(action === "copy" ? "商品名已复制，可在 Epic 的 Fab 库中搜索。" : "已请求打开官方商品页，可在页面核对购买记录与支持版本。");
    } catch (reason) { if (gate.isCurrent(ticket)) setError(String(reason)); }
    finally { if (gate.isCurrent(ticket)) setBusy(null); }
  };
  return <div className="modal-backdrop" onPointerDown={event => { if (event.target === event.currentTarget) close(); }}>
    <section ref={dialog} className="modal-card epic-fab-dialog" role="dialog" aria-modal="true" aria-label="通过 Epic 添加到 UE 工程" onKeyDown={event => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
      if (event.key === "Tab") { const buttons = dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled),select:not(:disabled)"); if (!buttons?.length) return; const first = buttons[0], last = buttons[buttons.length - 1]; if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); } }
    }}>
      <header><div><span className="eyebrow">EPIC · FAB</span><h2>添加到 UE 工程</h2></div><button className="icon-button" aria-label="关闭 Epic 添加窗口" onClick={close}><X size={18}/></button></header>
      <div className="epic-fab-body">
        <p className="epic-asset-title">{title}</p>
        <p>这里核对工程与版本，再定位 Epic 中的 Fab 商品。请在商品页点击“添加到工程”并确认目标工程；需使用已购买此素材的 Epic 账号。</p>
        {busy === "loading" && <p role="status"><RefreshCw size={14} className="spinning"/>正在读取 Fab 支持版本和已登记工程…</p>}
        <label>目标 UE 工程<select disabled={!!busy} value={selectedPath} onChange={event => { setSelectedPath(event.target.value); setMessage(""); const next = targets.find(item => item.path === event.target.value); const same = options?.supportedVersions.find(item => ueVersionRelation(item, next?.engineVersion || null) === "same"); if (same) setVersion(same); }}><option value="">请选择工程</option>{targets.map(item => <option key={item.path} value={item.path} disabled={!item.available}>{item.projectName ? `${item.projectName} · ` : ""}{item.name}{item.available ? "" : "（路径失效）"}</option>)}</select></label>
        <button className="secondary-button" disabled={!!busy || !context.current} onClick={() => void chooseProject()}><FolderOpen size={15}/>{busy === "picking" ? "正在检查…" : "选择其他 .uproject"}</button>
        {target && <div className="epic-project-info"><span>{target.path}</span><strong>工程版本：{target.engineVersion || target.engineAssociation || "未知，请在 Epic 中核对"}</strong>{target.warnings.map((warning, index) => <span key={index}>{warning}</span>)}</div>}
        <label>Fab 官方支持版本<select disabled={!!busy || !options?.supportedVersions.length} value={version} onChange={event => { setVersion(event.target.value); setMessage(""); }}><option value="">{options?.supportedVersions.length ? "请选择版本" : "暂未获得官方版本信息"}</option>{options?.supportedVersions.map(item => <option value={item} key={item} disabled={ueVersionRelation(item, target?.engineVersion || null) === "newer"}>UE {item}{ueVersionRelation(item, target?.engineVersion || null) === "newer" ? "（高于工程版本）" : ""}</option>)}</select></label>
        <button className="secondary-button" disabled={!!busy} onClick={() => void load()}><RefreshCw size={14}/>重新读取官方信息</button>
        {options && options.versionStatus !== "known" && !busy && <p className="form-warning">官方版本信息不完整，不会使用素材中手填的版本代替。可重试，或打开 Epic 后手动核对支持版本。</p>}
        {options?.warnings.map((warning, index) => <p className="form-warning" key={index}>{warning}</p>)}
        {target && relation === "older" && <p className="form-warning">所选素材版本低于工程版本，允许继续，但可能需要转换或出现兼容性问题。请先备份工程。</p>}
        {target && relation === "newer" && <p className="form-warning">素材版本高于工程版本，不能继续。请选择更早的官方版本或更高版本的工程。</p>}
        <p className="form-warning">工程路径和所选版本不会自动传给 Epic。请在 Epic 中再次选择目标工程与版本；插件或完整工程可能显示“安装插件”或“创建工程”。</p>
        <p>定位请求不代表 Epic 已显示商品或开始安装。如果仍停在库首页，可重试定位、复制商品名在 Fab 库搜索，或手动打开官方商品页。</p>
        <div className="epic-fallback-actions">
          <button className="secondary-button" disabled={!canOpen} onClick={() => void launch()}><RefreshCw size={14}/>重试定位</button>
          <button className="secondary-button" disabled={!!busy || !context.current} onClick={() => void fallback("copy")}><Copy size={14}/>复制商品名</button>
          <button className="secondary-button" disabled={!!busy || !context.current || !options} onClick={() => void fallback("web")}><ExternalLink size={14}/>打开官方商品页</button>
        </div>
        {error && <p role="alert" className="form-error">{error}</p>}
        {message && <p role="status">{message}</p>}
      </div>
      <footer><button className="secondary-button" onClick={close}>关闭</button><button className="primary-button" disabled={!canOpen} onClick={() => void launch()}><ExternalLink size={16}/>{busy === "opening" ? "正在发出定位请求…" : "定位 Fab 商品"}</button></footer>
    </section>
  </div>;
}
