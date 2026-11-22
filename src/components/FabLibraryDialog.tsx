import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Library, X } from "lucide-react";
import { api } from "../lib/api";
import { useFabTranslationSettings } from "../lib/bilingual";
import { ImportTaskMap } from "./ImportTaskMap";
import type { FabAccountStatus, ImportProgress, ImportReport, OwnedFabPreview, OwnedFabRow } from "../types";

interface Props {
  onClose: () => void;
  onImported: () => void;
  notify: (message: string, error?: boolean) => void;
  expectedLibraryId?: string;
}

type Filter = "new" | "duplicate" | "skipped" | "all";

const VISIBLE_LIMIT = 500;

export function FabLibraryDialog({ onClose, onImported, notify, expectedLibraryId }: Props) {
  const translation = useFabTranslationSettings();
  const autoTranslate = translation.settings?.fabAutoTranslate ?? false;
  const [account, setAccount] = useState<FabAccountStatus | null>(null);
  const [preview, setPreview] = useState<OwnedFabPreview | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState<Filter>("new");
  const [busy, setBusy] = useState("");
  const [progress, setProgress] = useState("");
  const [report, setReport] = useState<ImportReport | null>(null);

  useEffect(() => {
    let active = true;
    void api.fabAccountStatus().then(status => { if (active) setAccount(status); }).catch(error => { if (active) notify(String(error), true); });
    return () => { active = false; };
  }, [notify]);

  useEffect(() => {
    let active = true;
    let disposeSync: (() => void) | undefined;
    let disposeImport: (() => void) | undefined;
    void listen<{ phase: string; fetched: number }>("fab-library-sync", event => {
      if (active) setProgress(`${event.payload.phase} · ${event.payload.fetched.toLocaleString("zh-CN")} 条`);
    }).then(value => { if (active) disposeSync = value; else value(); }).catch(() => undefined);
    void listen<ImportProgress>("fab-import-progress", event => {
      if (active) setProgress(`${event.payload.phase} · ${event.payload.current}/${event.payload.total} ${event.payload.currentName}`);
    }).then(value => { if (active) disposeImport = value; else value(); }).catch(() => undefined);
    return () => { active = false; disposeSync?.(); disposeImport?.(); };
  }, []);

  const visible = useMemo(() => {
    const rows = preview?.rows.filter(row => filter === "all" || row.status === filter) ?? [];
    return rows.slice(0, VISIBLE_LIMIT);
  }, [preview, filter]);
  const hidden = Math.max(0, (preview?.rows.filter(row => filter === "all" || row.status === filter).length ?? 0) - visible.length);

  const login = async (fresh: boolean) => {
    setBusy("login");
    setReport(null);
    try {
      if (fresh && account?.signedIn) await api.logoutFabAccount();
      const status = await api.beginFabLogin();
      setAccount(status);
      setPreview(null);
      notify(status.displayName ? `已登录 ${status.displayName}` : "已登录 Epic");
    } catch (error) {
      const message = String(error);
      if (message !== "已取消登录") notify(message, true);
    } finally { setBusy(""); }
  };

  const logout = async () => {
    setBusy("logout");
    try {
      await api.logoutFabAccount();
      setAccount({ signedIn: false, displayName: "", message: "尚未登录" });
      setPreview(null);
      setSelected(new Set());
      notify("已退出本机的 Fab 登录");
    } catch (error) { notify(String(error), true); }
    finally { setBusy(""); }
  };

  const loadLibrary = async () => {
    setBusy("load");
    setReport(null);
    setProgress("正在读取已购库");
    try {
      const next = await api.previewOwnedFabLibrary();
      setPreview(next);
      setSelected(new Set(next.rows.filter(row => row.status === "new").map(row => row.listingId)));
      setFilter(next.newCount ? "new" : "all");
      setAccount(current => ({ signedIn: true, displayName: next.displayName, message: current?.message || "" }));
      setProgress("");
    } catch (error) { notify(String(error), true); setProgress(""); }
    finally { setBusy(""); }
  };

  const toggle = (row: OwnedFabRow) => {
    if (row.status !== "new") return;
    setSelected(current => {
      const next = new Set(current);
      if (next.has(row.listingId)) next.delete(row.listingId); else next.add(row.listingId);
      return next;
    });
  };

  const importSelected = async () => {
    if (!preview) return;
    const urls = preview.rows.filter(row => row.status === "new" && selected.has(row.listingId)).map(row => row.url);
    if (!urls.length) return;
    setBusy("import");
    setProgress("正在准备导入");
    try {
      const path = await api.stageOwnedFabImport(urls);
      const result = await api.commitImport(path, { fab_url: "fab_url" }, autoTranslate, expectedLibraryId);
      setReport(result);
      setProgress("");
      onImported();
      notify(`已导入 ${result.imported} 条，跳过 ${result.skipped} 条，失败 ${result.failed} 条`, result.failed > 0);
    } catch (error) { notify(String(error), true); setProgress(""); }
    finally { setBusy(""); }
  };

  return <div className="modal-backdrop"><section className="import-modal" role="dialog" aria-modal="true" aria-label="同步已购 Fab">
    <header className="modal-header"><div><span className="eyebrow">EPIC · FAB</span><h2>同步已购 Fab</h2></div><button className="icon-button" aria-label="关闭同步已购 Fab" onClick={onClose}><X size={19} /></button></header>
    <div className="quick-body">
      <ImportTaskMap current="fabLibrary" />
      <p>在弹出的 Epic 窗口里自己完成登录和二次验证。密码不会进入栈藏，登录凭证保存在 Windows 凭据管理器。读取的是这个账号 Fab 里带 UE 版本的已入库商品，含已领取的免费商品；愿望单和没有 UE 版本的网页商品不会出现。</p>
      <p>{account?.signedIn ? `已登录 ${account.displayName || "Epic 账号"}` : account?.message || "正在确认登录状态…"}</p>
      <div className="fab-library-actions">
        <button className="secondary-button" disabled={!!busy} onClick={() => void login(account?.signedIn === true)}><Library size={16} />{account?.signedIn ? "重新登录" : "登录 Epic"}</button>
        {account?.signedIn && <button className="secondary-button" disabled={!!busy} onClick={() => void logout()}>退出登录</button>}
        <button className="primary-button" disabled={!!busy || !account?.signedIn} onClick={() => void loadLibrary()}>{busy === "load" ? "正在读取…" : "读取已购库"}</button>
      </div>
      {progress && <p role="status">{progress}</p>}
      {preview && <>
        <p>Epic 返回 {preview.fetched.toLocaleString("zh-CN")} 条。新的 {preview.newCount.toLocaleString("zh-CN")}，已在栈藏 {preview.duplicateCount.toLocaleString("zh-CN")}，无法录入 {preview.skippedCount.toLocaleString("zh-CN")}。已有素材不会被修改。</p>
        <div className="fab-library-actions" role="tablist" aria-label="筛选已购结果">
          <button className={filter === "new" ? "primary-button" : "secondary-button"} onClick={() => setFilter("new")}>新的</button>
          <button className={filter === "duplicate" ? "primary-button" : "secondary-button"} onClick={() => setFilter("duplicate")}>已有</button>
          <button className={filter === "skipped" ? "primary-button" : "secondary-button"} onClick={() => setFilter("skipped")}>无法录入</button>
          <button className="secondary-button" disabled={!!busy} onClick={() => setSelected(new Set(preview.rows.filter(row => row.status === "new").map(row => row.listingId)))}>全选新的</button>
          <button className="secondary-button" disabled={!!busy} onClick={() => setSelected(new Set())}>清除选择</button>
        </div>
        <div className="fab-library-list">
          {visible.length === 0 && <p>这一组里没有商品。</p>}
          {visible.map(row => <label key={row.listingId || `${row.status}:${row.title}:${row.url}`}>
            <input type="checkbox" disabled={row.status !== "new" || !!busy} checked={row.status === "new" && selected.has(row.listingId)} onChange={() => toggle(row)} />
            <span>{row.title}</span>
            <small>{row.status === "new" ? "新的" : row.message}</small>
          </label>)}
        </div>
        {hidden > 0 && <p>还有 {hidden.toLocaleString("zh-CN")} 条未展开。导入仍按上面的选择，不限于当前展开的行。</p>}
      </>}
      {report && <p>导入完成：写入 {report.imported}，跳过 {report.skipped}，失败 {report.failed}。详细结果在任务中心。</p>}
    </div>
    <footer className="modal-footer">
      <button className="secondary-button" onClick={onClose}>关闭</button>
      <button className="primary-button" disabled={!!busy || selected.size === 0 || !preview} onClick={() => void importSelected()}>{busy === "import" ? "正在导入…" : `导入 ${selected.size.toLocaleString("zh-CN")} 条新商品`}</button>
    </footer>
  </section></div>;
}
